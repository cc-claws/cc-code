//! 终端键盘增强协议（kitty keyboard protocol）的启用与恢复。
//!
//! 传统终端会把 `Ctrl+Enter`、`Ctrl+I`、`Esc` 等多种按键编码成相同字节，应用层无法区分——
//! 例如 macOS Terminal.app 上 `Ctrl+Enter` 发出的就是普通 `Enter` 的 `\r`。支持 kitty 协议的
//! 终端在收到 `CSI > flags u` 后会把这类组合键改写为 CSI-u 序列（`Ctrl+Enter` → `\x1b[13;5u`），
//! crossterm 即可解析出带修饰位的 `KeyEvent`。
//!
//! 只推送 `DISAMBIGUATE_ESCAPE_CODES` 一位：
//! - `REPORT_EVENT_TYPES` 会让终端额外上报 release/repeat 事件，按键语义随之改变；
//! - `REPORT_ALTERNATE_KEYS` 会用 alternate keycode 覆盖基础 keycode，Shift 组合的 `KeyCode` 会变。
//!
//! 两者都不是区分 `Ctrl+Enter` 所必需的，故不启用。
//!
//! 不支持的终端（Terminal.app 等）会直接忽略该序列，行为保持不变。

use std::fmt;
use std::io::stdout;

use ratatui::crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, Command};

const DISABLE_KEYBOARD_ENHANCEMENT_ENV_VAR: &str = "PERI_TUI_DISABLE_KEYBOARD_ENHANCEMENT";

/// 进入 raw mode 后推送键盘增强标志；不支持的终端会忽略该序列。
pub fn enable_keyboard_enhancement() {
    if keyboard_enhancement_disabled() {
        return;
    }

    let _ = execute!(
        stdout(),
        DisableModifyOtherKeys,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
}

/// 退出前恢复终端键盘模式，必须与 [`enable_keyboard_enhancement`] 成对调用。
pub fn restore_keyboard_enhancement() {
    let _ = execute!(
        stdout(),
        PopKeyboardEnhancementFlags,
        DisableModifyOtherKeys
    );
}

fn keyboard_enhancement_disabled() -> bool {
    std::env::var(DISABLE_KEYBOARD_ENHANCEMENT_ENV_VAR)
        .ok()
        .and_then(|value| parse_bool_env(&value))
        .unwrap_or(false)
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        value if value.eq_ignore_ascii_case("true") => Some(true),
        value if value.eq_ignore_ascii_case("false") => Some(false),
        value if value.eq_ignore_ascii_case("yes") => Some(true),
        value if value.eq_ignore_ascii_case("no") => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisableModifyOtherKeys;

impl Command for DisableModifyOtherKeys {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("\x1b[>4;0m")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "modifyOtherKeys reset is not implemented for the legacy Windows API",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool_env_accepts_common_values() {
        assert_eq!(parse_bool_env("1"), Some(true));
        assert_eq!(parse_bool_env("true"), Some(true));
        assert_eq!(parse_bool_env("yes"), Some(true));
        assert_eq!(parse_bool_env("0"), Some(false));
        assert_eq!(parse_bool_env("false"), Some(false));
        assert_eq!(parse_bool_env("no"), Some(false));
        assert_eq!(parse_bool_env("maybe"), None);
    }

    #[test]
    fn test_enable_pushes_only_disambiguate_flag() {
        let mut out = String::new();
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            .write_ansi(&mut out)
            .expect("write_ansi 不应失败");
        assert_eq!(
            out, "\x1b[>1u",
            "只推送 disambiguate 位；release/repeat 与 alternate keycode 位会改变按键语义"
        );
    }

    #[test]
    fn test_restore_pops_flag_and_disables_modify_other_keys() {
        let mut out = String::new();
        PopKeyboardEnhancementFlags
            .write_ansi(&mut out)
            .expect("write_ansi 不应失败");
        DisableModifyOtherKeys
            .write_ansi(&mut out)
            .expect("write_ansi 不应失败");
        assert_eq!(
            out, "\x1b[<1u\x1b[>4;0m",
            "退出前必须弹出一层标志并关闭 modifyOtherKeys"
        );
    }
}
