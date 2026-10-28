use super::*;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};

fn make_mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 4,
        row: 2,
        modifiers: KeyModifiers::NONE,
    })
}

#[tokio::test]
async fn test_screen_selection_release_waits_for_pending_frame() {
    let (mut app, _) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .ui
        .screen_selection
        .start_drag(1, 1);
    let release = make_mouse(MouseEventKind::Up(MouseButton::Left));
    assert!(
        selection_needs_frame(&app, true, &release),
        "复制屏幕选区前必须先呈现最后一次拖拽"
    );
    assert!(
        !selection_needs_frame(&app, false, &release),
        "已呈现的选区可立即释放"
    );
}

#[tokio::test]
async fn test_message_selection_release_waits_for_pending_frame() {
    let (mut app, _) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .ui
        .text_selection
        .start_drag(1, 1);
    assert!(
        selection_needs_frame(
            &app,
            true,
            &make_mouse(MouseEventKind::Up(MouseButton::Left))
        ),
        "消息选区应保留先呈现再复制的顺序"
    );
}

#[tokio::test]
async fn test_selection_boundary_keeps_drag_and_keyboard_available() {
    let (mut app, _) = App::new_headless(80, 10).await;
    app.session_mgr
        .current_mut()
        .ui
        .screen_selection
        .start_drag(1, 1);
    for event in [
        make_mouse(MouseEventKind::Drag(MouseButton::Left)),
        make_mouse(MouseEventKind::Down(MouseButton::Left)),
        make_mouse(MouseEventKind::Up(MouseButton::Right)),
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    ] {
        assert!(
            !selection_needs_frame(&app, true, &event),
            "选区屏障仅约束触发复制的左键释放"
        );
    }
    app.session_mgr.current_mut().ui.screen_selection.clear();
    assert!(
        !selection_needs_frame(
            &app,
            true,
            &make_mouse(MouseEventKind::Up(MouseButton::Left))
        ),
        "取消的选区不能留下释放屏障"
    );
}
