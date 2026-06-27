use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use peri_widgets::BorderedPanel;

use crate::{app::App, ui::theme};

/// HITL 批量确认弹窗（底部展开区）
pub(crate) fn render_hitl_popup(f: &mut Frame, app: &mut App, area: Rect) {
    // 先借不可变引用读完渲染需要的纯数据，drop 借用后再写入 last_visible_height
    let (scroll_offset, lines, inner, inner_height) = {
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
        let inner_height = inner.height;
        let max_width = inner.width as usize;

        let mut lines: Vec<Line> = Vec::new();
        for (i, (item, &approved)) in prompt.items.iter().zip(prompt.approved.iter()).enumerate() {
            let is_cursor = i == prompt.cursor;
            let (status_icon, status_color) = if approved {
                ("✓", theme::SAGE)
            } else {
                ("✗", theme::ERROR)
            };
            let cursor_indicator = if is_cursor { "❯ " } else { "  " };
            let approved_label = if approved {
                lc.tr("hitl-approved")
            } else {
                lc.tr("hitl-rejected")
            };
            lines.push(Line::styled(
                format!(
                    "{}{} {}  {}",
                    cursor_indicator, status_icon, item.tool_name, approved_label
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
        }
        if item_count > 1 {
            let approved_count = prompt.approved.iter().filter(|&&v| v).count() as i64;
            let rejected_count = prompt.approved.iter().filter(|&&v| !v).count() as i64;
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
        (prompt.scroll_offset, lines, inner, inner_height)
    };

    // 写入 last_visible_height 供 hitl_move 使用
    if let Some(crate::app::InteractionPrompt::Approval(p)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    {
        p.last_visible_height = inner_height;
    }

    let para = Paragraph::new(Text::from(lines)).scroll((scroll_offset, 0));
    f.render_widget(para, inner);
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

    if s.chars().count() > max_len && max_len > 1 {
        format!("{}…", s.chars().take(max_len - 1).collect::<String>())
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{App, HitlBatchPrompt, InteractionPrompt};
    use peri_middlewares::hitl::BatchItem;
    include!("hitl_test.rs");
}
