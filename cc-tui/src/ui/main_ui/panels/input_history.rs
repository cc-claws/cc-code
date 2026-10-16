use crate::{
    app::{input_history_panel::InputHistoryPanel, App},
    ui::{message_render::truncate_to_display_width, theme},
};
use cc_widgets::BorderedPanel;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

/// 渲染输入历史面板：一行一条历史，光标行加标记与强调色。
pub(crate) fn render_input_history_panel(
    f: &mut Frame,
    panel: &InputHistoryPanel,
    app: &mut App,
    area: Rect,
) {
    let inner = BorderedPanel::new(Span::styled(
        format!(" {} ", app.services.lc.tr("input-history-title")),
        Style::default()
            .fg(theme::cur().thinking)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::cur().prompt_border))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);

    if panel.entries.is_empty() {
        let line = Line::from(Span::styled(
            format!(" {}", app.services.lc.tr("input-history-empty")),
            Style::default().fg(theme::cur().inactive),
        ));
        f.render_widget(Paragraph::new(line), inner);
        return;
    }

    // 左侧标记 3 列（" ▸ " / "   "），右侧留 1 列贴边余量
    let text_width = inner.width.saturating_sub(4) as usize;
    let lines: Vec<Line<'static>> = panel
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == panel.cursor;
            let marker = if selected { " \u{25b8} " } else { "   " };
            let style = if selected {
                Style::default()
                    .fg(theme::cur().thinking)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::cur().text)
            };
            Line::from(Span::styled(
                format!("{marker}{}", flatten_entry(entry, text_width)),
                style,
            ))
        })
        .collect();

    let offset = scroll_offset_for(panel.cursor, inner.height as usize);
    f.render_widget(
        Paragraph::new(Text::from(lines)).scroll((offset as u16, 0)),
        inner,
    );
}

/// 多行输入折叠成单行（换行渲染为 `⏎`），并按显示宽度截断。
fn flatten_entry(entry: &str, max_width: usize) -> String {
    let flat = entry.lines().collect::<Vec<_>>().join(" \u{23ce} ");
    truncate_to_display_width(&flat, max_width)
}

/// 让光标行始终落在可视区内（对齐底部）。
fn scroll_offset_for(cursor: usize, visible_rows: usize) -> usize {
    if visible_rows == 0 || cursor < visible_rows {
        0
    } else {
        cursor + 1 - visible_rows
    }
}
