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
    assert!(panel.buf_language.is_empty());
    assert_eq!(panel.buf_proactiveness, "medium");
}

#[test]
fn test_config_panel_cursor_navigation() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert_eq!(panel.cursor, ROW_AUTOCOMPACT);

    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_THRESHOLD);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_LANGUAGE);
    panel.cursor_down();
    assert_eq!(panel.cursor, ROW_DIFF);
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
    assert_eq!(panel.cursor, ROW_DIFF);
    panel.cursor_up();
    assert_eq!(panel.cursor, ROW_LANGUAGE);
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
