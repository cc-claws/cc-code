use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use cc_widgets::BorderedPanel;

use crate::{
    app::{theme_panel::ThemePanel, App},
    ui::theme::{self, ThemeName},
};

pub(crate) fn render_theme_panel(f: &mut Frame, panel: &ThemePanel, app: &mut App, area: Rect) {
    let inner = BorderedPanel::new(Span::styled(
        " Select theme ",
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::cur().prompt_border))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    let mut lines: Vec<Line> = Vec::new();

    // Description
    lines.push(Line::from(Span::styled(
        "  Select a color theme. Moving the cursor previews it live.",
        Style::default().fg(theme::cur().inactive),
    )));
    lines.push(Line::from(""));

    // Theme rows（✔ = 打开面板时已保存的主题；右侧圆点为该主题自身的代表色）
    for (idx, name) in ThemeName::ALL.iter().enumerate() {
        let is_saved = *name == panel.original;
        let is_cursor = panel.cursor == idx;

        let label_style = if is_cursor {
            Style::default()
                .fg(theme::cur().thinking)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::cur().text)
                .add_modifier(Modifier::BOLD)
        };
        let check = if is_saved { "\u{2714}" } else { " " };
        let cursor_char = if is_cursor { "\u{276f}" } else { " " };
        let check_style = if is_saved {
            Style::default().fg(theme::cur().success)
        } else {
            Style::default().fg(theme::cur().inactive)
        };

        let p = name.palette();
        // `auto` 行直接标出当前终端解析到的具体主题，避免"选了 auto 但不知道实际用了哪套"
        let label = if matches!(name, ThemeName::Auto) {
            format!(
                "Auto (follow terminal: {})",
                crate::terminal_theme::resolve().setting_value()
            )
        } else {
            name.label().to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cursor_char),
                Style::default().fg(theme::cur().thinking),
            ),
            Span::styled(format!("{}. ", idx + 1), label_style),
            Span::styled(format!("{:<38}", label), label_style),
            Span::styled(format!(" {}  ", check), check_style),
            Span::styled("●".to_string(), Style::default().fg(p.claude)),
            Span::styled(" ●".to_string(), Style::default().fg(p.success)),
            Span::styled(" ●  ".to_string(), Style::default().fg(p.error)),
            Span::styled(
                name.setting_value().to_string(),
                Style::default().fg(theme::cur().inactive),
            ),
        ]));
    }

    // Footer：预览状态
    let selected = panel.selected();
    let footer = if selected == panel.original {
        format!("  Current theme: {}", panel.original.setting_value())
    } else {
        format!(
            "  Previewing: {} (Enter to apply, Esc to keep {})",
            selected.setting_value(),
            panel.original.setting_value()
        )
    };
    lines.push(Line::from(Span::styled(
        footer,
        Style::default().fg(theme::cur().inactive),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
