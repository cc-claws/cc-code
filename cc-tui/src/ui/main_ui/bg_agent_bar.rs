//! 底栏抽屉：导航游标与当前视口解耦，按显示列宽对齐。
use ratatui::{layout::Rect, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::Paragraph, Frame};
use unicode_width::UnicodeWidthStr;
use crate::app::{App, subagent_session::{format_duration, SubagentRunStatus}};
use crate::ui::theme;

pub(crate) fn bg_bar_height(app: &App, terminal_height: u16) -> u16 {
    let count = app.session_mgr.current().background_agents.len();
    let approval_height = u16::from(app.session_mgr.current().approval_source_agent_id.is_some());
    // 末尾 +1 为抽屉上方的空行（与状态栏分隔）
    approval_height
        + if count == 0 {
            0
        } else if terminal_height < 18 {
            2
        } else {
            (count + 1).min(5) as u16 + 1
        }
}

fn fit(text: &str, width: usize) -> String {
    let text = crate::app::tool_display::sanitize_display_text(text);
    let text = crate::ui::message_render::truncate_to_display_width(&text, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(text.as_str()));
    format!("{text}{}", " ".repeat(padding))
}

/// 抽屉单行：列宽与参考原型一致（游标 2 列 + 圆点 2 列 + 类型名 ≤18 列 + 弹性预览 + 右对齐指标）。
///
/// 配色：`bright`（键盘游标所在行）用亮白，其余行统一灰，形成"只有选中的那行亮"的层次；
/// 圆点 `●`/`◯` 单独表达当前视口；`accent` 仅在示警（待审批/失败/停止）时覆盖圆点与右侧指标。
// 9 个入参都是抽屉单行的独立显示输入（宽度 / 游标 / 当前视口 / 高亮 / 类型 / 预览 /
// 指标 / 告警色 / 实心圆点），拆结构体反而让调用点更绕；此处显式豁免该 lint。
#[allow(clippy::too_many_arguments)]
pub(crate) fn drawer_line(
    width: usize,
    selected: bool,
    active: bool,
    bright: bool,
    agent_type: &str,
    preview: &str,
    metrics: &str,
    accent: Option<Color>,
    // 强制实心圆点 `●`（覆盖 `active` 的空心 `◯`）。停止态用它配合 `accent=ERROR`
    // 呈现「红 ● + stopped by …」：红实心比空心情形更醒目，用户一眼确认已停（PRD §4.4）。
    force_filled: bool,
) -> Line<'static> {
    let usable = width.saturating_sub(1);
    let right_width = UnicodeWidthStr::width(metrics).min(usable.saturating_sub(4));
    let left_width = usable.saturating_sub(right_width + usize::from(right_width > 0));
    let type_width = left_width.saturating_sub(4).min(18);
    let preview_width = left_width.saturating_sub(4 + type_width);

    // 只用前景色区分游标行（真机定稿）：键盘上下移动时**不得**出现整行底色，
    // 底色会让人以为「已经进这个视口了」，而 Enter 才是进入。
    let st = move |color: Color| Style::default().fg(color);

    // 前缀（游标 + 圆点）固定 4 列，但极窄终端必须按 left_width 收缩，否则整行会越界。
    let truncate = crate::ui::message_render::truncate_to_display_width;
    let prefix_width = left_width.min(4);
    let cursor_cols = prefix_width.min(2);
    let circle_cols = prefix_width - cursor_cols;
    let cursor_text = truncate(if selected { "> " } else { "  " }, cursor_cols);
    let circle_text = truncate(
        &format!("{} ", if active || force_filled { "●" } else { "◯" }),
        circle_cols,
    );

    let circle_color = accent.unwrap_or(if active {
        theme::cur().drawer_circle_on
    } else {
        theme::cur().drawer_muted
    });
    // 正文只有两档：游标所在行亮白、其余统一灰。不要在抽屉里堆中间色（
    // 不同灰阶叠在一起会看起来「花」，参考实现整块只有 muted + bright）。
    let (cursor_color, body_color) = if bright {
        (theme::cur().drawer_text_active, theme::cur().drawer_text_active)
    } else {
        (theme::cur().drawer_muted, theme::cur().drawer_muted)
    };
    let mut spans = vec![
        Span::styled(cursor_text, st(cursor_color).add_modifier(Modifier::BOLD)),
        Span::styled(circle_text, st(circle_color)),
        Span::styled(
            fit(agent_type, type_width),
            if bright { st(body_color).add_modifier(Modifier::BOLD) } else { st(body_color) },
        ),
        Span::styled(fit(preview, preview_width), st(body_color)),
    ];
    if right_width > 0 {
        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(
            fit(metrics, right_width),
            st(accent.unwrap_or(body_color)),
        ));
    }
    let used = left_width + if right_width > 0 { 1 + right_width } else { 0 };
    let pad = width.saturating_sub(used);
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), Style::default()));
    }
    Line::from(spans)
}

pub(crate) fn render_bg_agent_bar(f: &mut Frame, app: &mut App, mut area: Rect) {
    if area.height == 0 { return; }
    let session = app.session_mgr.current();
    if let Some(sub) = session.approval_source_agent_id.as_ref().and_then(|id| session.subagent_sessions.get(id)) {
        let hint = if matches!(session.agent.interaction_prompt, Some(crate::app::InteractionPrompt::Approval(_))) { "y:同意 / n:拒绝 / Enter:查看详情" } else { "Enter:查看问题" };
        // 审批条要的是「正在申请执行什么」，用运行中工具；抽屉正文才是助手文本。
        let action = sub
            .agent
            .active_tool
            .as_ref()
            .map(|tool| format!("{}({})", tool.display, tool.args_summary))
            .unwrap_or_else(|| sub.current_activity.clone());
        let text = format!("[审批请求] 子Agent \"{}\": {} [{hint}]", sub.task_preview, action);
        f.render_widget(Paragraph::new(fit(&text, area.width as usize)).style(Style::default().fg(theme::cur().warning)), Rect::new(area.x, area.y, area.width, 1));
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(1);
        if area.height == 0 { return; }
    }
    let cursor = session.ui.bg_bar_cursor;
    let current = session.focused_instance_id.as_deref();
    let width = area.width as usize;
    // 抽屉与状态栏之间保留一个空行（对齐参考实现，避免"贴身"）。
    let content_height = area.height.saturating_sub(1) as usize;
    if content_height == 1 {
        let selected = cursor.unwrap_or(0);
        let text = if let Some(agent) = selected.checked_sub(1).and_then(|index| session.background_agents.get(index)) {
            let sub = session.subagent_sessions.get(&agent.instance_id);
            format!("[{} {}] ↑/↓ · Enter to view · x to stop", if current == Some(&agent.instance_id) { "●" } else { "◯" }, sub.map(|sub| sub.task_preview.as_str()).unwrap_or(&agent.agent_name))
        } else {
            let mut parts = vec![format!("{} main", if current.is_none() { "●" } else { "◯" })];
            parts.extend(session.background_agents.iter().map(|agent| {
                let sub = session.subagent_sessions.get(&agent.instance_id);
                format!("{} {}", if current == Some(&agent.instance_id) { "●" } else { "◯" }, sub.map(|sub| sub.task_preview.as_str()).unwrap_or(&agent.agent_name))
            }));
            format!("[{}] (↓ 展开管理)", parts.join(" | "))
        };
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(fit(&text, width), Style::default().fg(theme::cur().drawer_text_active))),
        ];
        f.render_widget(Paragraph::new(lines), area);
        return;
    }
    // 亮白高亮行 = 键盘游标所在行；游标不在抽屉时退回「当前视口」那一行，
    // 避免整个抽屉全灰、看不出自己在哪。
    let bright_for = |index: usize, is_active: bool| -> bool {
        match cursor {
            Some(cursor) => cursor == index,
            None => is_active,
        }
    };
    let mut lines = vec![
        Line::from(""),
        drawer_line(
            width,
            cursor == Some(0),
            current.is_none(),
            bright_for(0, current.is_none()),
            "main", "", "", None,
            false,
        ),
    ];
    let overflow = session.background_agents.len() + 1 > content_height;
    let capacity = content_height.saturating_sub(1 + usize::from(overflow));
    let start = cursor.unwrap_or(0).saturating_sub(capacity).min(session.background_agents.len().saturating_sub(capacity));
    for (index, agent) in session.background_agents.iter().enumerate().skip(start).take(capacity) {
        let sub = session.subagent_sessions.get(&agent.instance_id);
        let (preview, metrics, accent) = if let Some(sub) = sub {
            let metrics = match sub.status {
                SubagentRunStatus::Completed => "✓ completed".into(),
                // 停止：`stopped by user|main Agent` 区分发起方（PRD §4.4.1），
                // 与完成/失败/超时三种终态各自的文案并列，用户一眼看出是谁停的。
                SubagentRunStatus::Stopped => {
                    let by = if sub.stopped_by_user { "user" } else { "main Agent" };
                    format!("stopped by {by}")
                }
                SubagentRunStatus::Failed if sub.final_result.as_deref().is_some_and(|result| result.contains("timed out")) => "✗ timeout".into(),
                SubagentRunStatus::Failed => "✗ failed".into(),
                // 单个箭头 + 最近一次用量：`↓` = Agent 回复产出，`↑` = 用户提问输入。
                _ => match sub.token_metrics() {
                    Some(metrics) => format!("{} · {metrics}", format_duration(sub.elapsed())),
                    None => format_duration(sub.elapsed()),
                },
            };
            // 只在需要示警或收尾提示时上色；正常行保持白灰层次。
            // 已完成必须转绿：用户要能一眼看出「它结束了，我看完就可以走了」（对齐参考实现）。
            let accent = match sub.status {
                SubagentRunStatus::AwaitingApproval => Some(theme::cur().warning),
                SubagentRunStatus::Completed => Some(theme::cur().success),
                SubagentRunStatus::Failed | SubagentRunStatus::Stopped => Some(theme::cur().error),
                _ => None,
            };
            let preview = if sub.current_activity.is_empty() { &sub.task_preview } else { &sub.current_activity };
            (preview.as_str(), metrics, accent)
        } else { ("", format_duration(agent.started_at.elapsed()), None) };
        let is_active = current == Some(&agent.instance_id);
        lines.push(drawer_line(
            width,
            cursor == Some(index + 1),
            is_active,
            bright_for(index + 1, is_active),
            &agent.agent_name, preview, &metrics, accent,
            // 停止态强制实心 ●，配合 accent=ERROR 呈现红 ●（PRD §4.4）。
            sub.is_some_and(|sub| sub.status == SubagentRunStatus::Stopped),
        ));
    }
    if overflow { lines.push(Line::from(format!("  …+{} more", session.background_agents.len().saturating_sub(capacity)))); }
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "bg_agent_bar_test.rs"]
mod tests;
