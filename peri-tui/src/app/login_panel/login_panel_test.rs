use super::*;

fn make_test_config() -> PeriConfig {
    let mut cfg = PeriConfig::default();
    cfg.config.active_provider_id = "anthropic".to_string();
    cfg.config.providers.push(ProviderConfig {
        id: "anthropic".to_string(),
        provider_type: "anthropic".to_string(),
        api_key: "sk-ant-123".to_string(),
        base_url: String::new(),
        name: Some("Anthropic".to_string()),
        models: ProviderModels {
            opus: "claude-opus-4-7".to_string(),
            sonnet: "claude-sonnet-4-6".to_string(),
            haiku: "claude-haiku-4-5".to_string(),
        },
        thinking: None,
        extra: Default::default(),
    });
    cfg.config.providers.push(ProviderConfig {
        id: "openrouter".to_string(),
        provider_type: "openai".to_string(),
        api_key: "or-123".to_string(),
        base_url: "https://openrouter.ai/v1".to_string(),
        name: Some("OpenRouter".to_string()),
        models: ProviderModels {
            opus: "gpt-4o".to_string(),
            sonnet: "gpt-4o-mini".to_string(),
            haiku: "gpt-3.5-turbo".to_string(),
        },
        thinking: None,
        extra: Default::default(),
    });
    cfg
}

#[test]
fn test_login_panel_from_config_cursor_at_active_provider() {
    let cfg = make_test_config();
    let panel = LoginPanel::from_config(&cfg);
    assert_eq!(panel.cursor(), 0); // anthropic is at index 0
}

#[test]
fn test_login_panel_from_config_empty_providers_cursor_zero() {
    let cfg = PeriConfig::default();
    let panel = LoginPanel::from_config(&cfg);
    assert_eq!(panel.cursor(), 0);
}

#[test]
fn test_login_panel_move_cursor_clamp() {
    let cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    assert_eq!(panel.cursor(), 0);
    panel.move_cursor(1);
    assert_eq!(panel.cursor(), 1);
    panel.move_cursor(1);
    assert_eq!(panel.cursor(), 1); // clamp，不再循环
    panel.move_cursor(-1);
    assert_eq!(panel.cursor(), 0);
    panel.move_cursor(-1);
    assert_eq!(panel.cursor(), 0); // clamp，不再循环
}

#[test]
fn test_login_panel_enter_edit_fills_buffers() {
    let cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_edit();
    assert_eq!(panel.buf_opus_model, "claude-opus-4-7");
    assert_eq!(panel.buf_api_key, "sk-ant-123");
    assert_eq!(panel.mode, LoginPanelMode::Edit);
}

#[test]
fn test_login_panel_enter_new_auto_fills_openai() {
    let cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    assert_eq!(panel.buf_type, "openai");
    assert_eq!(panel.buf_opus_model, "gpt-4o");
    assert_eq!(panel.buf_sonnet_model, "gpt-4o-mini");
    assert_eq!(panel.buf_haiku_model, "gpt-3.5-turbo");
}

#[test]
fn test_login_panel_cycle_type_auto_fills_anthropic() {
    let cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    assert_eq!(panel.buf_type, "openai");
    panel.edit_field = LoginEditField::Type;
    panel.cycle_type();
    assert_eq!(panel.buf_type, "anthropic");
    assert_eq!(panel.buf_opus_model, "claude-opus-4-7");
    assert_eq!(panel.buf_sonnet_model, "claude-sonnet-4-6");
    assert_eq!(panel.buf_haiku_model, "claude-haiku-4-5");
}

#[test]
fn test_login_panel_cycle_type_preserves_custom_model() {
    let cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    panel.buf_opus_model = "my-custom-model".to_string();
    panel.edit_field = LoginEditField::Type;
    panel.cycle_type();
    assert_eq!(panel.buf_opus_model, "my-custom-model");
}

#[test]
fn test_login_panel_field_navigation() {
    let mut panel = LoginPanel::from_config(&PeriConfig::default());
    assert_eq!(panel.edit_field, LoginEditField::Name);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::Type);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::BaseUrl);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::ApiKey);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::OpusModel);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::SonnetModel);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::HaikuModel);
    panel.field_next();
    assert_eq!(panel.edit_field, LoginEditField::Name);
    panel.field_prev();
    assert_eq!(panel.edit_field, LoginEditField::HaikuModel);
}

#[test]
fn test_login_panel_push_pop_char() {
    use crate::app::handle_edit_key;
    use tui_textarea::{Input, Key};
    let mut panel = LoginPanel::from_config(&PeriConfig::default());
    panel.edit_field = LoginEditField::OpusModel;
    let (buf, cur) = panel.active_field().unwrap();
    handle_edit_key(
        buf,
        cur,
        Input {
            key: Key::Char('x'),
            ctrl: false,
            alt: false,
            shift: false,
        },
    );
    let (buf, cur) = panel.active_field().unwrap();
    handle_edit_key(
        buf,
        cur,
        Input {
            key: Key::Char('x'),
            ctrl: false,
            alt: false,
            shift: false,
        },
    );
    assert_eq!(panel.buf_opus_model, "xx");
    let (buf, cur) = panel.active_field().unwrap();
    handle_edit_key(
        buf,
        cur,
        Input {
            key: Key::Backspace,
            ctrl: false,
            alt: false,
            shift: false,
        },
    );
    assert_eq!(panel.buf_opus_model, "x");
}

#[test]
fn test_login_panel_push_char_ignored_for_type() {
    let mut panel = LoginPanel::from_config(&PeriConfig::default());
    let orig_type = panel.buf_type.clone();
    panel.edit_field = LoginEditField::Type;
    assert!(panel.active_field().is_none());
    assert_eq!(panel.buf_type, orig_type);
}

#[test]
fn test_login_panel_paste_text_filters_newlines() {
    let mut panel = LoginPanel::from_config(&PeriConfig::default());
    panel.edit_field = LoginEditField::ApiKey;
    panel.paste_text("key\nval\r\nend");
    assert_eq!(panel.buf_api_key, "keyvalend");
}

#[test]
fn test_login_panel_paste_text_ignored_for_type() {
    let mut panel = LoginPanel::from_config(&PeriConfig::default());
    let orig_type = panel.buf_type.clone();
    panel.edit_field = LoginEditField::Type;
    panel.paste_text("anthropic");
    assert_eq!(panel.buf_type, orig_type);
}

#[test]
fn test_login_panel_apply_edit_new_provider() {
    let mut cfg = PeriConfig::default();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    panel.buf_name = "My Provider".to_string();
    panel.buf_api_key = "sk-123".to_string();
    panel.buf_opus_model = "gpt-4o".to_string();
    panel.buf_sonnet_model = "gpt-4o-mini".to_string();
    panel.buf_haiku_model = "gpt-3.5-turbo".to_string();
    let ok = panel.apply_edit(&mut cfg);
    assert!(ok);
    assert_eq!(cfg.config.providers.len(), 1);
    assert_eq!(cfg.config.providers[0].models.opus, "gpt-4o");
}

#[test]
fn test_login_panel_apply_edit_new_provider_sets_active_id_when_empty() {
    let mut cfg = PeriConfig::default();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    panel.buf_name = "Test".to_string();
    panel.buf_api_key = "key".to_string();
    panel.apply_edit(&mut cfg);
    assert_eq!(cfg.config.active_provider_id, "test");
}

#[test]
fn test_login_panel_apply_edit_existing_provider() {
    let mut cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_edit();
    panel.buf_api_key = "new-key".to_string();
    let ok = panel.apply_edit(&mut cfg);
    assert!(ok);
    assert_eq!(cfg.config.providers[0].api_key, "new-key");
}

#[test]
fn test_login_panel_apply_edit_empty_name_returns_false() {
    let mut cfg = PeriConfig::default();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.enter_new();
    panel.buf_name = String::new();
    let ok = panel.apply_edit(&mut cfg);
    assert!(!ok);
}

#[test]
fn test_login_panel_confirm_delete_removes_provider() {
    let mut cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.browse_list.move_cursor_to(1); // openrouter
    panel.mode = LoginPanelMode::ConfirmDelete;
    panel.confirm_delete(&mut cfg);
    assert_eq!(cfg.config.providers.len(), 1);
    assert_eq!(cfg.config.providers[0].id, "anthropic");
}

#[test]
fn test_login_panel_confirm_delete_clears_active_provider_id() {
    let mut cfg = make_test_config();
    let mut panel = LoginPanel::from_config(&cfg);
    panel.browse_list.move_cursor_to(0); // anthropic (active)
    panel.mode = LoginPanelMode::ConfirmDelete;
    panel.confirm_delete(&mut cfg);
    assert!(cfg.config.active_provider_id.is_empty());
}

#[test]
fn test_login_panel_request_delete_no_providers_noop() {
    let cfg = PeriConfig::default();
    let mut panel = LoginPanel::from_config(&cfg);
    assert_eq!(panel.mode, LoginPanelMode::Browse);
    panel.request_delete();
    assert_eq!(panel.mode, LoginPanelMode::Browse);
}
