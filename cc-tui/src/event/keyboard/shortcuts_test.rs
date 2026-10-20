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
async fn test_shortcuts_ctrl_p_and_alt_p_both_trigger_command_panel() {
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
async fn test_shortcuts_ctrl_t_completely_removed() {
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

/// 回归：进入子 Agent 视图后，Shift+Tab 不应切换主会话的权限模式
#[tokio::test]
async fn test_backtab_ignored_in_subagent_view() {
    use cc_middlewares::prelude::PermissionMode;
    let mut app = App::new().await;
    app.services.permission_mode.store(PermissionMode::AutoMode);

    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "child-1",
        "agentType": "general-purpose",
        "taskPreview": "sleep",
        "isBackground": true,
    }));
    app.switch_subagent_viewport(Some("child-1".to_string()));
    assert!(
        app.session_mgr.current().active_subagent().is_some(),
        "应已进入子 agent 视图"
    );

    let _ = crate::event::keyboard::handle_key_event(
        &mut app,
        make_key_event(KeyCode::BackTab, KeyModifiers::SHIFT),
    );
    assert_eq!(
        app.services.permission_mode.load(),
        PermissionMode::AutoMode,
        "子 agent 视图下 Shift+Tab 不应切换权限模式"
    );
}
