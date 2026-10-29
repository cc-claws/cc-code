use std::sync::OnceLock;

use ratatui::{
    style::{Color, Style},
    text::Span,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// 默认字形集：`· ✢ * ✶ ✻ ✽`，与上游二进制（2.1.310）`other` 字形集逐字一致。
///
/// 第三位是 ASCII `*` 而非 `✳`——上游把 `✳` 留给 darwin / ghostty 两套，理由是
/// `✳`(U+2733) 会被 emoji-regex 判成双宽，切帧时导致后续文字左右抖动。
/// 本实现没有 darwin 分支（见 [`spinner_chars`]），macOS 上同样落到这一套。
const SPINNER_CHARS_DEFAULT: [char; 6] = ['·', '✢', '*', '✶', '✻', '✽'];

/// Ghostty 字形集：`· ✢ ✳ ✶ ✻ ✻`，与上游二进制（2.1.310）`ghostty` 字形集一致。
///
/// `✽` 在 Ghostty 下渲染偏移，末位改用 `✻`；`✳` 在 Ghostty 下宽度正确，
/// 故第三位保留 `✳`（而非默认集里的 ASCII `*`）。
const SPINNER_CHARS_GHOSTTY: [char; 6] = ['·', '✢', '✳', '✶', '✻', '✻'];

/// 单次完整呼吸往返的周期（毫秒）。
///
/// **与上游不一致**：上游二进制（2.1.310）是 `spinner[floor(t/250) % 12]` 的线性
/// 12 帧回文，一圈 3000ms；本实现是 6 帧余弦缓动、2000ms 一圈。
pub const SPINNER_CYCLE_MS: u64 = 2000;

/// 当前终端适用的字形集。按 `TERM` 选一次并缓存（30fps 下不能每帧读环境变量）。
fn spinner_chars() -> &'static [char; 6] {
    static CACHE: OnceLock<&'static [char; 6]> = OnceLock::new();
    CACHE.get_or_init(|| {
        if std::env::var("TERM").as_deref() == Ok("xterm-ghostty") {
            &SPINNER_CHARS_GHOSTTY
        } else {
            &SPINNER_CHARS_DEFAULT
        }
    })
}

/// 按动画时钟（毫秒）取当前字形。
///
/// ```text
/// frame = round( (1 - cos(2π · t / 2000)) / 2 · (字形数 - 1) )
/// ```
///
/// **本实现自带的设计，不是上游算法。** 上游二进制（2.1.310）的取帧是
/// `spinner[floor(t/250) % 12]`——12 帧回文数组上的线性步进，每帧恒定 250ms、
/// 一圈 3000ms，没有余弦。这里的余弦缓动让两端字形（`·` 与 `✽`）停留更久、
/// 中段更快（实测 `*` / `✶` 只停约 130ms，上游是 250ms），整体是一个 2 秒的
/// 「呼吸」往返；「帧速与渲染帧率解耦、纯时间驱动」这一点与上游一致。
pub fn spinner_frame(elapsed_ms: u64) -> char {
    let chars = spinner_chars();
    let phase = (elapsed_ms % SPINNER_CYCLE_MS) as f32 / SPINNER_CYCLE_MS as f32;
    let eased = (1.0 - (phase * std::f32::consts::TAU).cos()) / 2.0;
    let idx = (eased * (chars.len() - 1) as f32).round() as usize;
    chars[idx.min(chars.len() - 1)]
}

pub fn smooth_increment(displayed: usize, target: usize) -> usize {
    if displayed >= target {
        return target;
    }
    let gap = target - displayed;
    let step = if gap < 70 {
        3
    } else if gap < 200 {
        (gap * 15 / 100).max(8)
    } else {
        50
    };
    (displayed + step).min(target)
}

/// 把秒数格式化为人类可读的动态走秒时长（如 `22s`、`5m 00s`、`8m 38s`、`1h 05m 12s`）。
///
/// 走秒期间分钟以上保持两位补零（`{:02}s`），确保整分（如 8m 00s）时宽度不塌陷，
/// 避免类似 `7m 59s` -> `8m 0s` -> `8m 01s` 导致的文本宽度跳变与视觉闪烁。
pub fn format_elapsed_secs(secs: u64) -> String {
    if secs >= 3600 {
        let (hours, minutes, rest) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        format!("{hours}h {minutes:02}m {rest:02}s")
    } else if secs >= 60 {
        let (minutes, rest) = (secs / 60, secs % 60);
        format!("{minutes}m {rest:02}s")
    } else {
        format!("{secs}s")
    }
}

/// 把毫秒数格式化为人类可读的动态走秒时长。
pub fn format_elapsed(elapsed_ms: u64) -> String {
    format_elapsed_secs(elapsed_ms / 1000)
}

pub fn format_tokens(count: usize) -> String {
    if count >= 1000 {
        let k = count as f64 / 1000.0;
        if k >= 10.0 {
            format!("{:.0}k", k)
        } else {
            format!("{:.1}k", k)
        }
    } else {
        count.to_string()
    }
}

/// 两个 RGB 颜色之间的线性插值
fn blend_rgb(c1: (u8, u8, u8), c2: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = (c1.0 as f32 * (1.0 - t) + c2.0 as f32 * t).round() as u8;
    let g = (c1.1 as f32 * (1.0 - t) + c2.1 as f32 * t).round() as u8;
    let b = (c1.2 as f32 * (1.0 - t) + c2.2 as f32 * t).round() as u8;
    (r, g, b)
}

/// 在两个颜色之间按 `t`（0..=1）插值（停滞转红 / 思考强度着色用）。
///
/// - 两个都是 RGB 时逐通道线性插值；
/// - 任一为命名色（ANSI 主题）时无法插值，按 CC 的降级规则 `t > 0.5` 取目标色、否则取源色。
pub fn blend_color(from: Color, to: Color, t: f32) -> Color {
    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let (r, g, b) = blend_rgb((r1, g1, b1), (r2, g2, b2), t);
            Color::Rgb(r, g, b)
        }
        _ => {
            if t > 0.5 {
                to
            } else {
                from
            }
        }
    }
}

/// 流光位置（对齐 CC 2.1.300 的 glimmer 机制）：
///
/// - 步进 50ms，周期 = 文本宽 + 20，位置从 −10 起左→右扫过文本；
/// - 位置在文本外（周期两端的 10 列填充区）时即「停泊」，整词底色渲染。
pub fn glimmer_index(elapsed_ms: u64, text_width: usize) -> i64 {
    let cycle = text_width as i64 + 20;
    ((elapsed_ms / 50) as i64 % cycle) - 10
}

/// 动词流光渲染（对齐 CC 的 GlimmerMessage / ShimmerChar）：
///
/// - 只有流光位置 ±1 列内的字形用 `shimmer_color`，其余用 `base_color`（硬切，不插值）；
/// - 流光停泊（位置在文本外）时整词单段底色；
/// - 相邻同色字形合并成一段，减少 span 数量。
pub fn shimmer_verb_spans(
    verb: &str,
    elapsed_ms: u64,
    base_color: Color,
    shimmer_color: Color,
) -> Vec<Span<'static>> {
    let total_width = verb.width();
    if total_width == 0 {
        return Vec::new();
    }
    let idx = glimmer_index(elapsed_ms, total_width);
    if idx + 1 < 0 || idx > total_width as i64 {
        return vec![Span::styled(
            verb.to_string(),
            Style::default().fg(base_color),
        )];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col: i64 = 0;
    for grapheme in verb.graphemes(true) {
        let glyph_width = grapheme.width() as i64;
        let center = col + glyph_width / 2;
        col += glyph_width;
        let color = if (center - idx).abs() <= 1 {
            shimmer_color
        } else {
            base_color
        };
        match spans.last_mut() {
            Some(last) if last.style.fg == Some(color) => {
                last.content.to_mut().push_str(grapheme);
            }
            _ => spans.push(Span::styled(
                grapheme.to_string(),
                Style::default().fg(color),
            )),
        }
    }
    spans
}

/// 无新 token 停滞的转红强度目标值（对齐 CC 2.1.300 的停滞函数）：
///
/// - 无 token 超过 10s 判定停滞，强度再用 10s 线性渐满至 1；
/// - 有工具活跃执行时永不停滞（返回 0）。
pub fn stall_intensity_target(time_since_token_ms: u64, has_active_tools: bool) -> f32 {
    if has_active_tools || time_since_token_ms <= 10_000 {
        0.0
    } else {
        ((time_since_token_ms - 10_000) as f32 / 10_000.0).min(1.0)
    }
}

/// 思考强度（对齐 CC 2.1.300 的 thinkingIntensity）：
///
/// 当前思考段超过 10s 后线性爬升，再 10s 渐满至 1；驱动状态词向 warning 色插值。
pub fn thinking_intensity(thinking_elapsed_ms: u64) -> f32 {
    if thinking_elapsed_ms <= 10_000 {
        0.0
    } else {
        ((thinking_elapsed_ms - 10_000) as f32 / 10_000.0).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("animation_test.rs");
}
