use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear},
    Frame,
};

/// 带边框容器——封装 Clear + Block + borders 一步到位
///
/// render() 返回 inner Rect 供后续渲染使用。
pub struct BorderedPanel<'a> {
    title: Line<'a>,
    border_style: Style,
}

impl<'a> BorderedPanel<'a> {
    pub fn new(title: impl Into<Line<'a>>) -> Self {
        Self {
            title: title.into(),
            border_style: Style::default(),
        }
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// 渲染边框面板：先 Clear 背景，再渲染 Block 边框，返回 inner area
    pub fn render(self, f: &mut Frame, area: Rect) -> Rect {
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(self.title)
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(self.border_style);
        let inner = block.inner(area);
        f.render_widget(&block, area);
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    include!("bordered_panel_test.rs");
}
