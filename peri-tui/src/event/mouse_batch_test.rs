use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::collections::VecDeque;

fn make_mouse(kind: MouseEventKind, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 10,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn make_batch(queue: &mut VecDeque<Event>) -> Vec<Event> {
    let first = queue.pop_front().expect("测试队列不能为空");
    let mut pending = None;
    let batch = collect_mouse_batch(first, &mut pending, || Ok(queue.pop_front()))
        .expect("模拟队列读取应成功");
    if let Some(event) = pending {
        queue.push_front(event);
    }
    batch
}

#[test]
fn test_mouse_batch_wheel_burst_preserves_distance_and_direction() {
    let mut queue: VecDeque<_> =
        std::iter::repeat_n(make_mouse(MouseEventKind::ScrollDown, 5), 100)
            .chain(std::iter::repeat_n(
                make_mouse(MouseEventKind::ScrollUp, 6),
                20,
            ))
            .collect();
    let expected: Vec<_> = queue.iter().cloned().collect();
    let batch = make_batch(&mut queue);
    assert_eq!(
        batch, expected,
        "120 个事件应在一次重绘前按序处理，不能丢失距离、方向和位置"
    );
    assert!(queue.is_empty());
}

#[test]
fn test_mouse_batch_drag_keeps_final_position_before_release() {
    let final_drag = make_mouse(MouseEventKind::Drag(MouseButton::Left), 8);
    let release = make_mouse(MouseEventKind::Up(MouseButton::Left), 8);
    let key = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let mut queue = VecDeque::from([
        make_mouse(MouseEventKind::Drag(MouseButton::Left), 4),
        final_drag.clone(),
        release.clone(),
        key.clone(),
    ]);
    let batch = make_batch(&mut queue);
    assert_eq!(batch, vec![final_drag], "释放前必须先绘制最终拖拽坐标");
    assert_eq!(queue.pop_front(), Some(release));
    assert_eq!(queue.pop_front(), Some(key), "后续按键应保留在输入队列");
}

#[test]
fn test_mouse_batch_stops_at_keyboard_boundary() {
    let wheel = make_mouse(MouseEventKind::ScrollUp, 4);
    let key = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let mut queue = VecDeque::from([wheel.clone(), key.clone(), wheel.clone()]);
    assert_eq!(make_batch(&mut queue), vec![wheel.clone()]);
    assert_eq!(
        queue.pop_front(),
        Some(key),
        "按键应延迟到下一批，保证事件顺序"
    );
    assert_eq!(
        queue.pop_front(),
        Some(wheel),
        "不能越过键盘事件继续消费鼠标"
    );
}

#[test]
fn test_mouse_batch_bounds_continuous_input() {
    let mut queue = VecDeque::from(vec![make_mouse(MouseEventKind::ScrollDown, 1); 300]);
    assert_eq!(make_batch(&mut queue).len(), MAX_MOUSE_BATCH);
    assert_eq!(
        queue.len(),
        300 - MAX_MOUSE_BATCH,
        "事件积压不能饿死主循环其他任务"
    );
}

#[test]
fn test_mouse_batch_does_not_drain_after_key() {
    let key = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let mut queue = VecDeque::from([key.clone(), make_mouse(MouseEventKind::ScrollDown, 1)]);
    assert_eq!(make_batch(&mut queue), vec![key]);
    assert_eq!(queue.len(), 1);
}

#[tokio::test]
async fn test_mouse_batch_long_history_matches_individual_scrolls_with_one_redraw() {
    use crate::app::MessageViewModel;
    use crate::ui::{main_ui::message_area::render_messages, render_thread::WrappedLineInfo};
    use crate::{
        app::App,
        event::{handle_event, handle_event_batch, Action},
    };
    use ratatui::{layout::Rect, text::Line};
    let (mut app, mut handle) = App::new_headless(100, 30).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.width = 99;
        cache.lines = (0..10_000)
            .map(|i| Line::from(format!("历史行 {i}")))
            .collect();
        cache.wrap_map = (0..10_000)
            .map(|i| WrappedLineInfo {
                line_idx: i,
                visual_row_start: i,
                visual_row_end: i + 1,
                plain_text: format!("历史行 {i}"),
                char_widths: vec![],
            })
            .collect();
        cache.total_lines = 10_000;
    }
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scrollbar_max_offset = 10_000;
    ui.scroll_offset = 500;
    ui.scroll_follow = false;
    let mut queue: VecDeque<_> = std::iter::repeat_n(make_mouse(MouseEventKind::ScrollUp, 5), 100)
        .chain(std::iter::repeat_n(
            make_mouse(MouseEventKind::ScrollDown, 5),
            20,
        ))
        .collect();
    let mut old_redraws = 0;
    for event in queue.iter().cloned() {
        if matches!(
            handle_event(&mut app, event).await.expect("旧路径应成功"),
            Some(Action::Redraw)
        ) {
            old_redraws += 1;
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("旧路径渲染应成功");
        }
    }
    let expected_offset = app.session_mgr.current().ui.scroll_offset;
    let expected_frame = handle.snapshot();
    app.session_mgr.current_mut().ui.scroll_offset = 500;
    let action = handle_event_batch(&mut app, make_batch(&mut queue))
        .await
        .expect("批处理应成功");
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("批处理渲染应成功");
    assert_eq!(old_redraws, 120, "旧路径每个滚轮事件均触发重绘");
    assert!(
        matches!(action, Some(Action::Redraw)),
        "新路径整批只返回一次重绘"
    );
    assert_eq!(app.session_mgr.current().ui.scroll_offset, expected_offset);
    assert_eq!(expected_offset, 260, "长历史下滚动距离不应丢失");
    assert_eq!(
        handle.snapshot(),
        expected_frame,
        "一万行历史的最终可见内容必须一致"
    );
}
