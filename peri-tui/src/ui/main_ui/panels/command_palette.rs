use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::{
    app::{
        command_palette_panel::{CommandPalettePanel, EFFORT_OPTIONS, ListRow, SelectPhase},
        App,
    },
    ui::theme,
};

pub(crate) fn render_command_palette(
    f: &mut Frame,
    panel: &mut CommandPalettePanel,
    _app: &mut App,
    area: Rect,
) {
    match panel.phase {
        SelectPhase::Model => render_model_phase(f, panel, _app, area),
        SelectPhase::Effort => render_effort_phase(f, panel, _app, area),
    }
}

/// 第一步：选择 Provider + Model
fn render_model_phase(
    f: &mut Frame,
    panel: &mut CommandPalettePanel,
    _app: &mut App,
    area: Rect,
) {
    let active_provider = _app
        .services
        .peri_config
        .as_ref()
        .map(|c| c.config.active_provider_id.as_str())
        .unwrap_or("");
    let active_alias = _app
        .services
        .peri_config
        .as_ref()
        .map(|c| c.config.active_alias.as_str())
        .unwrap_or("");

    let inner = BorderedPanel::new(Span::styled(
        " Switch Provider & Model ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 提示行
            Constraint::Length(1), // 分隔线
            Constraint::Min(1),   // 列表 + 底部提示
        ])
        .split(inner);

    // ── 顶部提示行 ──
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " \u{2191}\u{2193} Move",
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                "  Enter/\u{2192} Effort",
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(
                "  Esc Close",
                Style::default().fg(theme::MUTED),
            ),
        ])),
        chunks[0],
    );

    // ── 分隔线 ──
    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(theme::DIM))),
        chunks[1],
    );

    // ── 列表 ──
    let list_area = chunks[2];
    let max_list_lines = list_area.height.saturating_sub(1) as usize;
    let start = panel.scroll_offset;
    let end = (start + max_list_lines).min(panel.rows.len());

    let mut lines: Vec<Line> = Vec::new();

    for row_idx in start..end {
        match &panel.rows[row_idx] {
            ListRow::Header(name) => {
                lines.push(Line::from(Span::styled(
                    format!("  {}:", name),
                    Style::default()
                        .fg(theme::MAGENTA)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            ListRow::Entry(entry_idx) => {
                let entry = &panel.entries[*entry_idx];
                let is_cursor = panel.cursor == row_idx;
                let is_active =
                    entry.provider_id == active_provider && entry.alias == active_alias;

                let cursor_char = if is_cursor { "\u{276f}" } else { " " };
                let check = if is_active { "\u{2714}" } else { "" };

                let label_style = if is_active {
                    Style::default()
                        .fg(theme::SAGE)
                        .add_modifier(Modifier::BOLD)
                } else if is_cursor {
                    Style::default()
                        .fg(theme::THINKING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                };

                let check_style = if is_active {
                    Style::default().fg(theme::SAGE)
                } else {
                    Style::default().fg(theme::MUTED)
                };

                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {} ", cursor_char),
                        Style::default().fg(theme::THINKING),
                    ),
                    Span::styled(format!("{:<8}", entry.alias.to_uppercase()), label_style),
                    Span::styled(format!(" {} ", check), check_style),
                    Span::styled(
                        entry.model_name.clone(),
                        Style::default().fg(theme::DIM),
                    ),
                ]));
            }
        }
    }

    // 填充
    while lines.len() < max_list_lines {
        lines.push(Line::from(""));
    }

    // ── 底部 ──
    let total = panel.rows.iter().filter(|r| matches!(r, ListRow::Entry(_))).count();
    lines.push(Line::from(Span::styled(
        format!("  {} models available", total),
        Style::default().fg(theme::DIM),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), list_area);
}

/// 第二步：选择 Effort 级别
fn render_effort_phase(
    f: &mut Frame,
    panel: &mut CommandPalettePanel,
    _app: &mut App,
    area: Rect,
) {
    let model_label = panel
        .selected_model
        .as_ref()
        .map(|e| format!("{} / {}", e.provider_name, e.alias.to_uppercase()))
        .unwrap_or_default();

    let inner = BorderedPanel::new(Span::styled(
        format!(" Select Effort ({}) ", model_label),
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 提示行
            Constraint::Length(1), // 分隔线
            Constraint::Min(1),   // 选项
        ])
        .split(inner);

    // ── 顶部提示行 ──
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " \u{2191}\u{2193} Move",
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                "  1-5 Quick",
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(
                "  Enter Confirm",
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                "  Esc/\u{2190} Back",
                Style::default().fg(theme::MUTED),
            ),
        ])),
        chunks[0],
    );

    // ── 分隔线 ──
    let sep = "\u{2500}".repeat(chunks[1].width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(theme::DIM))),
        chunks[1],
    );

    // ── 选项列表 ──
    let list_area = chunks[2];
    let mut lines: Vec<Line> = Vec::new();

    for (i, (key, desc)) in EFFORT_OPTIONS.iter().enumerate() {
        let is_cursor = panel.effort_cursor == i;
        let is_active = *key == panel.current_effort;

        let cursor_char = if is_cursor { "\u{276f}" } else { " " };
        let check = if is_active { "\u{2714}" } else { "" };
        let num = format!("{}.", i + 1);

        let style = if is_active {
            Style::default()
                .fg(theme::SAGE)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        let desc_style = if is_active {
            Style::default().fg(theme::SAGE)
        } else if is_cursor {
            Style::default().fg(theme::THINKING)
        } else {
            Style::default().fg(theme::DIM)
        };

        let check_style = if is_active {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::MUTED)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cursor_char),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("{:<2} {:<6}", num, key), style),
            Span::styled(format!(" {} ", check), check_style),
            Span::styled(desc.to_string(), desc_style),
        ]));
    }

    while lines.len() < list_area.height as usize {
        lines.push(Line::from(""));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), list_area);
}
