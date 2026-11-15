use super::*;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use std::collections::VecDeque;

fn make_key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// 同一次模拟粘贴的连续投递：所有事件在同一时刻到达，视为亚毫秒级连发。
fn burst(base: Instant, events: impl IntoIterator<Item = Event>) -> VecDeque<StampedEvent> {
    events
        .into_iter()
        .map(|event| StampedEvent::at(event, base))
        .collect()
}

fn events_of(pending: &VecDeque<StampedEvent>) -> Vec<Event> {
    pending
        .iter()
        .map(|stamped| stamped.event.clone())
        .collect()
}

#[test]
fn test_simulated_paste_long_text_keeps_final_enter_inside_paste() {
    let base = Instant::now();
    let mut queue = burst(
        base,
        std::iter::repeat_n(make_key(KeyCode::Char('甲')), 5000).chain([make_key(KeyCode::Enter)]),
    );
    let mut pending = VecDeque::new();
    let result = detect_simulated_paste(
        StampedEvent::at(make_key(KeyCode::Char('甲')), base),
        &mut pending,
        |_| Ok(queue.pop_front()),
    )
    .expect("检测成功");
    assert_eq!(
        result.event,
        Event::Paste(format!("{}\n", "甲".repeat(5001)))
    );
    assert!(pending.is_empty(), "尾部换行不能变成单独的提交按键");
}

#[test]
fn test_simulated_paste_preserves_mouse_and_shortcut_boundaries() {
    let base = Instant::now();
    for boundary in [
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 79,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Event::Resize(80, 24),
    ] {
        let mut queue = burst(
            base,
            [
                make_key(KeyCode::Char('好')),
                boundary.clone(),
                make_key(KeyCode::Char('后')),
            ],
        );
        let mut pending = VecDeque::new();
        let result = detect_simulated_paste(
            StampedEvent::at(make_key(KeyCode::Char('你')), base),
            &mut pending,
            |_| Ok(queue.pop_front()),
        )
        .expect("检测成功");
        assert_eq!(result.event, Event::Paste("你好".into()));
        assert_eq!(
            events_of(&pending),
            vec![boundary],
            "边界事件不能被粘贴吞掉"
        );
        assert_eq!(
            queue.pop_front().map(|stamped| stamped.event),
            Some(make_key(KeyCode::Char('后')))
        );
    }
}

#[test]
fn test_simulated_paste_multiline_and_single_key_are_preserved() {
    let base = Instant::now();
    let mut queue = burst(
        base,
        [make_key(KeyCode::Enter), make_key(KeyCode::Char('乙'))],
    );
    let mut pending = VecDeque::new();
    assert_eq!(
        detect_simulated_paste(
            StampedEvent::at(make_key(KeyCode::Char('甲')), base),
            &mut pending,
            |_| Ok(queue.pop_front())
        )
        .expect("检测成功")
        .event,
        Event::Paste("甲\n乙".into())
    );
    assert_eq!(
        detect_simulated_paste(
            StampedEvent::at(make_key(KeyCode::Enter), base),
            &mut pending,
            |_| Ok(None)
        )
        .expect("检测成功")
        .event,
        make_key(KeyCode::Enter)
    );
}

#[test]
fn test_simulated_paste_hover_between_text_and_enter_does_not_submit() {
    let hover = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 79,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    let base = Instant::now();
    let mut queue = burst(
        base,
        [
            make_key(KeyCode::Char('乙')),
            hover.clone(),
            make_key(KeyCode::Enter),
        ],
    );
    let mut pending = VecDeque::new();
    let event = detect_simulated_paste(
        StampedEvent::at(make_key(KeyCode::Char('甲')), base),
        &mut pending,
        |_| Ok(queue.pop_front()),
    )
    .expect("检测成功");
    assert_eq!(
        event.event,
        Event::Paste("甲乙\n".into()),
        "移动鼠标不能让粘贴换行变为提交"
    );
    assert_eq!(events_of(&pending), vec![hover], "悬停位置仍必须送交 UI");
}

#[test]
fn test_simulated_paste_slow_burst_stays_inside_windows() {
    // 慢终端按 0.5ms 间隔逐字投递：首对落在 1ms 起始窗口内，其余靠 15ms 空闲窗口续接。
    let base = Instant::now();
    let step = Duration::from_micros(500);
    let mut queue: VecDeque<StampedEvent> = (1u32..6)
        .map(|index| StampedEvent::at(make_key(KeyCode::Char('甲')), base + step * index))
        .collect();
    let mut pending = VecDeque::new();
    let result = detect_simulated_paste(
        StampedEvent::at(make_key(KeyCode::Char('甲')), base),
        &mut pending,
        |_| Ok(queue.pop_front()),
    )
    .expect("检测成功");
    assert_eq!(
        result.event,
        Event::Paste("甲".repeat(6)),
        "0.5ms 间隔的连发仍属同一次粘贴"
    );
    assert!(pending.is_empty(), "窗口内的续帧不能被推迟");
}

#[test]
fn test_simulated_paste_pair_beyond_start_window_is_not_merged() {
    // 2ms 间隔超出 1ms 起始窗口：按人工输入处理，后一键留给下一轮独立处理。
    let base = Instant::now();
    let mut queue = VecDeque::from([StampedEvent::at(
        make_key(KeyCode::Char('乙')),
        base + Duration::from_millis(2),
    )]);
    let mut pending = VecDeque::new();
    let result = detect_simulated_paste(
        StampedEvent::at(make_key(KeyCode::Char('甲')), base),
        &mut pending,
        |_| Ok(queue.pop_front()),
    )
    .expect("检测成功");
    assert_eq!(
        result.event,
        make_key(KeyCode::Char('甲')),
        "超窗按键必须单独处理"
    );
    assert_eq!(events_of(&pending), vec![make_key(KeyCode::Char('乙'))]);
}

#[test]
fn test_simulated_paste_queued_enter_presses_stay_separate_submits() {
    // 渲染/流式处理期间连按 Enter：多个 Enter 早已排进输入队列，但真实到达间隔
    // 远超起始窗口，必须逐个作为提交处理，不能折叠成 `[Pasted text #N]` 占位符。
    let base = Instant::now();
    let mut queue: VecDeque<StampedEvent> = [40, 180, 320, 460]
        .iter()
        .map(|offset| {
            StampedEvent::at(
                make_key(KeyCode::Enter),
                base + Duration::from_millis(*offset),
            )
        })
        .collect();
    let mut pending = VecDeque::new();
    let mut handled = Vec::new();
    for _ in 0..4 {
        // 与 next_event 一致：先取积压事件，再取输入线程队列。
        let first = pending
            .pop_front()
            .or_else(|| queue.pop_front())
            .expect("仍有待处理按键");
        handled.push(
            detect_simulated_paste(first, &mut pending, |_| Ok(queue.pop_front()))
                .expect("检测成功")
                .event,
        );
    }
    assert_eq!(
        handled,
        vec![make_key(KeyCode::Enter); 4],
        "四次独立按键必须得到四次独立提交"
    );
    assert!(
        pending.is_empty() && queue.is_empty(),
        "积压事件不能滞留或丢失"
    );
}

#[test]
fn test_simulated_paste_queued_enter_is_deferred_to_next_round() {
    // 队列里积压的旧事件必须原样放回 pending，既不能并进粘贴，也不能被丢弃。
    let base = Instant::now();
    let second = base + Duration::from_millis(250);
    let mut queue = VecDeque::from([StampedEvent::at(make_key(KeyCode::Enter), second)]);
    let mut pending = VecDeque::new();
    let result = detect_simulated_paste(
        StampedEvent::at(make_key(KeyCode::Enter), base),
        &mut pending,
        |_| Ok(queue.pop_front()),
    )
    .expect("检测成功");
    assert_eq!(
        result.event,
        make_key(KeyCode::Enter),
        "单次 Enter 不能被合并"
    );
    assert_eq!(
        events_of(&pending),
        vec![make_key(KeyCode::Enter)],
        "积压事件必须留给下一轮独立处理"
    );
    assert!(queue.is_empty());
}
