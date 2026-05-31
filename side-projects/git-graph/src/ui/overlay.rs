use crate::app::{App, Overlay};
use ratatui::{
    layout::{Alignment, Margin, Rect},
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
        Overlay::InputDialog => {
            if let Some(dialog) = &app.input_dialog {
                draw_input_dialog(f, area, &dialog.title, &dialog.value);
            }
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
        .map(|item| {
            Line::from(Span::styled(
                item.clone(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(para, popup_area);
}

/// 弹窗输入框（创建 tag / branch 共用）
fn draw_input_dialog(f: &mut Frame, area: Rect, title: &str, value: &str) {
    let popup_width = 50u16.min(area.width);
    let popup_height = 5u16;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", title));
    let inner = popup_area.inner(Margin::new(1, 1));

    // 输入行：值 + 光标
    let input_line = Line::from(vec![
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
        Span::styled("▎", Style::default().fg(Color::Cyan)),
    ]);

    // 提示行
    let hint_line = Line::from(Span::styled(
        "Enter 确认 · Esc 取消",
        Style::default().fg(Color::DarkGray),
    ));

    let para = Paragraph::new(vec![input_line, Line::from(""), hint_line])
        .block(block)
        .alignment(Alignment::Left);
    f.render_widget(para, popup_area);
    let _ = inner;
}
