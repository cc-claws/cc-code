use super::*;

fn make_lc() -> LcRegistry {
    LcRegistry::new(None)
}

fn make_lc_zh() -> LcRegistry {
    LcRegistry::new(Some("zh-CN"))
}

#[test]
fn test_lc_registry_default_lang() {
    let lc = make_lc();
    assert_eq!(lc.current_lang(), "en");
}

#[test]
fn test_lc_registry_explicit_lang() {
    let lc = LcRegistry::new(Some("zh-CN"));
    assert_eq!(lc.current_lang(), "zh-CN");
}

#[test]
fn test_lc_registry_invalid_lang_fallback() {
    let lc = LcRegistry::new(Some("fr"));
    assert_eq!(lc.current_lang(), "en");
}

#[test]
fn test_lc_registry_tr_english() {
    let lc = make_lc();
    assert_eq!(lc.tr("test-hello"), "Hello, World!");
}

#[test]
fn test_lc_registry_tr_chinese() {
    let lc = make_lc_zh();
    assert_eq!(lc.tr("test-hello"), "你好，世界！");
}

#[test]
fn test_lc_registry_tr_missing_key_fallback() {
    let lc = make_lc();
    assert_eq!(lc.tr("nonexistent-key-xyz"), "nonexistent-key-xyz");
}

#[test]
fn test_lc_registry_tr_args() {
    let lc = make_lc_zh();
    let result = lc.tr_args("test-greeting", &[("name".into(), FluentValue::from("Alice"))]);
    assert_eq!(result, "你好，Alice！");
}

#[test]
fn test_lc_registry_switch_success() {
    let mut lc = make_lc();
    assert!(lc.switch("zh-CN").is_ok());
    assert_eq!(lc.current_lang(), "zh-CN");
}

#[test]
fn test_lc_registry_switch_invalid() {
    let mut lc = make_lc();
    assert!(lc.switch("invalid").is_err());
}

#[test]
fn test_lc_registry_available_langs() {
    let lc = make_lc();
    let langs = lc.available_langs();
    assert!(langs.contains(&"en"));
    assert!(langs.contains(&"zh-CN"));
}

#[test]
fn test_lc_registry_cross_lang_fallback() {
    let mut lc = make_lc_zh();
    lc.switch("zh-CN").unwrap();
    let result = lc.tr("nonexistent-key");
    assert_eq!(result, "nonexistent-key");
}

fn make_detected_language(saved: Option<&str>, values: &[(&str, &str)]) -> &'static str {
    resolve_language(saved, |key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    })
}

#[test]
fn test_locale_detection_saved_language_overrides_environment() {
    assert_eq!(
        make_detected_language(Some("en"), &[("LANG", "zh_CN.UTF-8")]),
        "en"
    );
    assert_eq!(
        make_detected_language(Some("zh-CN"), &[("LC_ALL", "en_US")]),
        "zh-CN"
    );
    assert_eq!(
        make_detected_language(Some("unsupported"), &[("LANG", "zh_CN")]),
        "en"
    );
}

#[test]
fn test_locale_detection_normalizes_supported_locales() {
    for locale in ["zh", "zh_CN.UTF-8", "zh-Hans-CN", "zh_TW@variant", "ZH-cn"] {
        assert_eq!(make_detected_language(None, &[("LANG", locale)]), "zh-CN");
    }
    assert_eq!(
        make_detected_language(None, &[("LANG", "en_US.UTF-8")]),
        "en"
    );
}

#[test]
fn test_locale_detection_respects_environment_priority() {
    assert_eq!(
        make_detected_language(
            None,
            &[("LC_ALL", "en_US"), ("LANGUAGE", "zh"), ("LANG", "zh_CN")]
        ),
        "en"
    );
    assert_eq!(
        make_detected_language(
            None,
            &[
                ("LANGUAGE", "zh:en"),
                ("LC_MESSAGES", "en_US"),
                ("LANG", "en_US")
            ]
        ),
        "zh-CN"
    );
    assert_eq!(
        make_detected_language(None, &[("LC_MESSAGES", "zh_CN"), ("LANG", "en_US")]),
        "zh-CN"
    );
    assert_eq!(
        make_detected_language(None, &[("LC_ALL", ""), ("LANGUAGE", "fr:zh_CN:en")]),
        "zh-CN"
    );
}

#[test]
fn test_locale_detection_posix_and_unknown_fallback() {
    for locale in ["C", "C.UTF-8", "POSIX"] {
        assert_eq!(
            make_detected_language(None, &[("LANGUAGE", "zh"), ("LANG", locale)]),
            "en"
        );
    }
    assert_eq!(
        make_detected_language(None, &[("LC_ALL", "fr_FR"), ("LANG", "zh_CN")]),
        "en"
    );
    assert_eq!(
        make_detected_language(None, &[("LANGUAGE", "fr:de"), ("LANG", "zh_CN")]),
        "zh-CN"
    );
    assert_eq!(
        make_detected_language(None, &[("LANG", "fr_FR.UTF-8")]),
        "en"
    );
    assert_eq!(make_detected_language(None, &[]), "en");
}
