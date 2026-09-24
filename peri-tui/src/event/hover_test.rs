use super::*;
use crate::app::MessageViewModel;
use crate::ui::{main_ui::message_area::render_messages, render_thread::WrappedLineInfo};
use ratatui::crossterm::event::MouseEvent;
use ratatui::{layout::Rect, text::Line};

fn make_mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[tokio::test]
async fn test_scrollbar_hover_edge_drag_leave_and_focus_loss() {
    let (mut app, mut handle) = App::new_headless(80, 20).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("长历史".into()));
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.width = 79;
        cache.total_lines = 1020;
        cache.lines = (0..1020).map(|i| Line::from(format!("历史 {i}"))).collect();
        cache.wrap_map = (0..1020)
            .map(|i| WrappedLineInfo {
                line_idx: i,
                visual_row_start: i,
                visual_row_end: i + 1,
                plain_text: format!("历史 {i}"),
                char_widths: vec![],
            })
            .collect();
    }
    app.session_mgr.current_mut().ui.scroll_follow = false;
    app.session_mgr.current_mut().ui.scroll_offset = 500;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("绘制隐藏状态");
    let hidden = handle.snapshot();
    let text_area = app.session_mgr.current().ui.messages_area;
    assert!(
        app.session_mgr
            .current()
            .ui
            .message_scrollbar_metrics
            .is_none(),
        "初始隐藏"
    );
    assert!(
        handle_event(&mut app, make_mouse(MouseEventKind::Moved, 30, 5))
            .await
            .expect("事件成功")
            .is_none(),
        "正文移动不重绘"
    );
    assert!(
        matches!(
            handle_event(&mut app, make_mouse(MouseEventKind::Moved, 79, 5))
                .await
                .expect("事件成功"),
            Some(Action::Redraw)
        ),
        "最右列必须唤出滚动条"
    );
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("绘制显示状态");
    assert_eq!(
        app.session_mgr.current().ui.messages_area,
        text_area,
        "显隐不改变正文宽度"
    );
    let thumb = app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_metrics
        .expect("应显示滑块")
        .thumb_area;
    assert_eq!(
        handle.terminal.backend().buffer()[(79, thumb.y)].symbol(),
        "█"
    );
    for column in [78, 77, 79] {
        assert!(
            handle_event(&mut app, make_mouse(MouseEventKind::Moved, column, 5))
                .await
                .expect("事件成功")
                .is_none(),
            "热区内移动无需重绘"
        );
    }
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Down(MouseButton::Left), 79, thumb.y),
    )
    .await
    .expect("抓住滑块");
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Drag(MouseButton::Left), 40, thumb.y + 1),
    )
    .await
    .expect("拖出热区");
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("拖拽期间绘制");
    assert!(
        app.session_mgr
            .current()
            .ui
            .message_scrollbar_metrics
            .is_some(),
        "拖拽离开热区不能消失"
    );
    assert!(app.session_mgr.current().ui.scroll_offset > 500);
    handle_event(
        &mut app,
        make_mouse(MouseEventKind::Up(MouseButton::Left), 40, thumb.y + 1),
    )
    .await
    .expect("释放滑块");
    app.session_mgr.current_mut().ui.scroll_offset = 500;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("离开隐藏");
    assert_eq!(handle.snapshot(), hidden, "隐藏后不留残影");
    handle_event(&mut app, make_mouse(MouseEventKind::Moved, 79, 5))
        .await
        .expect("再次进入");
    handle_event(&mut app, Event::FocusLost)
        .await
        .expect("失去焦点");
    assert!(!app.session_mgr.current().ui.scrollbar_hover);
    assert!(!app.session_mgr.current().ui.message_scrollbar_dragging);
}

#[tokio::test]
async fn test_scrollbar_hover_short_content_never_appears() {
    let (mut app, mut handle) = App::new_headless(80, 20).await;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("绘制空会话");
    assert!(
        handle_event(&mut app, make_mouse(MouseEventKind::Moved, 79, 5))
            .await
            .expect("事件成功")
            .is_none()
    );
    assert!(app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_area
        .is_none());
}
