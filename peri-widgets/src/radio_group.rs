use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::StatefulWidget,
};

/// 单选按钮组选项
#[derive(Debug, Clone)]
pub struct RadioOption<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
}

impl<'a> RadioOption<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            description: None,
        }
    }

    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }
}

/// 单选状态
#[derive(Debug, Clone)]
pub struct RadioState {
    selected: Option<usize>,
    cursor: usize,
}

impl RadioState {
    pub fn new() -> Self {
        Self {
            selected: None,
            cursor: 0,
        }
    }

    pub fn select(&mut self, index: usize) {
        self.selected = Some(index);
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn move_cursor(&mut self, delta: i32, total: usize) {
        if total == 0 {
            return;
        }
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, (total - 1) as i32) as usize;
    }
}

impl Default for RadioState {
    fn default() -> Self {
        Self::new()
    }
}

/// 单选按钮组 widget
pub struct RadioGroup<'a> {
    options: Vec<RadioOption<'a>>,
    marker_char: char,
    cursor_style: Style,
    normal_style: Style,
}

impl<'a> RadioGroup<'a> {
    pub fn new(options: Vec<RadioOption<'a>>) -> Self {
        Self {
            options,
            marker_char: '●',
            cursor_style: Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::BOLD),
            normal_style: Style::default(),
        }
    }

    pub fn marker_char(mut self, c: char) -> Self {
        self.marker_char = c;
        self
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }
}

impl StatefulWidget for RadioGroup<'_> {
    type State = RadioState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if self.options.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, opt) in self.options.iter().enumerate() {
            let is_cursor = i == state.cursor;
            let is_selected = state.selected == Some(i);
            let marker = if is_selected {
                self.marker_char.to_string()
            } else {
                "○".to_string()
            };
            let style = if is_cursor {
                self.cursor_style
            } else {
                self.normal_style
            };
            let mut spans = vec![
                Span::styled(format!("{} ", marker), style),
                Span::styled(opt.label.to_string(), style),
            ];
            if let Some(desc) = opt.description {
                spans.push(Span::styled(format!(" — {}", desc), style));
            }
            lines.push(Line::from(spans));
        }
        let text = ratatui::text::Text::from(lines);
        for (i, line) in text.lines.iter().enumerate() {
            if area.y as usize + i < buf.area.height as usize {
                let _ = buf.set_line(area.x, area.y + i as u16, line, area.width);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    include!("radio_group_test.rs");
}
