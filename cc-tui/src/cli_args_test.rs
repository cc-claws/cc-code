use super::*;
use cc_tui::i18n::LcRegistry;

#[test]
fn test_output_format_parses_supported_values() {
    assert_eq!("text".parse::<OutputFormat>(), Ok(OutputFormat::Text));
    assert_eq!("json".parse::<OutputFormat>(), Ok(OutputFormat::Json));
    assert_eq!(
        "stream-json".parse::<OutputFormat>(),
        Ok(OutputFormat::StreamJson)
    );
}

#[test]
fn test_output_format_invalid_value_is_localized() {
    for (language, expected) in [("en", "Unknown output format"), ("zh-CN", "未知的输出格式")]
    {
        let result = OutputFormat::parse_with_language("xml", &LcRegistry::new(Some(language)));
        assert!(
            matches!(result, Err(ref error) if error.contains(expected) && error.contains("xml")),
            "格式错误需包含所选语言提示和原始参数"
        );
    }
}

#[test]
fn test_plugin_scope_parses_and_converts_supported_values() {
    for (value, scope, expected) in [
        (
            "user",
            PluginScope::User,
            cc_middlewares::plugin::InstallScope::User,
        ),
        (
            "project",
            PluginScope::Project,
            cc_middlewares::plugin::InstallScope::Project,
        ),
        (
            "local",
            PluginScope::Local,
            cc_middlewares::plugin::InstallScope::Local,
        ),
    ] {
        assert_eq!(value.parse::<PluginScope>(), Ok(scope));
        assert_eq!(cc_middlewares::plugin::InstallScope::from(scope), expected);
    }
}

#[test]
fn test_plugin_scope_invalid_value_is_localized() {
    for (language, expected) in [("en", "Unknown plugin scope"), ("zh-CN", "未知的插件范围")]
    {
        let result = PluginScope::parse_with_language("global", &LcRegistry::new(Some(language)));
        assert!(
            matches!(result, Err(ref error) if error.contains(expected) && error.contains("global")),
            "范围错误需包含所选语言提示和原始参数"
        );
    }
}
