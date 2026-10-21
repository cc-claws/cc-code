use super::*;
use std::collections::HashMap;

fn make_global() -> AppConfig {
    AppConfig {
        active_alias: "sonnet".to_string(),
        active_provider_id: "openai-1".to_string(),
        providers: vec![ProviderConfig {
            id: "openai-1".to_string(),
            provider_type: "openai".to_string(),
            api_key: "sk-global".to_string(),
            ..Default::default()
        }],
        thinking: Some(ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: "medium".to_string(),
            max_tokens: 32000,
        }),
        language: Some("zh".to_string()),
        diff_enabled: true,
        ..Default::default()
    }
}

#[test]
fn test_merge_workspace_default_preserves_most_fields() {
    let mut global = make_global();
    let workspace = AppConfig::default();
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "sonnet");
    assert_eq!(global.providers.len(), 1);
    assert!(global.thinking.is_some());
    // diff_enabled is bool (not Option<bool>), so default=false overrides global's true.
    // This is deliberate — the design chose "direct override" for bool fields.
    assert!(!global.diff_enabled);
}

#[test]
fn test_merge_workspace_complete_overrides_all() {
    let mut global = make_global();
    let workspace = AppConfig {
        active_alias: "opus".to_string(),
        active_provider_id: "anthro-1".to_string(),
        providers: vec![ProviderConfig {
            id: "anthro-1".to_string(),
            provider_type: "anthropic".to_string(),
            api_key: "sk-ws".to_string(),
            ..Default::default()
        }],
        language: Some("en".to_string()),
        diff_enabled: false,
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "opus");
    assert_eq!(global.active_provider_id, "anthro-1");
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].provider_type, "anthropic");
    assert_eq!(global.language, Some("en".to_string()));
    assert!(!global.diff_enabled);
    assert!(global.thinking.is_some());
}

#[test]
fn test_merge_providers_empty_array_does_not_override() {
    let mut global = make_global();
    let workspace = AppConfig {
        providers: vec![],
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].api_key, "sk-global");
}

#[test]
fn test_merge_single_field_override() {
    let mut global = make_global();
    let workspace = AppConfig {
        active_alias: "haiku".to_string(),
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "haiku");
    assert_eq!(global.providers.len(), 1);
    assert_eq!(global.providers[0].api_key, "sk-global");
}

#[test]
fn test_merge_env_override() {
    let mut global = AppConfig {
        env: Some(HashMap::from([("FOO".to_string(), "bar".to_string())])),
        ..make_global()
    };
    let workspace = AppConfig {
        env: Some(HashMap::from([("BAZ".to_string(), "qux".to_string())])),
        ..Default::default()
    };
    global.merge_overrides(workspace);
    let env = global.env.unwrap();
    assert!(!env.contains_key("FOO"));
    assert_eq!(env.get("BAZ"), Some(&"qux".to_string()));
}

#[test]
fn test_merge_diff_enabled_false_overrides_global_true() {
    let mut global = make_global(); // diff_enabled: true
    let workspace = AppConfig {
        diff_enabled: false,
        ..Default::default()
    };
    global.merge_overrides(workspace);
    assert!(!global.diff_enabled);
}

#[test]
fn test_merge_json_workspace_overrides_single_field() {
    let mut global = make_global(); // diff_enabled: true, active_alias: "sonnet"
    let json = r#"{"active_alias":"haiku"}"#;
    let workspace: AppConfig = serde_json::from_str(json).unwrap();
    global.merge_overrides(workspace);
    assert_eq!(global.active_alias, "haiku");
    // diff_enabled is bool (not Option<bool>), so default=false from JSON
    // directly overrides global's true — this is deliberate behavior (design choice B)
    assert!(!global.diff_enabled);
    // Other fields preserved from global
    assert_eq!(global.providers.len(), 1);
    assert!(global.thinking.is_some());
}

#[test]
fn test_detail_render_strategy_defaults_and_serialization() {
    let cfg: PeriConfig = serde_json::from_str(r#"{"config":{}}"#).unwrap();
    assert_eq!(
        cfg.config.detail_render_strategy, None,
        "旧配置默认使用 auto"
    );
    let serialized = serde_json::to_value(&cfg).unwrap();
    assert!(serialized["config"].get("detail_render_strategy").is_none());
    for strategy in ["auto", "cache", "virtual", "future-strategy"] {
        let mut cfg = PeriConfig::default();
        cfg.config.detail_render_strategy = Some(strategy.to_owned());
        let decoded: PeriConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(
            decoded.config.detail_render_strategy.as_deref(),
            Some(strategy)
        );
    }
}

#[test]
fn test_detail_render_strategy_workspace_merge() {
    let mut global = AppConfig {
        detail_render_strategy: Some("cache".to_owned()),
        ..Default::default()
    };
    global.merge_overrides(AppConfig::default());
    assert_eq!(global.detail_render_strategy.as_deref(), Some("cache"));
    for strategy in ["virtual", "auto"] {
        global.merge_overrides(AppConfig {
            detail_render_strategy: Some(strategy.to_owned()),
            ..Default::default()
        });
        assert_eq!(global.detail_render_strategy.as_deref(), Some(strategy));
    }
}

#[test]
fn test_detail_render_strategy_save_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    for strategy in [None, Some("auto"), Some("cache"), Some("virtual")] {
        let mut cfg = PeriConfig::default();
        cfg.config.detail_render_strategy = strategy.map(str::to_owned);
        crate::provider::store::save_to(&cfg, &path).unwrap();
        let loaded = crate::provider::store::load_from(&path).unwrap();
        assert_eq!(loaded.config.detail_render_strategy.as_deref(), strategy);
    }
}
