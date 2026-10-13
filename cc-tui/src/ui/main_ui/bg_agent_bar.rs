//! 底栏抽屉：导航游标与当前视口解耦，按显示列宽对齐。
use ratatui::{layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::Paragraph, Frame};
use unicode_width::UnicodeWidthStr;
use crate::app::{App, subagent_session::{format_duration, SubagentRunStatus}};

pub(crate) fn bg_bar_height(app: &App, terminal_height: u16) -> u16 {
    let count = app.session_mgr.current().background_agents.len();
    let approval_height = u16::from(app.session_mgr.current().approval_source_agent_id.is_some());
    approval_height + if count == 0 { 0 } else if terminal_height < 18 { 1 } else { (count + 1).min(5) as u16 }
}

fn fit(text: &str, width: usize) -> String {
    let text = crate::app::tool_display::sanitize_display_text(text);
    let text = crate::ui::message_render::truncate_to_display_width(&text, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(text.as_str()));
    format!("{text}{}", " ".repeat(padding))
}

pub(crate) fn drawer_line(width: usize, selected: bool, active: bool, agent_type: &str, preview: &str, metrics: &str, color: Color) -> Line<'static> {
    let prefix = format!("{} {} ", if selected { ">" } else { " " }, if active { "•" } else { "○" });
    let usable = width.saturating_sub(1);
    let right_width = UnicodeWidthStr::width(metrics).min(usable.saturating_sub(4));
    let left_width = usable.saturating_sub(right_width + usize::from(right_width > 0));
    let type_width = left_width.saturating_sub(4).min(18);
    let preview_width = left_width.saturating_sub(4 + type_width);
    let left = format!("{}{}{}", prefix, fit(agent_type, type_width), fit(preview, preview_width));
    let line = format!("{}{}{} ", fit(&left, left_width), if right_width > 0 { " " } else { "" }, fit(metrics, right_width));
    let mut style = Style::default().fg(color);
    if selected { style = style.add_modifier(Modifier::REVERSED); }
    Line::from(Span::styled(fit(&line, width), style))
}

pub(crate) fn render_bg_agent_bar(f: &mut Frame, app: &mut App, mut area: Rect) {
    if area.height == 0 { return; }
    let session = app.session_mgr.current();
    if let Some(sub) = session.approval_source_agent_id.as_ref().and_then(|id| session.subagent_sessions.get(id)) {
        let hint = if matches!(session.agent.interaction_prompt, Some(crate::app::InteractionPrompt::Approval(_))) { "y:同意 / n:拒绝 / Enter:查看详情" } else { "Enter:查看问题" };
        let text = format!("[审批请求] 子Agent \"{}\": {} [{hint}]", sub.task_preview, sub.current_activity);
        f.render_widget(Paragraph::new(fit(&text, area.width as usize)).style(Style::default().fg(Color::Yellow)), Rect::new(area.x, area.y, area.width, 1));
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(1);
        if area.height == 0 { return; }
    }
    let cursor = session.ui.bg_bar_cursor;
    let current = session.focused_instance_id.as_deref();
    let width = area.width as usize;
    if area.height == 1 {
        let selected = cursor.unwrap_or(0);
        let text = if let Some(agent) = selected.checked_sub(1).and_then(|index| session.background_agents.get(index)) {
            let sub = session.subagent_sessions.get(&agent.instance_id);
            format!("[{} {}] ↑/↓ · Enter to view · x to stop", if current == Some(&agent.instance_id) { "•" } else { "○" }, sub.map(|sub| sub.task_preview.as_str()).unwrap_or(&agent.agent_name))
        } else {
            let mut parts = vec![format!("{} main", if current.is_none() { "•" } else { "○" })];
            parts.extend(session.background_agents.iter().map(|agent| {
                let sub = session.subagent_sessions.get(&agent.instance_id);
                format!("{} {}", if current == Some(&agent.instance_id) { "•" } else { "○" }, sub.map(|sub| sub.task_preview.as_str()).unwrap_or(&agent.agent_name))
            }));
            format!("[{}] (↓ 展开管理)", parts.join(" | "))
        };
        f.render_widget(Paragraph::new(fit(&text, width)).style(Style::default().fg(Color::Cyan)), area);
        return;
    }
    let mut lines = vec![drawer_line(width, cursor == Some(0), current.is_none(), "main", "", "", Color::Cyan)];
    let overflow = session.background_agents.len() + 1 > area.height as usize;
    let capacity = (area.height as usize).saturating_sub(1 + usize::from(overflow));
    let start = cursor.unwrap_or(0).saturating_sub(capacity).min(session.background_agents.len().saturating_sub(capacity));
    for (index, agent) in session.background_agents.iter().enumerate().skip(start).take(capacity) {
        let sub = session.subagent_sessions.get(&agent.instance_id);
        let (preview, metrics, color) = if let Some(sub) = sub {
            let metrics = match sub.status {
                SubagentRunStatus::Completed => "✓ completed".into(),
                SubagentRunStatus::Stopped => "✗ stopped".into(),
                SubagentRunStatus::Failed if sub.final_result.as_deref().is_some_and(|result| result.contains("timed out")) => "✗ timeout".into(),
                SubagentRunStatus::Failed => "✗ failed".into(),
                _ => format!("{} · ↓ {} tokens", format_duration(sub.elapsed()), sub.tokens_used()),
            };
            let color = match sub.status {
                SubagentRunStatus::AwaitingApproval => Color::Yellow,
                SubagentRunStatus::Failed | SubagentRunStatus::Stopped => Color::Red,
                SubagentRunStatus::Completed => Color::Green,
                _ => Color::Cyan,
            };
            let preview = if sub.current_activity.is_empty() { &sub.task_preview } else { &sub.current_activity };
            (preview.as_str(), metrics, color)
        } else { ("", format_duration(agent.started_at.elapsed()), Color::Cyan) };
        lines.push(drawer_line(width, cursor == Some(index + 1), current == Some(&agent.instance_id), &agent.agent_name, preview, &metrics, color));
    }
    if overflow { lines.push(Line::from(format!("  …+{} more", session.background_agents.len().saturating_sub(capacity)))); }
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "bg_agent_bar_test.rs"]
mod tests;
