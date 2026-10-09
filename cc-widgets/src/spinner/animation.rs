use ratatui::{
    style::{Color, Style},
    text::Span,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const BRAILLE_FRAMES: &[char] = &[
    '✵', '✶', '✷', '✸', '✹', '✺', '✻', '✼', '❃', '❊', '✼', '✻', '✺', '✸', '✹', '✷',
];

pub fn tick_to_frame(tick: u64) -> char {
    BRAILLE_FRAMES[(tick as usize) % BRAILLE_FRAMES.len()]
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

pub fn format_elapsed(elapsed_ms: u64) -> String {
    let secs = elapsed_ms / 1000;
    let mins = secs / 60;
    let secs = secs % 60;
    if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
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

/// 动词流光渲染（对齐 Codex CLI summary_shimmer 并做同色系抛光优化）
///
/// - 周期 5000ms（5.0 秒低频从容），扫光持续 1600ms（1.6 秒悠长拂过）
/// - 采用余弦衰减模型，波峰向浅暖亮色提亮 45%，绝不产生刺眼纯白反差
/// - 非 RGB 颜色降级为静态原色
pub fn shimmer_verb_spans(verb: &str, elapsed_ms: u64, base_color: Color) -> Vec<Span<'static>> {
    let (r, g, b) = match base_color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return vec![Span::styled(verb.to_string(), Style::default().fg(base_color))],
    };

    let total_width = verb.width() as f64;
    if total_width == 0.0 {
        return Vec::new();
    }

    const INTERVAL_MS: f64 = 5000.0;
    const SWEEP_DURATION_MS: f64 = 1600.0;
    const HIGHLIGHT_COLOR: (u8, u8, u8) = (255, 225, 210);

    let cycle_time = (elapsed_ms as f64) % INTERVAL_MS;
    let is_shimmering = cycle_time < SWEEP_DURATION_MS;

    if !is_shimmering {
        return vec![Span::styled(verb.to_string(), Style::default().fg(base_color))];
    }

    let progress = cycle_time / SWEEP_DURATION_MS;
    let half_width = (total_width * 0.25).max(3.5);
    let position = progress * (total_width + 2.0 * half_width) - half_width;

    let mut col = 0.0;
    verb.graphemes(true)
        .map(|grapheme| {
            let glyph_width = grapheme.width() as f64;
            let center = col + glyph_width / 2.0;
            col += glyph_width;

            let dist = ((center - position).abs() / half_width).min(1.0);
            let intensity = 0.5 * (1.0 + (std::f64::consts::PI * dist).cos());
            let (nr, ng, nb) = blend_rgb((r, g, b), HIGHLIGHT_COLOR, (intensity * 0.45) as f32);
            Span::styled(
                grapheme.to_string(),
                Style::default().fg(Color::Rgb(nr, ng, nb)),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("animation_test.rs");
}
