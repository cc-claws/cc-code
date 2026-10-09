use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph},
    Frame,
};

use cc_widgets::BorderedPanel;

use crate::{
    app::{tool_display::sanitize_display_text, App, ApprovalChoice},
    ui::{message_render::truncate_to_display_width, theme},
};

/// HITL 批量确认弹窗（底部展开区）
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &mut App, area: Rect) {
    // 边框两行、固定快捷键一行，剩余内容区用于滚动。
    if let Some(crate::app::InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    {
        prompt.last_visible_height = area.height.saturating_sub(3);
        prompt.keep_choice_visible();
    }
    let (scroll_offset, lines, inner, hint) = {
        let Some(crate::app::InteractionPrompt::Approval(prompt)) =
            &app.session_mgr.current().agent.interaction_prompt
        else {
            return;
        };
        let lc = &app.services.lc;
        let item_count = prompt.items.len();
        let popup_area = area;

        let title = if item_count == 1 {
            lc.tr("hitl-single-title")
        } else {
            lc.tr("hitl-batch-title")
        };

        let inner = BorderedPanel::new(Span::styled(
            title,
            Style::default()
                .fg(theme::THINKING)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(theme::WARNING))
        .render(f, popup_area);
        let max_width = inner.width as usize;

        let mut lines: Vec<Line> = Vec::new();
        for (i, (item, &choice)) in prompt.items.iter().zip(prompt.choices.iter()).enumerate() {
            let is_cursor = i == prompt.cursor;
            let (status_icon, status_color) = if choice.is_approved() {
                ("✓", theme::SAGE)
            } else {
                ("✗", theme::ERROR)
            };
            let cursor_indicator = if is_cursor { "❯ " } else { "  " };
            lines.push(Line::styled(
                truncate_to_display_width(
                    &format!("{}{} {}", cursor_indicator, status_icon, item.tool_name),
                    max_width,
                ),
                if is_cursor {
                    Style::default()
                        .fg(theme::THINKING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(status_color)
                },
            ));
            let input_preview = format_input_preview(&item.input, max_width.saturating_sub(6));
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(input_preview, Style::default().fg(theme::MUTED)),
            ]));
            for option in ApprovalChoice::ALL {
                let selected = choice == option;
                let marker = if selected { "●" } else { "○" };
                let indicator = if is_cursor && selected { "❯" } else { " " };
                let label = lc.tr(match option {
                    ApprovalChoice::Once => "hitl-choice-once",
                    ApprovalChoice::Session => "hitl-choice-session",
                    ApprovalChoice::Reject => "hitl-choice-reject",
                });
                lines.push(Line::styled(
                    truncate_to_display_width(
                        &format!("  {indicator} {marker} {label}"),
                        max_width,
                    ),
                    if is_cursor && selected {
                        Style::default()
                            .fg(theme::THINKING)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme::MUTED)
                    },
                ));
            }
        }
        if item_count > 1 {
            let approved_count = prompt.choices.iter().filter(|c| c.is_approved()).count() as i64;
            let rejected_count = prompt.choices.len() as i64 - approved_count;
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                lc.tr_args(
                    "hitl-summary",
                    &[
                        ("approved".into(), approved_count.into()),
                        ("rejected".into(), rejected_count.into()),
                    ],
                ),
                Style::default().fg(theme::MUTED),
            )));
        }
        (prompt.scroll_offset, lines, inner, lc.tr("hitl-key-hint"))
    };
    let content = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let footer = Rect {
        y: inner.y.saturating_add(content.height),
        height: inner.height.min(1),
        ..inner
    };
    let para = Paragraph::new(Text::from(lines)).scroll((scroll_offset, 0));
    f.render_widget(Clear, inner);
    f.render_widget(para, content);
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme::DIM)),
        footer,
    );
}

fn format_input_preview(input: &serde_json::Value, max_len: usize) -> String {
    let s = match input {
        serde_json::Value::Object(map) => {
            let key = ["command", "file_path", "pattern", "path"]
                .iter()
                .find(|k| map.contains_key(**k))
                .copied()
                .or_else(|| map.keys().next().map(|k| k.as_str()));

            if let Some(k) = key {
                if let Some(v) = map.get(k) {
                    let val = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    format!("{k}={val}")
                } else {
                    input.to_string()
                }
            } else {
                "{}".to_string()
            }
        }
        other => other.to_string(),
    };

    let s = sanitize_display_text(&s);
    truncate_to_display_width(&s, max_len)
}

#[cfg(test)]
mod tests {
    use crate::app::{App, HitlBatchPrompt, InteractionPrompt};
    use cc_middlewares::hitl::BatchItem;
    include!("hitl_test.rs");
}
