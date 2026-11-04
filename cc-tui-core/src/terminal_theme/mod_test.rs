use super::*;
use crate::ui::theme::ThemeName;

#[test]
fn test_theme_for_apple_terminal_dark_selects_dark_ansi() {
    let theme = theme_for(ColorSupport::Ansi, Scheme::Dark);
    assert_eq!(theme, ThemeName::DarkAnsi);
}

#[test]
fn test_theme_for_apple_terminal_light_selects_light_ansi() {
    let theme = theme_for(ColorSupport::Ansi, Scheme::Light);
    assert_eq!(theme, ThemeName::LightAnsi);
}

#[test]
fn test_theme_for_truecolor_terminal_selects_plain_palette() {
    assert_eq!(
        theme_for(ColorSupport::TrueColor, Scheme::Dark),
        ThemeName::Dark
    );
    assert_eq!(
        theme_for(ColorSupport::TrueColor, Scheme::Light),
        ThemeName::Light
    );
}

#[test]
fn test_capability_from_env_detects_apple_terminal() {
    let capability = capability_from_env(|name: &str| {
        (name == "TERM_PROGRAM").then(|| "Apple_Terminal".to_string())
    });
    assert_eq!(capability, ColorSupport::Ansi);
}

#[test]
fn test_capability_from_env_defaults_to_truecolor() {
    let capability =
        capability_from_env(|name: &str| (name == "TERM_PROGRAM").then(|| "iTerm.app".to_string()));
    assert_eq!(capability, ColorSupport::TrueColor);
    assert_eq!(capability_from_env(|_: &str| None), ColorSupport::TrueColor);
}

#[test]
fn test_parse_osc11_sixteen_bit_dark_background() {
    let reply = "\u{1b}]11;rgb:0000/0000/0000\u{7}";
    assert_eq!(parse_osc11(reply), Some(Scheme::Dark));
}

#[test]
fn test_parse_osc11_eight_bit_light_background() {
    let reply = "\u{1b}]11;rgb:ff/ff/ff\u{7}";
    assert_eq!(parse_osc11(reply), Some(Scheme::Light));
}

#[test]
fn test_parse_osc11_st_terminated_reply_without_bel() {
    let reply = "\u{1b}]11;rgb:1c1c/1c1c/1c1c\u{1b}\\";
    assert_eq!(parse_osc11(reply), Some(Scheme::Dark));
}

#[test]
fn test_parse_osc11_rejects_unrelated_reply() {
    assert_eq!(parse_osc11(""), None);
    assert_eq!(parse_osc11("hello"), None);
    assert_eq!(parse_osc11("\u{1b}]4;0;rgb:ffff/0000/0000\u{7}"), None);
}

#[test]
fn test_scheme_from_components_threshold() {
    assert_eq!(scheme_from_components(&[1.0]), Some(Scheme::Light));
    assert_eq!(scheme_from_components(&[0.0, 0.85]), Some(Scheme::Dark));
    assert_eq!(
        scheme_from_components(&[0.9, 0.9, 0.9, 1.0]),
        Some(Scheme::Light)
    );
    assert_eq!(scheme_from_components(&[0.2, 0.2]), Some(Scheme::Dark));
    assert_eq!(scheme_from_components(&[]), None);
    assert_eq!(scheme_from_components(&[0.1, 0.2, 0.3, 0.4, 0.5]), None);
}

#[test]
fn test_scheme_from_appearance_dark_system() {
    // `defaults read -g AppleInterfaceStyle` 在深色模式下输出 Dark 并以 0 退出。
    assert_eq!(scheme_from_appearance("Dark\n", true), Scheme::Dark);
}

#[test]
fn test_scheme_from_appearance_light_system_without_key() {
    // 浅色模式下该键不存在：命令非零退出、stdout 为空。
    assert_eq!(scheme_from_appearance("", false), Scheme::Light);
}

#[test]
fn test_scheme_from_appearance_unknown_value_is_light() {
    assert_eq!(scheme_from_appearance("Blue\n", true), Scheme::Light);
    assert_eq!(scheme_from_appearance("\n", true), Scheme::Light);
}

/// 真机探测：打印本机 auto 的判定链路结果，并顺带核对 `AppleInterfaceStyle`。
/// 依赖本机终端与系统设置，在别的机器上没有意义，故 `#[ignore]`。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "依赖本机终端与系统外观，需手工 cargo test -- --ignored 验证"]
fn test_macos_terminal_app_detection_on_this_machine() {
    println!(
        "capability={:?} scheme={:?} theme={:?} appearance={:?}",
        capability(),
        scheme(),
        resolve(),
        macos_appearance_scheme()
    );
}
