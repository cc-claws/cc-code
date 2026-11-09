use cc_widgets::BorderedPanel;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::{
    app::{kernel_panel::KernelPanel, App},
    ui::theme,
};

pub(crate) fn render_kernel_panel(f: &mut Frame, panel: &KernelPanel, app: &mut App, area: Rect) {
    let inner = BorderedPanel::new(Span::styled(
        " Select kernel ",
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::cur().prompt_border))
    .render(f, area);
    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    let current = app.current_kernel_id();
    let mut lines = vec![
        Line::from(Span::styled(
            "  Select an agent kernel. Enter to apply.",
            Style::default().fg(theme::cur().inactive),
        )),
        Line::from(""),
    ];
    for (index, id) in panel.list.items().iter().enumerate() {
        let focused = index == panel.list.cursor();
        let active = *id == current;
        let label_style = Style::default()
            .fg(if focused {
                theme::cur().thinking
            } else {
                theme::cur().text
            })
            .add_modifier(Modifier::BOLD);
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", if focused { theme::SYMBOL_CURSOR } else { " " }),
                Style::default().fg(theme::cur().thinking),
            ),
            Span::styled(format!("{}. {:<8}", index + 1, id), label_style),
            Span::styled(
                if active { " ✔" } else { "  " },
                Style::default().fg(theme::cur().success),
            ),
        ]));
    }
    lines.push(Line::from(Span::styled(
        format!("  Current kernel: {current}"),
        Style::default().fg(theme::cur().inactive),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}
