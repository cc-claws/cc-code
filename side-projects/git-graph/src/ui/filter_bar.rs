use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

#[allow(dead_code)]
pub fn draw_filter_bar(f: &mut Frame, area: Rect, app: &App) {
    let filter = app.filter_branch.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(" Filter: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            if filter.is_empty() {
                "all branches".to_string()
            } else {
                filter.to_string()
            },
            Style::default().fg(Color::White),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
