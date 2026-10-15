use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use cc_widgets::BorderedPanel;

use super::AtMentionState;
use crate::i18n::LcRegistry;
use crate::ui::theme;

/// 弹窗最大显示行数
pub const MAX_VIEWPORT: usize = 10;

/// 渲染 @ 提及文件候选弹窗
pub fn render_at_mention_popup(
    f: &mut Frame,
    state: &AtMentionState,
    input_area: Rect,
    lc: &LcRegistry,
) {
    let available_height = input_area.y.saturating_sub(f.area().y);
    if !state.active || available_height < 3 || input_area.width < 3 {
        return;
    }

    let total = state.candidates.len();
    let viewport = MAX_VIEWPORT
        .min(total.max(1))
        .min(available_height.saturating_sub(2) as usize);
    // 小终端和缩放后仍保证选中项位于可见范围内。
    let selected = state.selected.min(total.saturating_sub(1));
    let scroll_offset = state
        .scroll_offset
        .min(selected)
        .max((selected + 1).saturating_sub(viewport))
        .min(total.saturating_sub(viewport));
    let visible_count = viewport.min(total);
    let visible = &state.candidates[scroll_offset..scroll_offset + visible_count];

    let popup_height = viewport as u16 + 2; // 内容 + 边框上下
    let y = input_area.y.saturating_sub(popup_height);
    let popup_area = Rect {
        x: input_area.x,
        y,
        width: input_area.width,
        height: popup_height,
    };

    let title = if total > 0 {
        format!(
            "@ {}–{}/{}",
            scroll_offset + 1,
            scroll_offset + visible_count,
            total
        )
    } else {
        "@".to_string()
    };
    let inner = BorderedPanel::new(Span::styled(title, Style::default().fg(theme::cur().inactive)))
        .border_style(Style::default().fg(theme::cur().prompt_border))
        .render(f, popup_area);

    if total == 0 {
        let key = if state.searching {
            "at-mention-searching"
        } else if state.query.is_empty() {
            "at-mention-empty-directory"
        } else {
            "at-mention-no-matches"
        };
        let message = truncate_middle(&lc.tr(key), inner.width as usize);
        f.render_widget(
            Paragraph::new(message).style(Style::default().fg(theme::cur().inactive)),
            inner,
        );
        return;
    }

    let content_width = inner.width as usize;
    // "❯ " 或 "  " 占 2 列 + " " 图标占 1 列 + 空格 1 列 = 4 列前缀
    let path_max_width = content_width.saturating_sub(4);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(vi, cand)| {
            let global_idx = scroll_offset + vi;
            let is_selected = global_idx == state.selected;
            let icon = if cand.is_dir { "/" } else { " " };

            let display = truncate_middle(&cand.display, path_max_width);

            let prefix = if is_selected { "❯ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(theme::cur().thinking)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::cur().text)
            };

            Line::from(vec![
                Span::styled(
                    format!("{}{} ", prefix, icon),
                    Style::default().fg(theme::cur().thinking),
                ),
                Span::styled(display, style),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

/// 中间截断路径，保留前后部分，中间用 "..." 连接
fn truncate_middle(path: &str, max_width: usize) -> String {
    let width = UnicodeWidthStr::width(path);
    if width <= max_width {
        return path.to_string();
    }
    if max_width < 5 {
        // "..." 占 3 列 + 至少前后各 1 列 = 5
        return take_width(path, max_width);
    }

    let sep = "...";
    let sep_width = UnicodeWidthStr::width(sep);
    let remaining = max_width - sep_width;

    // 前半部分取 ceil(remaining/2) 列宽，后半取 floor(remaining/2)
    let head_width = remaining.div_ceil(2);
    let tail_width = remaining - head_width;

    let head: String = take_width(path, head_width);
    let tail: String = take_width_rev(path, tail_width);

    format!("{}{}{}", head, sep, tail)
}

/// 从字符串头部取指定显示宽度的字符
fn take_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_width {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result
}

/// 从字符串尾部取指定显示宽度的字符
fn take_width_rev(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut w = 0;
    for ch in s.chars().rev() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_width {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result.chars().rev().collect()
}

#[cfg(test)]
#[path = "popup_test.rs"]
mod tests;
