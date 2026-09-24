use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
use std::sync::atomic::AtomicUsize;

fn make_hover(row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 79,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn test_input_pump_stalled_consumer_coalesces_one_hundred_thousand_moves() {
    let consumed = Arc::new(AtomicUsize::new(0));
    let progress = consumed.clone();
    let pump = InputPump::with_source(move |timeout| {
        let n = progress.load(Ordering::Acquire);
        if n < 100_000 {
            progress.fetch_add(1, Ordering::Release);
            Ok(Some(make_hover((n % 20) as u16)))
        } else {
            thread::sleep(timeout);
            Ok(None)
        }
    })
    .expect("创建输入线程");
    let deadline = Instant::now() + Duration::from_secs(5);
    while consumed.load(Ordering::Acquire) < 100_000 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        consumed.load(Ordering::Acquire),
        100_000,
        "主线程不消费时仍应持续排空输入"
    );
    // source 的计数早于入队，等待最后一次入队完成。
    loop {
        let queue = pump.shared.queue.lock();
        if queue.events.back() == Some(&make_hover(19)) {
            break;
        }
        drop(queue);
        assert!(Instant::now() < deadline, "最终位置必须入队");
        thread::yield_now();
    }
    assert_eq!(
        pump.shared.queue.lock().events.len(),
        1,
        "悬停事件不能无限增长"
    );
    assert_eq!(
        pump.next(Duration::ZERO).expect("读取成功"),
        Some(make_hover(19))
    );
}

#[test]
fn test_input_pump_keeps_keyboard_click_wheel_paste_and_resize_order() {
    let mut queue = Queue::default();
    let mut click = match make_hover(2) {
        Event::Mouse(m) => m,
        _ => unreachable!(),
    };
    click.kind = MouseEventKind::Down(MouseButton::Left);
    let mut wheel = click;
    wheel.kind = MouseEventKind::ScrollUp;
    let boundaries = vec![
        Event::Key(KeyEvent::new(KeyCode::Char('中'), KeyModifiers::NONE)),
        Event::Mouse(click),
        Event::Mouse(wheel),
        Event::Paste("多行\n粘贴".into()),
        Event::Resize(80, 24),
    ];
    let mut expected = Vec::new();
    for boundary in boundaries {
        for event in [make_hover(1), make_hover(2), boundary.clone()] {
            if !queue.coalesce_hover(&event) {
                queue.events.push_back(event);
            }
        }
        expected.extend([make_hover(2), boundary]);
    }
    assert_eq!(
        queue.events.into_iter().collect::<Vec<_>>(),
        expected,
        "不能跨输入边界合并"
    );
}

#[test]
fn test_input_pump_pause_and_drop_do_not_steal_editor_input_or_deadlock() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let guard = pause_input();
    let pump = InputPump::with_source(move |_| {
        count.fetch_add(1, Ordering::Relaxed);
        Ok(Some(make_hover(1)))
    })
    .expect("创建输入线程");
    thread::sleep(Duration::from_millis(30));
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "编辑器独占期间不得读取终端"
    );
    drop(pump);
    drop(guard);
}

#[test]
fn test_input_pump_full_queue_shutdown_and_error_delivery() {
    let pump = InputPump::with_source(|_| Ok(Some(Event::FocusGained))).expect("创建输入线程");
    let deadline = Instant::now() + Duration::from_secs(5);
    while pump.shared.queue.lock().events.len() < MAX_QUEUED_EVENTS {
        assert!(Instant::now() < deadline, "队列应达到上限");
        thread::sleep(Duration::from_millis(1));
    }
    drop(pump);
    let pump =
        InputPump::with_source(|_| Err(io::Error::other("测试读取错误"))).expect("创建输入线程");
    assert!(
        pump.next(Duration::from_secs(1)).is_err(),
        "读取错误应交回主循环"
    );
}

#[test]
fn test_input_pump_stop_is_idempotent_and_releases_input_before_drop() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let mut pump = InputPump::with_source(move |timeout| {
        count.fetch_add(1, Ordering::Relaxed);
        thread::sleep(timeout);
        Ok(None)
    })
    .expect("创建输入线程");
    pump.stop();
    let stopped = calls.load(Ordering::Relaxed);
    thread::sleep(Duration::from_millis(30));
    pump.stop();
    assert_eq!(
        calls.load(Ordering::Relaxed),
        stopped,
        "退出钩子执行前必须停止读取终端"
    );
}
