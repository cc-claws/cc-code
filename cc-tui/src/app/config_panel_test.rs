use super::*;

fn make_lc() -> crate::i18n::LcRegistry {
    crate::i18n::LcRegistry::default()
}

#[test]
fn test_config_panel_from_config_defaults() {
    let cfg = PeriConfig::default();
    let panel = ConfigPanel::from_config(&cfg);
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);
    assert!(panel.buf_autocompact);
    assert_eq!(panel.buf_threshold, "85");
    assert!(panel.buf_recap);
    assert!(panel.buf_language.is_empty());
    assert_eq!(panel.buf_proactiveness, "medium");
    assert_eq!(panel.buf_detail_render_strategy, "auto");
}

#[test]
fn test_config_panel_cursor_navigation() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);

    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_THRESHOLD);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_RECAP);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_LANGUAGE);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_DIFF);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_HOOKS_DISPLAY);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_DETAIL_RENDER_STRATEGY);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_STREAMING);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PERSONA);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_TONE);
    // wrapping: TONE → AUTOCOMPACT
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);

    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_TONE);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_PERSONA);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_STREAMING);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_DETAIL_RENDER_STRATEGY);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_HOOKS_DISPLAY);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_DIFF);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_LANGUAGE);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_RECAP);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_THRESHOLD);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);
}

#[test]
fn test_config_panel_cursor_skips_headers() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.cursor = ROW_SEPARATOR;
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PERSONA);

    panel.cursor = ROW_SEPARATOR;
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS);

    panel.cursor = ROW_GENERAL_HEADER;
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);

    panel.cursor = ROW_OVERRIDES_HEADER;
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_PERSONA);

    panel.cursor = ROW_OVERRIDES_HEADER;
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_PROACTIVENESS);
}

#[test]
fn test_config_panel_cycle_autocompact() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(!panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(panel.buf_autocompact);
}

#[test]
fn test_config_panel_cycle_language() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    // 默认 empty，position 找不到 → fallback index 0 → first cycle 到 "en"
    assert!(panel.buf_language.is_empty());
    panel.cycle_language(false);
    assert_eq!(panel.buf_language, "en");
    // "en" → "zh-CN"
    panel.cycle_language(false);
    assert_eq!(panel.buf_language, "zh-CN");
    // "zh-CN" → "en" (wrap)
    panel.cycle_language(false);
    assert_eq!(panel.buf_language, "en");
    // reverse: "en" → "zh-CN"
    panel.cycle_language(true);
    assert_eq!(panel.buf_language, "zh-CN");
}

#[test]
fn test_config_panel_cycle_proactiveness() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_proactiveness = "low".to_string();
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "medium");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "high");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "low");
}

#[test]
fn test_config_panel_apply_edit_saves_to_config() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "zh-CN".to_string();
    panel.buf_persona = "Rust expert".to_string();
    panel.buf_tone = "concise".to_string();
    panel.buf_proactiveness = "high".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
    assert_eq!(cfg.config.persona.as_deref(), Some("Rust expert"));
    assert_eq!(cfg.config.tone.as_deref(), Some("concise"));
    assert_eq!(cfg.config.proactiveness.as_deref(), Some("high"));
}

#[test]
fn test_config_panel_apply_edit_compact_threshold() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "90".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.90).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_invalid_threshold_clamps() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "30".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.50).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_language_empty_is_none() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = String::new();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language, None);
}

#[test]
fn test_config_panel_apply_edit_language_en() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "en".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("en"));
}

#[test]
fn test_config_panel_apply_edit_language_zh_cn() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "zh-CN".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
}

#[test]
fn test_config_panel_cycle_diff() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(!panel.buf_diff);
    panel.cycle_diff();
    assert!(panel.buf_diff);
    panel.cycle_diff();
    assert!(!panel.buf_diff);
}

#[test]
fn test_config_panel_apply_edit_diff_enabled() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    assert!(!cfg.config.diff_enabled);
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_diff = true;
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert!(cfg.config.diff_enabled);
}

#[test]
fn test_config_panel_hooks_display_defaults_on() {
    let panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_hooks_display, "缺省应展示 hook 行");
}

#[test]
fn test_config_panel_cycle_hooks_display() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_hooks_display);
    panel.cycle_hooks_display();
    assert!(!panel.buf_hooks_display);
    panel.cycle_hooks_display();
    assert!(panel.buf_hooks_display);
}

#[test]
fn test_config_panel_apply_edit_hooks_display() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_hooks_display = false;
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert_eq!(cfg.config.hooks_ui, Some(false));
    assert!(!cfg.config.hooks_ui_enabled());
}

#[tokio::test]
async fn test_config_panel_hooks_display_updates_runtime_and_saved_config() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    app.services.config_path_override = Some(path.clone());
    app.services.peri_config = Some(PeriConfig::default());
    let (render_tx, mut render_rx) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = render_tx;
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.cursor = ROW_HOOKS_DISPLAY;
    let mut ctx = PanelContext {
        services: &mut app.services,
        session_mgr: &mut app.session_mgr,
        acp_client: None,
    };
    panel.handle_key(
        Input {
            key: tui_textarea::Key::Char(' '),
            ctrl: false,
            alt: false,
            shift: false,
        },
        &mut ctx,
    );
    drop(ctx);

    assert!(!panel.buf_hooks_display, "空格应翻转面板缓冲值");
    assert!(
        !app.session_mgr.current().ui.hook_rows_visible,
        "保存后运行时标志必须同步，否则改完不生效"
    );
    assert!(
        matches!(
            render_rx
                .try_recv()
                .expect("hook 开关变更必须即时通知渲染线程"),
            crate::ui::render_thread::RenderEvent::ToggleHookRows(false)
        ),
        "应补发 ToggleHookRows(false)"
    );
    let loaded = crate::config::load_from(&path).unwrap();
    assert_eq!(loaded.config.hooks_ui, Some(false), "保存应写回配置文件");
}

#[test]
fn test_config_panel_cycle_recap() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_recap);
    panel.cycle_recap();
    assert!(!panel.buf_recap);
    panel.cycle_recap();
    assert!(panel.buf_recap);
}

#[test]
fn test_config_panel_apply_edit_auto_recap() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_recap = false;
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert_eq!(cfg.config.auto_recap, Some(false));
}

#[test]
fn test_config_panel_cycle_detail_render_strategy() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    for expected in ["cache", "virtual", "auto"] {
        panel.cycle_detail_render_strategy(false);
        assert_eq!(panel.buf_detail_render_strategy, expected);
    }
    for expected in ["virtual", "cache", "auto"] {
        panel.cycle_detail_render_strategy(true);
        assert_eq!(panel.buf_detail_render_strategy, expected);
    }
}

#[test]
fn test_config_panel_detail_render_strategy_screen_rows() {
    assert_eq!(screen_to_logical_row(11), Some(ROW_HOOKS_DISPLAY));
    assert_eq!(screen_to_logical_row(12), Some(ROW_HOOKS_DISPLAY));
    assert_eq!(screen_to_logical_row(13), Some(ROW_DETAIL_RENDER_STRATEGY));
    assert_eq!(screen_to_logical_row(14), Some(ROW_DETAIL_RENDER_STRATEGY));
    assert_eq!(screen_to_logical_row(15), Some(ROW_STREAMING));
    assert_eq!(screen_to_logical_row(24), Some(ROW_TONE));
    assert_eq!(screen_to_logical_row(25), None);
}

#[test]
fn test_config_panel_detail_render_strategy_unknown_defaults_to_auto() {
    let mut cfg = PeriConfig::default();
    cfg.config.detail_render_strategy = Some("unknown".to_owned());
    let mut panel = ConfigPanel::from_config(&cfg);
    assert_eq!(panel.buf_detail_render_strategy, "auto");
    panel.apply_edit(&mut cfg, &make_lc()).unwrap();
    assert_eq!(cfg.config.detail_render_strategy, None);
}

#[tokio::test]
async fn test_config_panel_detail_render_strategy_updates_runtime_and_saved_config() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    app.services.config_path_override = Some(path.clone());
    app.services.peri_config = Some(PeriConfig::default());
    let (render_tx, mut render_rx) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = render_tx;
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.cursor = ROW_DETAIL_RENDER_STRATEGY;
    for expected in [Some("cache"), Some("virtual"), None] {
        let mut ctx = PanelContext {
            services: &mut app.services,
            session_mgr: &mut app.session_mgr,
            acp_client: None,
        };
        panel.handle_key(
            Input {
                key: tui_textarea::Key::Right,
                ctrl: false,
                alt: false,
                shift: false,
            },
            &mut ctx,
        );
        let event = render_rx.try_recv().expect("策略变更必须即时通知渲染线程");
        assert!(matches!(
            event,
            crate::ui::render_thread::RenderEvent::SetDetailStrategy(strategy)
                if strategy == expected.unwrap_or("auto")
        ));
        let loaded = crate::config::load_from(&path).unwrap();
        assert_eq!(loaded.config.detail_render_strategy.as_deref(), expected);
    }
}

#[tokio::test]
async fn test_config_panel_apply_detail_render_strategy_updates_runtime() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    let dir = tempfile::tempdir().unwrap();
    app.services.config_path_override = Some(dir.path().join("settings.json"));
    let (render_tx, mut render_rx) = tokio::sync::mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = render_tx;
    app.open_config_panel();
    app.session_mgr
        .current_mut()
        .session_panels
        .get_mut::<ConfigPanel>()
        .unwrap()
        .buf_detail_render_strategy = "virtual".to_owned();
    app.config_panel_apply();
    assert!(matches!(
        render_rx.try_recv().unwrap(),
        crate::ui::render_thread::RenderEvent::SetDetailStrategy(strategy) if strategy == "virtual"
    ));
}
