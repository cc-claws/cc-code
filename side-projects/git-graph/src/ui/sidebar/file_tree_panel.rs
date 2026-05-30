use crate::app::App;
use peri_widgets::file_tree::render::FileTree;
use peri_widgets::Theme;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
    Frame,
};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Explorer ")
        .border_style(Style::default().fg(theme.border()));
    f.render_widget(block, area);

    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    if inner.height == 0 {
        return;
    }

    let tree = FileTree::new()
        .cursor_style(Style::default().bg(theme.cursor_bg()))
        .line_style(Style::default().fg(theme.dim()))
        .dir_style(Style::default().fg(theme.text()))
        .file_style(Style::default().fg(theme.muted()));

    f.render_stateful_widget(tree, inner, &mut app.file_tree_state);
}
