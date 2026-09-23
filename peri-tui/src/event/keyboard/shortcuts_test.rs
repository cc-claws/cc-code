use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;

fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

#[tokio::test]
async fn test_shortcuts_ctrl_p_与_alt_p_均触发命令面板() {
    let mut app = App::new().await;

    // Arrange: 确保命令面板未打开
    assert!(
        !app.session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::CommandPalette),
        "初始状态命令面板应未打开"
    );

    // Act: 发送 Alt+P
    let alt_p = make_key_event(KeyCode::Char('p'), KeyModifiers::ALT);
    let action = handle_shortcuts(&mut app, &alt_p);
    assert!(
        matches!(action, Some(Action::Redraw)),
        "Alt+P 应返回 Some(Action::Redraw)"
    );
    assert!(
        app.session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::CommandPalette),
        "Alt+P 应成功打开命令面板"
    );

    // Act: 再次发送 Alt+P 关闭
    let action = handle_shortcuts(&mut app, &alt_p);
    assert!(
        matches!(action, Some(Action::Redraw)),
        "再次 Alt+P 应返回 Some(Action::Redraw)"
    );
    assert!(
        !app.session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::CommandPalette),
        "再次 Alt+P 应关闭命令面板"
    );

    // Act: 发送 Ctrl+P 打开
    let ctrl_p = make_key_event(KeyCode::Char('p'), KeyModifiers::CONTROL);
    let action = handle_shortcuts(&mut app, &ctrl_p);
    assert!(
        matches!(action, Some(Action::Redraw)),
        "Ctrl+P 应返回 Some(Action::Redraw)"
    );
    assert!(
        app.session_mgr
            .current()
            .session_panels
            .is_active(PanelKind::CommandPalette),
        "Ctrl+P 应成功打开命令面板"
    );
}

#[tokio::test]
async fn test_shortcuts_ctrl_t_已被完全移除() {
    let mut app = App::new().await;

    // Ctrl+T 按键
    let ctrl_t = make_key_event(KeyCode::Char('t'), KeyModifiers::CONTROL);
    let action = handle_shortcuts(&mut app, &ctrl_t);

    // Assert: handle_shortcuts 不再处理 Ctrl+T，返回 None（留给后续输入或静默忽略）
    assert!(
        action.is_none(),
        "Ctrl+T 不应再被 handle_shortcuts 拦截处理"
    );
}
