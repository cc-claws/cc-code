pub mod add_marketplace;
pub mod detail;
pub mod discover_detail;
pub mod discover_list;
pub mod discover_search;
pub mod list;

use ratatui::text::Line;

pub(crate) fn detail_kv_line<'a>(key: &str, value: &str) -> Line<'a> {
    use ratatui::{style::Style, text::Span};
    Line::from(vec![
        Span::styled(
            format!("  {}: ", key),
            Style::default().fg(crate::ui::theme::MUTED),
        ),
        Span::styled(
            value.to_string(),
            Style::default().fg(crate::ui::theme::TEXT),
        ),
    ])
}

pub(crate) fn truncate_display(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if UnicodeWidthStr::width(s) <= max_width {
        s.to_string()
    } else {
        let mut width = 0;
        let end = s
            .char_indices()
            .find(|&(_, c)| {
                width += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                width > max_width.saturating_sub(1)
            })
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}\u{2026}", &s[..end])
    }
}
