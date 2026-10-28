use super::*;
use crate::ui::{main_ui::try_prepare_width, render_thread::RenderEvent};
use ratatui::{
    crossterm::event::{KeyEvent, MouseEvent},
    layout::Size,
};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_input_backlog_reaches_keyboard_while_layout_worker_is_pending() {
    let (mut app, _) = App::new_headless(120, 40).await;
    app.global_ui.viewport.terminal_size = Size::new(120, 40);
    app.global_ui.viewport.fullscreen_latched = false;
    let (sender, mut receiver) = mpsc::channel(8);
    app.session_mgr.current_mut().messages.render_tx = sender;
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_follow = false;
    ui.scroll_offset = 20_000;
    ui.scrollbar_max_offset = 40_000;
    assert!(!try_prepare_width(&mut app, 120, false));
    let RenderEvent::PrepareWidth { reply, .. } =
        receiver.try_recv().expect("首个准备请求应成功入队")
    else {
        panic!("应收到宽度准备请求");
    };
    let wheel = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 4,
        row: 2,
        modifiers: KeyModifiers::NONE,
    });
    let mut queue: VecDeque<_> = std::iter::repeat_n(wheel, 4095)
        .chain([Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        ))])
        .collect();
    let now = Instant::now();
    let mut scheduler = RedrawScheduler::new(Duration::from_millis(33), now);
    scheduler.finish_draw(now, true);
    let mut wheel_batches = 0;
    while let Some(first) = queue.pop_front() {
        let mut boundary = None;
        let batch =
            mouse_batch::collect_mouse_batch(first, &mut boundary, || Ok(queue.pop_front()))
                .expect("批量读取积压输入");
        if let Some(boundary) = boundary {
            queue.push_front(boundary);
        }
        wheel_batches += usize::from(matches!(batch.first(), Some(Event::Mouse(_))));
        let action = handle_event_batch(&mut app, batch)
            .await
            .expect("后台未回复时仍应正常派发输入");
        if matches!(action, Some(Action::Redraw)) {
            scheduler.request_redraw();
        }
        assert!(
            !try_prepare_width(&mut app, 120, false),
            "后台未回复时不能同步等待或重复投递"
        );
        assert!(!scheduler.is_due(now), "同一期限内合并重画请求");
    }
    assert_eq!(wheel_batches, 32, "保留既有每批最多128条的边界");
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 7715);
    assert_eq!(app.session_mgr.current().ui.textarea.lines()[0], "x");
    assert!(!reply.is_closed(), "按键应在后台确认前处理完成");
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert!(scheduler.is_due(now + Duration::from_millis(33)));
}
