use crate::app::App;
use ratatui::{
    layout::{Alignment, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn draw_confirm(f: &mut Frame, area: Rect, app: &App) {
    if app.confirm_message.is_none() {
        return;
    }
    let msg = app.confirm_message.as_ref().unwrap();

    let popup_width = 50u16.min(area.width);
    let popup_height = 5u16;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(" ⚠ Confirm ");
    let inner = popup_area.inner(Margin::new(1, 1));

    let lines = vec![
        Line::from(Span::styled(
            msg.clone(),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " [Y]es ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                " [N]o ",
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
        ]),
    ];

    let para = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(para, popup_area);

    // 在 inner 区域渲染内容（block 内部）
    let _ = inner;
}
