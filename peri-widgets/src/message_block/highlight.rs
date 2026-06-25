use ratatui::text::Span;

use crate::theme::Theme;

pub fn highlight_diff_line(line: &str, theme: &dyn Theme) -> Vec<Span<'static>> {
    if line.starts_with("@@ ") {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_hunk()),
        )]
    } else if line.starts_with('+') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_add()),
        )]
    } else if line.starts_with('-') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_remove()),
        )]
    } else {
        vec![Span::raw(line.to_string())]
    }
}

pub fn is_diff_content(text: &str) -> bool {
    for line in text.lines().take(5) {
        if line.starts_with("@@ ") || line.starts_with("+++") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("highlight_test.rs");
}
