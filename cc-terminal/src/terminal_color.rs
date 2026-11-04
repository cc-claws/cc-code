//! 终端色彩能力探测与输出降级。
//!
//! 主题与状态栏配色全部是 `Color::Rgb`：真彩色终端能直接表达 24-bit 序列，
//! 但只支持 256 色 / 16 色的终端会把 `38;2;r;g;b` 自行近似甚至忽略，设置
//! `NO_COLOR` 的环境也仍然照常上色。本模块负责把「终端到底能表达多少色」
//! 判定清楚，并提供等价降级。
//!
//! 判定与降级只发生在输出层（[`WidthSafeBackend`](crate::terminal_backend)），
//! 渲染侧仍按主题调色板取 `Color::Rgb`——配色表、主题指纹与渲染缓存都不受影响。
//!
//! 判定遵循 fail-open：只对**明确标识**为低色深的终端降级，认不出来时保持真彩色，
//! 与引入探测之前的行为一致（Windows 宿主通常不设 `TERM`，靠这条兜底）。

use std::sync::OnceLock;

use ratatui::style::Color;

/// 终端实际能表达的颜色深度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit 真彩色。
    TrueColor,
    /// 256 色：`Color::Rgb` 量化到 xterm 256 调色板。
    Ansi256,
    /// 16 色：`Color::Rgb` 量化到 ANSI 基础 16 色。
    Ansi16,
    /// 按 <https://no-color.org> 约定关闭彩色输出。
    NoColor,
}

static DEPTH: OnceLock<ColorDepth> = OnceLock::new();

/// 当前终端的颜色深度（进程内只判定一次）。
///
/// 结果缓存：输出层每个 cell 都要用到，环境变量只读一次。
pub fn depth() -> ColorDepth {
    *DEPTH.get_or_init(|| detect_from_env(|name| std::env::var(name).ok()))
}

/// 颜色深度判定。注入 env 读取器便于测试。
pub fn detect_from_env(get: impl Fn(&str) -> Option<String>) -> ColorDepth {
    // NO_COLOR 优先级最高：变量存在且非空即关闭彩色（空字符串按约定不生效），
    // 任何终端能力探测都不能把它翻回来。
    if get("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return ColorDepth::NoColor;
    }
    let term = get("TERM").unwrap_or_default().to_ascii_lowercase();
    if term == "dumb" {
        return ColorDepth::NoColor;
    }
    let term_program = get("TERM_PROGRAM").unwrap_or_default();
    let mut depth = if supports_true_color(&get, &term) {
        ColorDepth::TrueColor
    } else if term_program.eq_ignore_ascii_case("Apple_Terminal") || term.contains("256") {
        // Apple Terminal 只支持到 256 色（主题侧据此选 `*-ansi` 调色板）。
        ColorDepth::Ansi256
    } else {
        // 认不出来的宿主按真彩色处理，避免误伤（Windows 默认不设 TERM）。
        ColorDepth::TrueColor
    };
    // xterm.js（VS Code / Cursor / code-server）自 2017 年起支持真彩色，但容器化的
    // code-server 常不设 `COLORTERM`，会落到 256 色档把品牌色量化得失真。
    // 只在 256 档提级：`NO_COLOR` / `FORCE_COLOR=0` 已是 NoColor 档，不受影响。
    if depth == ColorDepth::Ansi256 && term_program.eq_ignore_ascii_case("vscode") {
        depth = ColorDepth::TrueColor;
    }
    // tmux 能把 `38;2` 正确解析进 cell 缓冲，但 client 端只在外层终端声明了
    // `Tc`/`RGB`（`terminal-overrides`）时才转发真彩色序列；默认配置下背景序列
    // 被丢掉，深色主题整块变黑。压到 256 色可稳定透传，视觉差异几乎不可见。
    if depth == ColorDepth::TrueColor
        && get("TMUX").is_some_and(|value| !value.is_empty())
        && get("PERI_TMUX_TRUECOLOR").is_none_or(|value| value.is_empty())
    {
        depth = ColorDepth::Ansi256;
    }
    depth
}

fn supports_true_color(get: &impl Fn(&str) -> Option<String>, term: &str) -> bool {
    let colorterm = get("COLORTERM").unwrap_or_default().to_ascii_lowercase();
    colorterm.contains("truecolor")
        || colorterm.contains("24bit")
        || term.contains("truecolor")
        || term.contains("24bit")
        || term.ends_with("-direct")
}

impl ColorDepth {
    /// 把渲染侧的颜色降级到本深度可表达的等价取值。
    ///
    /// `TrueColor` 原样返回；`NoColor` 只把颜色换成终端默认值，
    /// 保留 bold / dim / reversed 等属性（`NO_COLOR` 关的是颜色不是样式）。
    pub fn adapt(self, color: Color) -> Color {
        match self {
            ColorDepth::TrueColor => color,
            ColorDepth::NoColor => Color::Reset,
            ColorDepth::Ansi256 => match color {
                Color::Rgb(r, g, b) => Color::Indexed(nearest_256_index(r, g, b)),
                _ => color,
            },
            ColorDepth::Ansi16 => match color {
                Color::Rgb(r, g, b) => ansi16_color(r, g, b),
                Color::Indexed(index) => match index {
                    0..=15 => ANSI16_COLORS[usize::from(index)].1,
                    _ => {
                        let (r, g, b) = indexed_to_rgb(index);
                        ansi16_color(r, g, b)
                    }
                },
                _ => color,
            },
        }
    }
}

/// xterm 256 调色板的 6×6×6 色立方档位。
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// ANSI 基础 16 色：标准 xterm 取值 → ratatui 基础色变体。
///
/// 16 色的实际显示由终端主题重新定义，这里只能取 xterm 标准值做最近邻，
/// 与 chalk level 1 的做法一致。
const ANSI16_COLORS: [((u8, u8, u8), Color); 16] = [
    ((0, 0, 0), Color::Black),
    ((128, 0, 0), Color::Red),
    ((0, 128, 0), Color::Green),
    ((128, 128, 0), Color::Yellow),
    ((0, 0, 128), Color::Blue),
    ((128, 0, 128), Color::Magenta),
    ((0, 128, 128), Color::Cyan),
    ((192, 192, 192), Color::Gray),
    ((128, 128, 128), Color::DarkGray),
    ((255, 0, 0), Color::LightRed),
    ((0, 255, 0), Color::LightGreen),
    ((255, 255, 0), Color::LightYellow),
    ((0, 0, 255), Color::LightBlue),
    ((255, 0, 255), Color::LightMagenta),
    ((0, 255, 255), Color::LightCyan),
    ((255, 255, 255), Color::White),
];

/// 量化到 xterm 256 色索引。
///
/// 6×6×6 色立方与 24 级灰阶各算一个候选，取欧氏距离更近者：
/// 近灰的中性色落到灰阶段（232-255）才不会被色立方带出可辨的偏色。
fn nearest_256_index(r: u8, g: u8, b: u8) -> u8 {
    let cube = [nearest_level(r), nearest_level(g), nearest_level(b)];
    let cube_rgb = [
        CUBE_LEVELS[cube[0]],
        CUBE_LEVELS[cube[1]],
        CUBE_LEVELS[cube[2]],
    ];
    let cube_index = 16 + 36 * cube[0] as u8 + 6 * cube[1] as u8 + cube[2] as u8;
    let cube_distance = distance((r, g, b), (cube_rgb[0], cube_rgb[1], cube_rgb[2]));

    let average = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let gray_step = average.saturating_sub(8) as u8 / 10;
    let gray_index = 232 + gray_step.min(23);
    let gray_level = 8 + 10 * gray_step.min(23);
    let gray_distance = distance((r, g, b), (gray_level, gray_level, gray_level));

    if gray_distance < cube_distance {
        gray_index
    } else {
        cube_index
    }
}

fn nearest_level(value: u8) -> usize {
    let mut best = 0usize;
    let mut best_distance = u32::MAX;
    for (index, level) in CUBE_LEVELS.iter().enumerate() {
        let diff = i32::from(value) - i32::from(*level);
        let distance = (diff * diff) as u32;
        if distance < best_distance {
            best_distance = distance;
            best = index;
        }
    }
    best
}

fn ansi16_color(r: u8, g: u8, b: u8) -> Color {
    let mut best = ANSI16_COLORS[15].1;
    let mut best_distance = u32::MAX;
    for ((cr, cg, cb), color) in ANSI16_COLORS {
        let distance = distance((r, g, b), (cr, cg, cb));
        if distance < best_distance {
            best_distance = distance;
            best = color;
        }
    }
    best
}

fn distance((r, g, b): (u8, u8, u8), (cr, cg, cb): (u8, u8, u8)) -> u32 {
    let dr = i32::from(r) - i32::from(cr);
    let dg = i32::from(g) - i32::from(cg);
    let db = i32::from(b) - i32::from(cb);
    (dr * dr + dg * dg + db * db) as u32
}

/// 256 色索引还原成 RGB，供 16 色降级复用。
fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => ANSI16_COLORS[usize::from(index)].0,
        16..=231 => {
            let offset = index - 16;
            (
                CUBE_LEVELS[usize::from(offset / 36)],
                CUBE_LEVELS[usize::from((offset % 36) / 6)],
                CUBE_LEVELS[usize::from(offset % 6)],
            )
        }
        _ => {
            let level = 8 + 10 * (index - 232);
            (level, level, level)
        }
    }
}

#[cfg(test)]
#[path = "terminal_color_test.rs"]
mod tests;
