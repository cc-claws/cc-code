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
    let default_row = ThemeName::ALL
        .iter()
        .position(|t| *t == ThemeName::Default)
        .expect("default 必在 ALL 内");
    assert_eq!(
        panel.cursor, default_row,
        "当前为 default，光标应落在 default 行"
    );
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

/// `auto` 行必须标出当前终端解析到的具体主题，否则用户选了 auto 也无从知道实际用了哪套。
#[tokio::test]
async fn test_theme_panel_auto_row_shows_resolved_theme() {
    use crate::ui::main_ui::panels::theme::render_theme_panel;
    use ratatui::{backend::TestBackend, Terminal};

    let (mut app, _handle) = App::new_headless(80, 30).await;
    let mut panel = ThemePanel::new();
    panel.cursor = ThemeName::ALL
        .iter()
        .position(|t| *t == ThemeName::Auto)
        .expect("auto 必在 ALL 内");

    let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("测试终端");
    terminal
        .draw(|frame| {
            let area = frame.area();
            render_theme_panel(frame, &panel, &mut app, area);
        })
        .expect("主题面板应正常渲染");
    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for row in 0..buffer.area.height {
        for column in 0..buffer.area.width {
            rendered.push_str(buffer[(column, row)].symbol());
        }
        rendered.push('\n');
    }

    let resolved = crate::terminal_theme::resolve();
    assert!(
        rendered.contains(&format!("follow terminal: {}", resolved.setting_value())),
        "auto 行应标出解析结果 {}，实际渲染:\n{rendered}",
        resolved.setting_value()
    );
}
