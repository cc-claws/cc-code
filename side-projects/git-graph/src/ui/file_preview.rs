use crate::app::App;
use peri_widgets::Theme;
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use unicode_width::UnicodeWidthStr;

/// 将高亮 spans 按水平滚动偏移和最大宽度截断，保留样式
fn slice_spans_hscroll(spans: &[(Style, String)], offset: u16, max_w: u16) -> Vec<Span<'static>> {
    let mut result = Vec::new();
    if max_w == 0 {
        return result;
    }
    let mut pos = 0u16; // 当前 span 在原始行中的起始列
    for (style, text) in spans {
        let tw = UnicodeWidthStr::width(text.as_str()) as u16;
        let span_end = pos + tw;
        // 整个 span 在可见区左边 → 跳过
        if span_end <= offset {
            pos = span_end;
            continue;
        }
        // 整个 span 在可见区右边 → 结束
        if pos >= offset + max_w {
            break;
        }
        // 计算可见子串
        let visible = slice_text_hscroll(text, style, pos, offset, max_w);
        if !visible.is_empty() {
            result.push(Span::styled(visible, *style));
        }
        pos = span_end;
    }
    result
}

/// 对单段文本做字符级水平截断
fn slice_text_hscroll(
    text: &str,
    _style: &Style,
    span_pos: u16,
    offset: u16,
    max_w: u16,
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut w = 0usize;
    let mut started = span_pos >= offset; // 第一个可见字符之前的都跳过
    for c in chars {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if !started {
            if span_pos + w as u16 + cw as u16 > offset {
                started = true;
                // 当前字符跨越 offset 边界，仍然输出
            } else {
                w += cw;
                continue;
            }
        }
        if UnicodeWidthStr::width(out.as_str()) + cw > max_w as usize {
            break;
        }
        out.push(c);
    }
    out
}

/// 构建单行 Line：行号 | 分隔符 | 内容（处理水平滚动和稀疏缓存）
fn build_line(
    line_idx: usize,
    raw: &str,
    highlighted: Option<&[(Style, String)]>,
    scroll_x: u16,
    content_width: u16,
) -> Line<'static> {
    let num = Span::styled(
        format!("{:>6} ", line_idx + 1),
        Style::default()
            .fg(Color::Rgb(100, 100, 100))
            .bg(Color::Rgb(25, 25, 35)),
    );
    let sep = Span::styled(
        "│",
        Style::default()
            .fg(Color::Rgb(60, 60, 70))
            .bg(Color::Rgb(20, 20, 28)),
    );

    let content_spans: Vec<Span> = if let Some(spans) = highlighted {
        slice_spans_hscroll(spans, scroll_x, content_width)
    } else {
        let visible = slice_text_hscroll(raw, &Style::default(), 0, scroll_x, content_width);
        vec![Span::raw(visible)]
    };

    let mut all = vec![num, sep, Span::raw(" ")];
    all.extend(content_spans);
    Line::from(all)
}

/// 渲染文件预览面板
pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    // 懒加载
    if app.preview_raw_lines.is_empty() && app.preview_file.is_some() {
        app.load_preview();
    }
    // 确保 highlighted 容量与 raw 一致（后台线程推送前调用）
    if app.preview_highlighted.is_empty() {
        app.preview_highlighted = vec![None; app.preview_raw_lines.len()];
    }

    let theme = &app.theme;
    let (path, _) = app
        .preview_file
        .as_ref()
        .map(|(p, s)| (p.as_str(), *s))
        .unwrap_or(("", false));

    let title = format!(" {} ", path);
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(title)
        .title_style(Style::default().fg(theme.text()))
        .border_style(Style::default().fg(theme.border()));
    f.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));
    if inner.height < 3 {
        return;
    }

    let viewport = inner.height.saturating_sub(1);
    let total_lines = app.preview_raw_lines.len();

    // clamp 垂直滚动
    let max_scroll = total_lines
        .saturating_sub(viewport as usize)
        .min(u16::MAX as usize) as u16;
    if app.preview_scroll > max_scroll {
        app.preview_scroll = max_scroll;
    }

    // clamp 水平滚动
    let prefix_width = 8u16; // "   123 " + "│" + " " = 7+1
    let content_width = inner.width.saturating_sub(prefix_width).saturating_sub(1); // -1 for scrollbar
    let max_x = app
        .preview_max_line_width
        .saturating_sub(content_width)
        .min(u16::MAX - 1);
    if app.preview_scroll_x > max_x {
        app.preview_scroll_x = max_x;
    }

    let start = app.preview_scroll as usize;
    let end = (start + viewport as usize).min(total_lines);

    let visible: Vec<Line> = (start..end)
        .map(|i| {
            let raw = app
                .preview_raw_lines
                .get(i)
                .map(|s| s.as_str())
                .unwrap_or("");
            let hl = app.preview_highlighted.get(i).and_then(|o| o.as_deref());
            build_line(i, raw, hl, app.preview_scroll_x, content_width)
        })
        .collect();

    let content_area = Rect::new(inner.x, inner.y, inner.width, viewport);
    f.render_widget(Paragraph::new(visible), content_area);

    // 底部状态行
    let bottom_y = inner.y + inner.height.saturating_sub(1);
    let mut status_parts: Vec<Span> = Vec::new();

    if app.preview_truncated {
        status_parts.push(Span::styled(
            format!("截断前 {} 行  ", total_lines),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if app.preview_highlighting {
        let done = app
            .preview_highlighted
            .iter()
            .filter(|o| o.is_some())
            .count();
        let pct = (done * 100).checked_div(total_lines).unwrap_or(100);
        status_parts.push(Span::styled(
            format!("高亮中 {}%  ", pct),
            Style::default().fg(Color::Rgb(140, 140, 60)),
        ));
    }

    if total_lines > 0 && !app.preview_truncated {
        let pct = ((start as f64 / total_lines as f64) * 100.0) as u32;
        status_parts.push(Span::styled(
            format!("{}% L{}/{}", pct, start + 1, total_lines),
            Style::default().fg(Color::Rgb(100, 100, 110)),
        ));
    }

    // 水平滚动指示
    if app.preview_scroll_x > 0 || app.preview_max_line_width > content_width {
        let x_info = format!(" ←{}→ ", if app.preview_scroll_x > 0 { "+" } else { " " });
        status_parts.push(Span::styled(
            x_info,
            Style::default().fg(Color::Rgb(100, 100, 100)),
        ));
    }

    if !status_parts.is_empty() {
        let status_line = Line::from(status_parts);
        let status_width = status_line.width() as u16;
        let status_area = Rect::new(
            inner.x + inner.width.saturating_sub(status_width + 2),
            bottom_y,
            status_width + 2,
            1,
        );
        f.render_widget(Paragraph::new(status_line), status_area);
    }

    // 垂直滚动条
    if total_lines as u16 > viewport {
        let scrollbar_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y,
            1,
            viewport,
        );
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(app.preview_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}
