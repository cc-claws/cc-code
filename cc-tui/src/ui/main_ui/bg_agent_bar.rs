//! 底栏抽屉：导航游标与当前视口解耦，按显示列宽对齐。
use ratatui::{layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::Paragraph, Frame};
use unicode_width::UnicodeWidthStr;
use crate::app::{App, subagent_session::{format_duration, SubagentRunStatus}};
use crate::ui::theme;

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

/// 抽屉单行：列宽与参考原型一致（游标 2 列 + 圆点 2 列 + 类型名 ≤18 列 + 弹性预览 + 右对齐指标）。
///
/// 配色取自参考原型 `docs/subagent-mock.html`：正文白灰、游标亮青、圆点区分当前视口；
/// `accent` 仅在需要示警（待审批/失败/停止）时覆盖圆点与右侧指标的颜色。
pub(crate) fn drawer_line(
    width: usize,
    selected: bool,
    active: bool,
    agent_type: &str,
    preview: &str,
    metrics: &str,
    accent: Option<Color>,
) -> Line<'static> {
    let usable = width.saturating_sub(1);
    let right_width = UnicodeWidthStr::width(metrics).min(usable.saturating_sub(4));
    let left_width = usable.saturating_sub(right_width + usize::from(right_width > 0));
    let type_width = left_width.saturating_sub(4).min(18);
    let preview_width = left_width.saturating_sub(4 + type_width);

    let bg = selected.then_some(theme::DRAWER_SELECTED_BG);
    let st = move |color: Color| match bg {
        Some(bg) => Style::default().fg(color).bg(bg),
        None => Style::default().fg(color),
    };

    // 前缀（游标 + 圆点）固定 4 列，但极窄终端必须按 left_width 收缩，否则整行会越界。
    let truncate = crate::ui::message_render::truncate_to_display_width;
    let prefix_width = left_width.min(4);
    let cursor_cols = prefix_width.min(2);
    let circle_cols = prefix_width - cursor_cols;
    let cursor_text = truncate(if selected { "> " } else { "  " }, cursor_cols);
    let circle_text = truncate(&format!("{} ", if active { "•" } else { "○" }), circle_cols);

    let circle_color = accent.unwrap_or(if active {
        theme::DRAWER_CIRCLE_ON
    } else {
        theme::DRAWER_MUTED
    });
    let mut spans = vec![
        Span::styled(cursor_text, st(theme::DRAWER_CURSOR).add_modifier(Modifier::BOLD)),
        Span::styled(circle_text, st(circle_color)),
        Span::styled(
            fit(agent_type, type_width),
            if active {
                st(theme::DRAWER_TEXT_ACTIVE).add_modifier(Modifier::BOLD)
            } else {
                st(theme::DRAWER_TEXT)
            },
        ),
        Span::styled(fit(preview, preview_width), st(theme::DRAWER_PREVIEW)),
    ];
    if right_width > 0 {
        spans.push(Span::styled(" ", st(Color::Reset)));
        spans.push(Span::styled(
            fit(metrics, right_width),
            st(accent.unwrap_or(theme::DRAWER_MUTED)),
        ));
    }
    let used = left_width + if right_width > 0 { 1 + right_width } else { 0 };
    let pad = width.saturating_sub(used);
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), st(Color::Reset)));
    }
    Line::from(spans)
}

pub(crate) fn render_bg_agent_bar(f: &mut Frame, app: &mut App, mut area: Rect) {
    if area.height == 0 { return; }
    let session = app.session_mgr.current();
    if let Some(sub) = session.approval_source_agent_id.as_ref().and_then(|id| session.subagent_sessions.get(id)) {
        let hint = if matches!(session.agent.interaction_prompt, Some(crate::app::InteractionPrompt::Approval(_))) { "y:同意 / n:拒绝 / Enter:查看详情" } else { "Enter:查看问题" };
        let text = format!("[审批请求] 子Agent \"{}\": {} [{hint}]", sub.task_preview, sub.current_activity);
        f.render_widget(Paragraph::new(fit(&text, area.width as usize)).style(Style::default().fg(theme::WARNING)), Rect::new(area.x, area.y, area.width, 1));
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
        f.render_widget(Paragraph::new(fit(&text, width)).style(Style::default().fg(theme::DRAWER_CURSOR)), area);
        return;
    }
    let mut lines = vec![drawer_line(width, cursor == Some(0), current.is_none(), "main", "", "", None)];
    let overflow = session.background_agents.len() + 1 > area.height as usize;
    let capacity = (area.height as usize).saturating_sub(1 + usize::from(overflow));
    let start = cursor.unwrap_or(0).saturating_sub(capacity).min(session.background_agents.len().saturating_sub(capacity));
    for (index, agent) in session.background_agents.iter().enumerate().skip(start).take(capacity) {
        let sub = session.subagent_sessions.get(&agent.instance_id);
        let (preview, metrics, accent) = if let Some(sub) = sub {
            let metrics = match sub.status {
                SubagentRunStatus::Completed => "✓ completed".into(),
                SubagentRunStatus::Stopped => "✗ stopped".into(),
                SubagentRunStatus::Failed if sub.final_result.as_deref().is_some_and(|result| result.contains("timed out")) => "✗ timeout".into(),
                SubagentRunStatus::Failed => "✗ failed".into(),
                _ => format!("{} · ↓ {} tokens", format_duration(sub.elapsed()), sub.tokens_used()),
            };
            // 只在需要示警时上色；正常/完成行保持参考图的白灰配色。
            let accent = match sub.status {
                SubagentRunStatus::AwaitingApproval => Some(theme::WARNING),
                SubagentRunStatus::Failed | SubagentRunStatus::Stopped => Some(theme::ERROR),
                _ => None,
            };
            let preview = if sub.current_activity.is_empty() { &sub.task_preview } else { &sub.current_activity };
            (preview.as_str(), metrics, accent)
        } else { ("", format_duration(agent.started_at.elapsed()), None) };
        lines.push(drawer_line(width, cursor == Some(index + 1), current == Some(&agent.instance_id), &agent.agent_name, preview, &metrics, accent));
    }
    if overflow { lines.push(Line::from(format!("  …+{} more", session.background_agents.len().saturating_sub(capacity)))); }
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "bg_agent_bar_test.rs"]
mod tests;
