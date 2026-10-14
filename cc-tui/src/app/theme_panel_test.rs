use super::*;
use crate::app::panel_manager::PanelContext;
use crate::config::PeriConfig;
use tui_textarea::Key;

fn key(k: Key) -> Input {
    Input {
        key: k,
        ctrl: false,
        alt: false,
        shift: false,
    }
}

/// 面板交互全流程：打开时光标落在当前主题；上下移动实时预览；
/// Esc 恢复原主题；Enter 确认后主题生效且持久化到配置。
#[tokio::test]
async fn test_theme_panel_preview_cancel_and_confirm() {
    theme::set_current(ThemeName::Default);
    let (mut app, _handle) = App::new_headless(80, 30).await;
    app.services.peri_config = Some(PeriConfig::default());
    // 配置保存重定向到临时文件，避免污染真实 ~/.cc-code/settings.json
    let tmp_cfg = std::env::temp_dir().join(format!(
        "cc-theme-panel-test-{}.json",
        std::process::id()
    ));
    app.services.config_path_override = Some(tmp_cfg.clone());

    let mut panel = ThemePanel::new();
    assert_eq!(panel.cursor, 0, "当前为 default，光标应落在首行");
    assert_eq!(panel.selected(), ThemeName::Default);
    assert_eq!(panel.original, ThemeName::Default);

    {
        let mut ctx = PanelContext {
            services: &mut app.services,
            session_mgr: &mut app.session_mgr,
            acp_client: None,
        };
        // 下移 → 实时预览 dark
        let r = panel.handle_key(key(Key::Down), &mut ctx);
        assert_eq!(r, EventResult::Consumed);
        assert_eq!(panel.selected(), ThemeName::Dark);
        assert_eq!(theme::current_name(), ThemeName::Dark, "移动光标应实时预览");

        // 再下移 → light；上移 → 回到 dark
        panel.handle_key(key(Key::Down), &mut ctx);
        assert_eq!(theme::current_name(), ThemeName::Light);
        panel.handle_key(key(Key::Up), &mut ctx);
        assert_eq!(theme::current_name(), ThemeName::Dark);

        // Esc → 恢复原主题并关闭
        let r = panel.handle_key(key(Key::Esc), &mut ctx);
        assert_eq!(r, EventResult::ClosePanel);
        assert_eq!(theme::current_name(), ThemeName::Default, "Esc 应恢复原主题");
    }

    // 重新打开，选到 dark 后 Enter 确认
    let mut panel = ThemePanel::new();
    {
        let mut ctx = PanelContext {
            services: &mut app.services,
            session_mgr: &mut app.session_mgr,
            acp_client: None,
        };
        panel.handle_key(key(Key::Down), &mut ctx);
        let r = panel.handle_key(key(Key::Enter), &mut ctx);
        assert_eq!(r, EventResult::ClosePanel);
    }
    assert_eq!(theme::current_name(), ThemeName::Dark, "Enter 后主题应生效");
    let cfg = app.services.peri_config.as_ref().unwrap();
    assert_eq!(cfg.config.theme.as_deref(), Some("dark"), "应写入内存配置");
    let saved = std::fs::read_to_string(&tmp_cfg).expect("配置应已落盘");
    assert!(saved.contains("\"dark\""), "落盘配置应含主题值: {saved}");

    // 光标下移不越界
    let mut panel = ThemePanel::new();
    panel.cursor = ThemeName::ALL.len() - 1;
    {
        let mut ctx = PanelContext {
            services: &mut app.services,
            session_mgr: &mut app.session_mgr,
            acp_client: None,
        };
        panel.handle_key(key(Key::Down), &mut ctx);
        assert_eq!(panel.cursor, ThemeName::ALL.len() - 1);
    }

    let _ = std::fs::remove_file(&tmp_cfg);
    theme::set_current(ThemeName::Default);
}
