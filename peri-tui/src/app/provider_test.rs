use super::*;
use crate::config::{PeriConfig, ProviderConfig, ProviderModels};

fn make_config(alias: &str, provider_id: &str, model_id: &str, provider_type: &str) -> PeriConfig {
    let mut cfg = PeriConfig::default();
    cfg.config.active_alias = alias.to_string();
    cfg.config.active_provider_id = provider_id.to_string();
    cfg.config.providers.push(ProviderConfig {
        id: provider_id.to_string(),
        provider_type: provider_type.to_string(),
        api_key: "test-key".to_string(),
        models: ProviderModels {
            opus: if alias == "opus" {
                model_id.to_string()
            } else {
                String::new()
            },
            sonnet: if alias == "sonnet" {
                model_id.to_string()
            } else {
                String::new()
            },
            haiku: if alias == "haiku" {
                model_id.to_string()
            } else {
                String::new()
            },
        },
        ..Default::default()
    });
    cfg
}

#[test]
fn test_from_config_opus_alias() {
    let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
    assert_eq!(provider.model_name(), "claude-opus-4-6");
}

#[test]
fn test_from_config_sonnet_alias() {
    let cfg = make_config("sonnet", "openrouter", "gpt-5.4", "openai");
    let provider = LlmProvider::from_config(&cfg).expect("应成功解析");
    assert_eq!(provider.model_name(), "gpt-5.4");
}

#[test]
fn test_from_config_empty_model_fallback_anthropic() {
    let cfg = make_config("opus", "anthropic", "", "anthropic");
    let provider = LlmProvider::from_config(&cfg).expect("空 model 不应 panic");
    assert_eq!(provider.model_name(), "claude-sonnet-4-6");
}

#[test]
fn test_from_config_empty_model_fallback_openai() {
    let cfg = make_config("haiku", "openai", "", "openai");
    let provider = LlmProvider::from_config(&cfg).expect("空 model openai 不应 panic");
    assert_eq!(provider.model_name(), "gpt-4o");
}

#[test]
fn test_from_config_unknown_alias_fallback() {
    let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    cfg.config.active_alias = "ultra".to_string();
    let provider = LlmProvider::from_config(&cfg).expect("未知别名应 fallback");
    assert_eq!(provider.model_name(), "claude-sonnet-4-6");
}

#[test]
fn test_from_config_empty_api_key_returns_none() {
    let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    cfg.config.providers[0].api_key = String::new();
    let result = LlmProvider::from_config(&cfg);
    assert!(result.is_none(), "空 api_key 应返回 None");
}

#[test]
fn test_from_config_provider_not_found_returns_none() {
    let mut cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    cfg.config.active_provider_id = "nonexistent".to_string();
    let result = LlmProvider::from_config(&cfg);
    assert!(result.is_none(), "不存在的 provider 应返回 None");
}

// ── from_config_for_alias 测试 ─────────────────────────────────────────────

#[test]
fn test_from_config_for_alias_known() {
    let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    let p = LlmProvider::from_config_for_alias(&cfg, "opus").unwrap();
    assert_eq!(p.model_name(), "claude-opus-4-6");

    let cfg = make_config("sonnet", "openrouter", "gpt-5.4", "openai");
    let p = LlmProvider::from_config_for_alias(&cfg, "sonnet").unwrap();
    assert_eq!(p.model_name(), "gpt-5.4");

    let cfg = make_config("haiku", "anthropic", "claude-haiku-4", "anthropic");
    let p = LlmProvider::from_config_for_alias(&cfg, "haiku").unwrap();
    assert_eq!(p.model_name(), "claude-haiku-4");
}

#[test]
fn test_from_config_for_alias_unknown_returns_fallback() {
    let cfg = make_config("opus", "anthropic", "claude-opus-4-6", "anthropic");
    let p = LlmProvider::from_config_for_alias(&cfg, "turbo").unwrap();
    assert_eq!(p.model_name(), "claude-sonnet-4-6");
}

#[test]
fn test_from_config_for_alias_case_insensitive() {
    let cfg = make_config("haiku", "anthropic", "claude-haiku-4", "anthropic");
    let p = LlmProvider::from_config_for_alias(&cfg, "Haiku").unwrap();
    assert_eq!(p.model_name(), "claude-haiku-4");
    let p2 = LlmProvider::from_config_for_alias(&cfg, "HAIKU").unwrap();
    assert_eq!(p2.model_name(), "claude-haiku-4");
}
