use std::collections::HashMap;

use ratatui::style::Color;

use super::{detect_from_env, ColorDepth};

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn test_detect_no_color_disables_color() {
    assert_eq!(
        detect_from_env(env_of(&[("NO_COLOR", "1")])),
        ColorDepth::NoColor
    );
    // 按 no-color.org 约定，空字符串不生效
    assert_eq!(
        detect_from_env(env_of(&[("NO_COLOR", ""), ("TERM", "xterm-256color")])),
        ColorDepth::Ansi256
    );
    // NO_COLOR 优先级高于真彩色声明
    assert_eq!(
        detect_from_env(env_of(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")])),
        ColorDepth::NoColor
    );
}

#[test]
fn test_detect_dumb_terminal_disables_color() {
    assert_eq!(
        detect_from_env(env_of(&[("TERM", "dumb")])),
        ColorDepth::NoColor
    );
}

#[test]
fn test_detect_true_color_from_colorterm_and_term() {
    assert_eq!(
        detect_from_env(env_of(&[("COLORTERM", "truecolor")])),
        ColorDepth::TrueColor
    );
    assert_eq!(
        detect_from_env(env_of(&[("COLORTERM", "24bit")])),
        ColorDepth::TrueColor
    );
    assert_eq!(
        detect_from_env(env_of(&[("TERM", "xterm-direct")])),
        ColorDepth::TrueColor
    );
    assert_eq!(
        detect_from_env(env_of(&[("TERM", "xterm-truecolor")])),
        ColorDepth::TrueColor
    );
}

#[test]
fn test_detect_256_color_terminal() {
    assert_eq!(
        detect_from_env(env_of(&[("TERM", "xterm-256color")])),
        ColorDepth::Ansi256
    );
    // Apple Terminal 不支持 24-bit，只能到 256 色
    assert_eq!(
        detect_from_env(env_of(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "Apple_Terminal")
        ])),
        ColorDepth::Ansi256
    );
}

#[test]
fn test_detect_unknown_host_keeps_true_color() {
    // fail-open：认不出来的宿主（Windows 不设 TERM）保持现状
    assert_eq!(detect_from_env(|_: &str| None), ColorDepth::TrueColor);
    assert_eq!(
        detect_from_env(env_of(&[("TERM", "xterm")])),
        ColorDepth::TrueColor
    );
}

#[test]
fn test_detect_tmux_clamps_true_color_to_256() {
    // tmux 默认配置不转发 38;2 背景序列，真彩色背景会整块丢失
    assert_eq!(
        detect_from_env(env_of(&[
            ("COLORTERM", "truecolor"),
            ("TMUX", "/tmp/tmux-1000/default,1234,0")
        ])),
        ColorDepth::Ansi256
    );
    // terminal-overrides 已声明 Tc/RGB 时允许显式豁免
    assert_eq!(
        detect_from_env(env_of(&[
            ("COLORTERM", "truecolor"),
            ("TMUX", "/tmp/tmux-1000/default,1234,0"),
            ("PERI_TMUX_TRUECOLOR", "1")
        ])),
        ColorDepth::TrueColor
    );
}

#[test]
fn test_detect_vscode_xterm_js_boosts_256_to_true_color() {
    // code-server 等容器宿主常不设 COLORTERM，落到 256 色会量化失真
    assert_eq!(
        detect_from_env(env_of(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "vscode")
        ])),
        ColorDepth::TrueColor
    );
    // 显式关闭彩色优先于提级
    assert_eq!(
        detect_from_env(env_of(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "vscode"),
            ("NO_COLOR", "1")
        ])),
        ColorDepth::NoColor
    );
}

#[test]
fn test_detect_tmux_wins_over_vscode_boost() {
    // tmux 跑在 VS Code 终端里时，tmux 的转发限制优先
    assert_eq!(
        detect_from_env(env_of(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "vscode"),
            ("TMUX", "/tmp/tmux-1000/default,1234,0")
        ])),
        ColorDepth::Ansi256
    );
}

#[test]
fn test_adapt_true_color_is_identity() {
    let color = Color::Rgb(215, 119, 87);
    assert_eq!(ColorDepth::TrueColor.adapt(color), color);
}

#[test]
fn test_adapt_no_color_resets_colors() {
    assert_eq!(ColorDepth::NoColor.adapt(Color::Rgb(1, 2, 3)), Color::Reset);
    assert_eq!(ColorDepth::NoColor.adapt(Color::Red), Color::Reset);
}

#[test]
fn test_adapt_to_256_maps_primaries_to_cube() {
    assert_eq!(
        ColorDepth::Ansi256.adapt(Color::Rgb(255, 0, 0)),
        Color::Indexed(196)
    );
    assert_eq!(
        ColorDepth::Ansi256.adapt(Color::Rgb(215, 119, 87)),
        Color::Indexed(173)
    );
}

#[test]
fn test_adapt_to_256_keeps_near_gray_in_gray_ramp() {
    // 中性灰必须落进 232-255 灰阶，不能被色立方带出偏色
    assert_eq!(
        ColorDepth::Ansi256.adapt(Color::Rgb(128, 128, 128)),
        Color::Indexed(244)
    );
}

#[test]
fn test_adapt_to_16_maps_to_base_colors() {
    assert_eq!(ColorDepth::Ansi16.adapt(Color::Rgb(0, 0, 0)), Color::Black);
    assert_eq!(
        ColorDepth::Ansi16.adapt(Color::Rgb(255, 0, 0)),
        Color::LightRed
    );
    assert_eq!(
        ColorDepth::Ansi16.adapt(Color::Rgb(255, 255, 255)),
        Color::White
    );
    assert_eq!(
        ColorDepth::Ansi16.adapt(Color::Rgb(215, 119, 87)),
        Color::DarkGray
    );
}

#[test]
fn test_adapt_to_16_converts_indexed_above_palette() {
    // 基础 16 索引直接用对应变体，避免老终端不支持 38;5;n
    assert_eq!(ColorDepth::Ansi16.adapt(Color::Indexed(0)), Color::Black);
    assert_eq!(
        ColorDepth::Ansi16.adapt(Color::Indexed(196)),
        Color::LightRed
    );
}

#[test]
fn test_adapt_keeps_non_rgb_colors() {
    assert_eq!(ColorDepth::Ansi256.adapt(Color::Red), Color::Red);
    assert_eq!(ColorDepth::Ansi16.adapt(Color::Reset), Color::Reset);
    assert_eq!(
        ColorDepth::Ansi256.adapt(Color::Indexed(100)),
        Color::Indexed(100)
    );
}
