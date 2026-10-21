use super::*;

#[test]
fn test_screen_selection_highlight_crosses_viewport_edges() {
    use cc_tui::app::text_selection::ScreenSelection;
    use ratatui::{buffer::Buffer, layout::Rect};
    let area = Rect::new(3, 7, 6, 2);
    for (start, end, first_column, last_column) in [((7, 5), (10, 0), 5, 8), ((4, 8), (8, 5), 3, 5)]
    {
        let mut buffer = Buffer::empty(area);
        let selection = ScreenSelection {
            start: Some(start),
            end: Some(end),
            ..Default::default()
        };
        apply_screen_selection_highlight(&mut buffer, &selection);
        let selected = cc_tui::ui::theme::cur().selection_bg;
        assert_eq!(buffer[(first_column, 7)].bg, selected);
        assert_eq!(buffer[(last_column, 8)].bg, selected);
        if first_column > area.x {
            assert_ne!(buffer[(first_column - 1, 7)].bg, selected);
        }
        if last_column + 1 < area.right() {
            assert_ne!(buffer[(last_column + 1, 8)].bg, selected);
        }
    }
}

// ── 安全加固：默认权限模式为 Auto（fail-closed） ──

#[test]
fn test_default_permission_mode_is_auto() {
    use cc_middlewares::prelude::PermissionMode;
    // 无任何参数、无 YOLO 环境变量 → Auto（启用审批）
    assert_eq!(
        resolve_initial_permission_mode(false, None, false, false, false),
        PermissionMode::AutoMode,
        "默认必须走 Auto（审批），不得默认为 Bypass"
    );
}

#[test]
fn test_explicit_yolo_enables_bypass() {
    use cc_middlewares::prelude::PermissionMode;
    // 显式 -y/--yolo
    assert_eq!(
        resolve_initial_permission_mode(false, None, false, true, false),
        PermissionMode::Bypass
    );
    // 显式 YOLO_MODE=真值
    assert_eq!(
        resolve_initial_permission_mode(false, None, false, false, true),
        PermissionMode::Bypass
    );
}

#[test]
fn test_permission_mode_priority() {
    use cc_middlewares::prelude::PermissionMode;
    // --dangerously-skip-permissions 优先级最高
    assert_eq!(
        resolve_initial_permission_mode(true, Some("auto"), false, false, false),
        PermissionMode::Bypass
    );
    // --permission-mode bypass 高于 -y
    assert_eq!(
        resolve_initial_permission_mode(false, Some("bypass"), false, false, false),
        PermissionMode::Bypass
    );
    // 未知取值回退 Auto（不意外滑进 Bypass）
    assert_eq!(
        resolve_initial_permission_mode(false, Some("whatever"), false, true, true),
        PermissionMode::AutoMode
    );
    // -a/--approve 显式选择 Auto
    assert_eq!(
        resolve_initial_permission_mode(false, None, true, false, true),
        PermissionMode::AutoMode
    );
}

#[test]
fn test_env_priority_process_over_settings() {
    // 测试进程环境变量优先于 settings.json
    // 设置一个测试环境变量
    std::env::set_var("TEST_ENV_PRIORITY_VAR", "from_process");

    // 调用注入函数（即使 settings.json 存在该变量也不应覆盖）
    inject_env_from_settings();

    // 验证进程环境变量未被覆盖
    assert_eq!(
        std::env::var("TEST_ENV_PRIORITY_VAR").unwrap(),
        "from_process"
    );

    // 清理
    std::env::remove_var("TEST_ENV_PRIORITY_VAR");
}

#[test]
fn test_read_config_env_extracts_env_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"config":{"env":{"JEV_API_KEY":"k1","JEV_ENDPOINT":"http://x"}}}"#,
    )
    .unwrap();
    let env = read_config_env(&path).unwrap();
    assert_eq!(env.get("JEV_API_KEY").map(String::as_str), Some("k1"));
    assert_eq!(
        env.get("JEV_ENDPOINT").map(String::as_str),
        Some("http://x")
    );
}

#[test]
fn test_read_config_env_missing_or_malformed() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_config_env(&dir.path().join("nope.json")).is_none());
    let p1 = dir.path().join("a.json");
    std::fs::write(&p1, r#"{"config":{}}"#).unwrap();
    assert!(read_config_env(&p1).is_none());
    let p2 = dir.path().join("b.json");
    std::fs::write(&p2, "not json").unwrap();
    assert!(read_config_env(&p2).is_none());
}

#[test]
fn test_select_exit_session_id_prefers_live_thread_id() {
    let result = select_exit_session_id(
        Some("thread-live".to_string()),
        Some("acp".to_string()),
        Some("resume".to_string()),
        Some("cli".to_string()),
    );
    assert_eq!(result.as_deref(), Some("thread-live"));
}

#[test]
fn test_select_exit_session_id_falls_back_to_resume_arg() {
    let result = select_exit_session_id(
        None,
        None,
        Some("resume-id".to_string()),
        Some("cli".to_string()),
    );
    assert_eq!(result.as_deref(), Some("resume-id"));
}
