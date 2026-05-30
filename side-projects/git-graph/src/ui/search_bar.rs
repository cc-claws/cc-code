use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let query = app.search_query.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Cyan)),
        Span::styled(
            query.to_string(),
            Style::default().fg(Color::White),
        ),
        Span::styled("▎", Style::default().fg(Color::Cyan)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
