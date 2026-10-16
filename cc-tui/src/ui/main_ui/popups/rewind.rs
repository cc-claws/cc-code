//! Rewind 消息选择器弹窗渲染。
//!
//! 用户通过 [`RewindMode`] 三阶段选择回退节点：仅消息 → 消息+文件 → 确认回退文件。
//! 渲染逻辑参考 [`hitl`](super::hitl) 的 BorderedPanel 模式。

use cc_widgets::BorderedPanel;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::{
    app::{App, InteractionPrompt, RewindMode},
    ui::theme,
};

/// 渲染 Rewind 弹窗（底部展开区）。
pub(crate) fn render_rewind_popup(f: &mut Frame, app: &App, area: Rect) {
    let Some(InteractionPrompt::Rewind(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        return;
    };

    let inner = BorderedPanel::new(Span::styled(
        "Rewind",
        Style::default()
            .fg(theme::cur().claude)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::cur().claude))
    .render(f, area);
    let max_width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    // ── 消息列表 ──
    for (i, item) in prompt.items.iter().enumerate() {
        let is_cursor = i == prompt.cursor;
        let cursor_indicator = if is_cursor { "❯ " } else { "  " };

        // 字符级安全截断
        let max_summary = max_width.saturating_sub(12);
        let summary = if item.summary.chars().count() > max_summary {
            format!(
                "{}…",
                item.summary.chars().take(max_summary).collect::<String>()
            )
        } else {
            item.summary.clone()
        };
        let count_label = format!("({}msg)", item.message_count_after);

        lines.push(Line::from(vec![
            Span::styled(
                cursor_indicator,
                if is_cursor {
                    Style::default()
                        .fg(theme::cur().claude)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                summary,
                if is_cursor {
                    Style::default()
                        .fg(theme::cur().text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::cur().inactive)
                },
            ),
            Span::styled(
                format!(" {}", count_label),
                Style::default().fg(theme::cur().inactive),
            ),
        ]));
    }

    // ── 分隔 + 模式指示 ──
    lines.push(Line::from(""));
    let lc = &app.services.lc;
    let mode_label = match prompt.mode {
        RewindMode::MessagesOnly => lc.tr("rewind-mode-messages"),
        RewindMode::MessagesAndFiles => lc.tr("rewind-mode-messages-files"),
        RewindMode::ConfirmRevert => lc.tr("rewind-mode-confirm"),
    };
    lines.push(Line::from(vec![
        Span::styled(
            " Tab: ",
            Style::default()
                .fg(theme::cur().inactive)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(mode_label, Style::default().fg(theme::cur().claude)),
    ]));

    // ── 确认阶段：文件列表 ──
    if prompt.mode == RewindMode::ConfirmRevert {
        let selected = &prompt.items[prompt.cursor];
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            lc.tr("rewind-files-to-restore"),
            Style::default()
                .fg(theme::cur().warning)
                .add_modifier(Modifier::BOLD),
        )));
        for fc in &selected.file_changes {
            let op_label = match fc.operation.as_str() {
                "Write" => lc.tr("rewind-action-write"),
                "Edit" => lc.tr("rewind-action-edit"),
                _ => fc.operation.clone(),
            };
            let path_display: String = fc.path.chars().take(max_width.saturating_sub(20)).collect();
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(theme::cur().inactive)),
                Span::styled(path_display, Style::default().fg(theme::cur().text)),
                Span::styled(
                    format!(" ({})", op_label),
                    Style::default().fg(theme::cur().inactive),
                ),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            lc.tr("rewind-key-hint"),
            Style::default().fg(theme::cur().warning),
        )));
    }

    let para = Paragraph::new(Text::from(lines));
    f.render_widget(para, inner);
}
