use crate::app::{App, Overlay};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn draw_overlay(f: &mut Frame, area: Rect, app: &App) {
    match app.overlay {
        Overlay::BranchList => {
            let items = app.repo.branch_names().unwrap_or_default();
            draw_list(f, area, " Branches ", &items);
        }
        Overlay::TagList => {
            let items = app.repo.tag_names_list().unwrap_or_default();
            let tags: Vec<String> = items.into_iter().collect();
            draw_list(f, area, " Tags ", &tags);
        }
        Overlay::StashList => {
            let stashes: Vec<String> = app
                .stash_map
                .values()
                .flatten()
                .map(|s| format!("stash@{{{}}}: {}", s.index, s.message))
                .collect();
            draw_list(f, area, " Stash ", &stashes);
        }
        _ => {}
    }
}

fn draw_list(f: &mut Frame, area: Rect, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    let popup_width = 40u16.min(area.width);
    let popup_height = (items.len() as u16 + 2).min(20).min(area.height);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let lines: Vec<Line> = items
        .iter()
        .map(|item| Line::from(Span::styled(item.clone(), Style::default().fg(Color::White))))
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(para, popup_area);
}
