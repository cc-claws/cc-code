//! UI 主题系统。
//!
//! 七套主题：`default` 是 cc-code 现行配色（本模块前身 `theme.rs` 的常量集），
//! 其余六套（`dark` / `light` / `light-daltonized` / `dark-daltonized` /
//! `light-ansi` / `dark-ansi`）的名称与配色与 claude-code-best/claude-code
//! 的 `src/utils/theme.ts` 逐值一致，见 `palettes.rs` 与一致性测试。
//!
//! 渲染代码统一通过 [`cur`] 读取当前调色板字段（如 `theme::cur().inactive`），
//! 不再引用编译期颜色常量；`/theme` 命令切换后写入进程级当前主题并持久化到
//! 配置 `theme` 字段，启动时由 `App::new` 恢复。
//!
//! `auto` 是跟随终端的动态项：选中时由 [`crate::terminal_theme`] 按终端颜色能力
//! 与背景明暗解析成上面某套具体主题。

pub mod palettes;

pub use palettes::Palette;

#[cfg(test)]
#[path = "palettes_parity_test.rs"]
mod palettes_parity_test;

use std::sync::RwLock;

/// 主题名。设置值（`setting_value`）与 claude-code-best 的 `THEME_NAMES`
/// 一致，另加 cc-code 自有的 `default`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeName {
/// 跟随终端：按颜色能力与背景明暗自动解析
Auto,
/// cc-code 现行配色
#[default]
Default,
Dark,
Light,
LightDaltonized,
DarkDaltonized,
LightAnsi,
DarkAnsi,
}

impl ThemeName {
/// 选择器展示顺序：auto 跟随终端排首，其后 default 与 claude-code-best ThemePicker 顺序。
pub const ALL: [ThemeName; 8] = [
ThemeName::Auto,
ThemeName::Default,
ThemeName::Dark,
ThemeName::Light,
ThemeName::DarkDaltonized,
ThemeName::LightDaltonized,
ThemeName::DarkAnsi,
ThemeName::LightAnsi,
];

/// 配置文件 / 命令参数中使用的主题名。
pub fn setting_value(&self) -> &'static str {
match self {
ThemeName::Auto => "auto",
ThemeName::Default => "default",
ThemeName::Dark => "dark",
ThemeName::Light => "light",
ThemeName::LightDaltonized => "light-daltonized",
ThemeName::DarkDaltonized => "dark-daltonized",
ThemeName::LightAnsi => "light-ansi",
ThemeName::DarkAnsi => "dark-ansi",
}
}

/// 从设置值解析主题名（大小写不敏感）；未知值返回 None。
pub fn from_setting(s: &str) -> Option<ThemeName> {
let lower = s.trim().to_ascii_lowercase();
ThemeName::ALL
.into_iter()
.find(|t| t.setting_value() == lower)
}

/// 选择器展示名（与 claude-code-best ThemePicker 的 label 一致，
/// auto / default 为 cc-code 自有项）。
pub fn label(&self) -> &'static str {
match self {
ThemeName::Auto => "Auto (follow terminal)",
ThemeName::Default => "Default",
ThemeName::Dark => "Dark mode",
ThemeName::Light => "Light mode",
ThemeName::DarkDaltonized => "Dark mode (colorblind-friendly)",
ThemeName::LightDaltonized => "Light mode (colorblind-friendly)",
ThemeName::DarkAnsi => "Dark mode (ANSI colors only)",
ThemeName::LightAnsi => "Light mode (ANSI colors only)",
}
}

/// 该主题的完整调色板。`auto` 解析为具体主题后再取。
pub fn palette(&self) -> Palette {
match self {
ThemeName::Auto => crate::terminal_theme::resolve().palette(),
ThemeName::Default => palettes::DEFAULT,
ThemeName::Dark => palettes::DARK,
ThemeName::Light => palettes::LIGHT,
ThemeName::LightDaltonized => palettes::LIGHT_DALTONIZED,
ThemeName::DarkDaltonized => palettes::DARK_DALTONIZED,
ThemeName::LightAnsi => palettes::LIGHT_ANSI,
ThemeName::DarkAnsi => palettes::DARK_ANSI,
}
}

/// 解析成具体主题（`auto` → 终端探测结果；其余原样返回）。
pub fn resolve(&self) -> ThemeName {
if matches!(self, ThemeName::Auto) {
crate::terminal_theme::resolve()
} else {
*self
}
}
}

static CURRENT: RwLock<ThemeName> = RwLock::new(ThemeName::Default);

/// 当前主题名。
pub fn current_name() -> ThemeName {
*CURRENT.read().unwrap_or_else(|p| p.into_inner())
}

/// 切换当前主题（仅进程内状态；持久化由调用方负责）。
pub fn set_current(name: ThemeName) {
*CURRENT.write().unwrap_or_else(|p| p.into_inner()) = name;
}

/// 当前主题的调色板。渲染热路径：Palette 为 Copy，读锁开销可忽略。
pub fn cur() -> Palette {
current_name().palette()
}

/// 按当前调色板构造 cc-widgets 的渲染主题（markdown / diff / 消息块）。
/// 每次渲染时调用，保证 /theme 切换后 widgets 渲染同步生效。
pub fn widgets_theme() -> cc_widgets::CustomTheme {
    let p = cur();
    cc_widgets::CustomTheme {
        accent: p.claude,
        success: p.success,
        warning: p.warning,
        error: p.error,
        thinking: p.thinking,
        text: p.text,
        muted: p.inactive,
        dim: p.subtle,
        border: p.prompt_border,
        border_active: p.claude,
        popup_bg: p.popup_bg,
        cursor_bg: p.cursor_bg,
        loading: p.loading,
        user_bg: p.user_message_background,
        bash_border: p.bash_border,
        diff_add: p.diff_add_fg,
        diff_add_bg: p.diff_added,
        diff_add_word_bg: p.diff_added_word,
        diff_remove: p.diff_remove_fg,
        diff_remove_bg: p.diff_removed,
        diff_remove_word_bg: p.diff_removed_word,
        diff_hunk: p.diff_hunk,
        syntax_theme_name: match current_name().resolve() {
            ThemeName::Light | ThemeName::LightDaltonized | ThemeName::LightAnsi => {
                "base16-ocean.light"
            }
            _ => "base16-ocean.dark",
        },
    }
}

// ── 全局统一符号契约 ─────────────────────────────────────────────────────────

/// 全局统一行首圆点符号（U+25CF Black Circle，以系统 Bash 工具指示符为唯一基准）
///
/// ****：
/// 统一作为 Bash 工具调用、AI 回复第一行前缀、子 Agent 启动/完成行、
/// AskUser 提示等所有行首状态圆点的。
/// 严禁使用 U+2022 ('•' BULLET 子弹符) 或其它异形圆点，杜绝终端渲染时一大一小的字形割裂。
pub const SYMBOL_DOT: &str = "●";

/// 带后置空格的统一圆点前缀（"● "），方便构建 Line/Span
pub const SYMBOL_DOT_SPACED: &str = "● ";

#[cfg(test)]
mod tests {
use super::*;
use ratatui::style::Color;

fn rgb(r: u8, g: u8, b: u8) -> Color {
Color::Rgb(r, g, b)
}

#[test]
fn setting_values_roundtrip() {
for t in ThemeName::ALL {
assert_eq!(ThemeName::from_setting(t.setting_value()), Some(t));
}
assert_eq!(ThemeName::from_setting(" Dark "), Some(ThemeName::Dark));
assert_eq!(ThemeName::from_setting("nope"), None);
}

#[test]
fn set_and_read_current() {
set_current(ThemeName::Light);
assert_eq!(current_name(), ThemeName::Light);
assert_eq!(cur(), palettes::LIGHT);
set_current(ThemeName::Default);
assert_eq!(cur(), palettes::DEFAULT);
}

/// default 主题保真锁定：这些值 = 主题化改造前 theme.rs 的常量原值，
/// 任一漂移都会改变默认外观，必须变红。
#[test]
fn default_palette_matches_legacy_constants() {
let p = palettes::DEFAULT;
assert_eq!(p.claude, rgb(215, 119, 87)); // ACCENT
assert_eq!(p.success, rgb(78, 186, 101)); // SAGE（TOOL_NAME / SUB_AGENT 同值）
assert_eq!(p.warning, rgb(255, 193, 7)); // WARNING
assert_eq!(p.error, rgb(255, 107, 128)); // ERROR
assert_eq!(p.thinking, rgb(162, 169, 228)); // THINKING
assert_eq!(p.yellow, rgb(255, 204, 0)); // YELLOW
assert_eq!(p.cyan, rgb(0, 188, 212)); // CYAN
assert_eq!(p.magenta, rgb(200, 100, 200)); // MAGENTA
assert_eq!(p.text, rgb(255, 255, 255)); // TEXT
assert_eq!(p.text_soft, rgb(208, 208, 208)); // TEXT_SOFT
assert_eq!(p.inactive, rgb(153, 153, 153)); // MUTED
assert_eq!(p.subtle, rgb(80, 80, 80)); // DIM
assert_eq!(p.drawer_text_active, rgb(255, 255, 255));
assert_eq!(p.drawer_muted, rgb(100, 116, 139));
assert_eq!(p.drawer_circle_on, rgb(241, 245, 249));
assert_eq!(p.prompt_border, rgb(80, 80, 80)); // BORDER
assert_eq!(p.border_dim, rgb(42, 42, 48)); // BORDER_DIM
assert_eq!(p.popup_bg, rgb(0, 0, 0)); // POPUP_BG
assert_eq!(p.cursor_bg, rgb(38, 38, 38)); // CURSOR_BG
assert_eq!(p.loading, rgb(147, 165, 255)); // LOADING
assert_eq!(p.user_message_background, rgb(55, 55, 55)); // USER_BG
assert_eq!(p.selection_bg, rgb(38, 79, 120)); // SELECTION_BG
assert_eq!(p.selected_fg, rgb(178, 185, 249)); // SELECTED_FG
assert_eq!(p.bash_border, rgb(253, 93, 177)); // BASH_BORDER
assert_eq!(p.shell_bg, rgb(36, 36, 36)); // SHELL_BG
assert_eq!(p.shell_border, rgb(106, 106, 106)); // SHELL_BORDER
assert_eq!(p.sub_agent_bg, rgb(30, 30, 38)); // SUB_AGENT_BG
assert_eq!(p.model_info, rgb(160, 130, 95)); // MODEL_INFO
assert_eq!(p.diff_added, rgb(18, 52, 26)); // widgets diff_add_bg
assert_eq!(p.diff_removed, rgb(55, 20, 18)); // widgets diff_remove_bg
assert_eq!(p.diff_added_word, rgb(26, 78, 36));
assert_eq!(p.diff_removed_word, rgb(78, 28, 22));
}
}
