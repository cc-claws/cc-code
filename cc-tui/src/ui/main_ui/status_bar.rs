use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::app::{tool_display, App};
use crate::ui::theme::{self, ThemeName};

/// `default` 主题下的状态栏配色字面量。
///
/// 这些值是 `default` 主题打磨定稿的观感，切到其它主题再切回来必须保持不变：
/// 模型名 / cwd / git 分支 / `✓` / 上下文仪表沿用 ANSI 具名色（随终端调色板解析），
/// 其余沿用 #509 之前的写死 RGB。
///
/// **非 `default` 主题不读这里**——它们经 [`SbColors`] 从 `/theme` 调色板取值。
/// 状态栏若一律写死，浅色主题下会出现白字贴浅底（`LIGHT` 的 `text` 是纯黑）与
/// 同一行内「ANSI 名色跟着终端漂、RGB 不漂」的割裂。新增状态栏元素请从
/// [`SbColors`] 取色，不要再直接引用本模块常量。
mod sb {
    use ratatui::style::Color;

    /// 原 `theme::MUTED`——次要信息 / 标签 / 分隔点
    pub const MUTED: Color = Color::Rgb(153, 153, 153);
    /// 原 `theme::WARNING`——警告态
    pub const WARNING: Color = Color::Rgb(255, 193, 7);
    /// 原 `theme::ERROR`——错误态
    pub const ERROR: Color = Color::Rgb(255, 107, 128);
    /// 原 `theme::SAGE`——成功 / 在线态
    pub const SAGE: Color = Color::Rgb(78, 186, 101);
    /// 原 `theme::TEXT`——主文字
    pub const TEXT: Color = Color::Rgb(255, 255, 255);
    /// 原 `theme::SELECTION_BG`——待办 pill 聚焦底色
    pub const SELECTION_BG: Color = Color::Rgb(38, 79, 120);
    /// 原 `theme::CURSOR_BG`——待办 pill 常态底色
    pub const CURSOR_BG: Color = Color::Rgb(38, 38, 38);
}

/// 一帧状态栏渲染所需的全部颜色。
///
/// `default` 主题取 [`sb`] 的打磨定稿字面量（逐字节保持既有观感），其余主题取
/// `/theme` 当前调色板的对应字段，使状态栏与消息区在浅色主题下保持同一套对比度。
#[derive(Clone, Copy)]
struct SbColors {
    /// 模型名 / 工具名 / git 分支名（默认主题为 ANSI Cyan）
    cyan: Color,
    /// cwd / 运行中工具指示（默认主题为 ANSI Yellow）
    yellow: Color,
    /// 完成指示 `✓`（默认主题为 ANSI Green）
    green: Color,
    /// git 前缀 `git:(`（默认主题为 ANSI Magenta）
    magenta: Color,
    /// 次要信息 / 标签 / 分隔点
    muted: Color,
    /// 警告态
    warning: Color,
    /// 错误态
    error: Color,
    /// 成功 / 在线态
    success: Color,
    /// 主文字
    text: Color,
    /// 上下文仪表：充足（默认主题为 ANSI Green）
    context_ok: Color,
    /// 上下文仪表：预警（默认主题为 ANSI Yellow）
    context_warn: Color,
    /// 上下文仪表：危急（默认主题为 ANSI Red）
    context_high: Color,
    /// 聚焦 pill 底色
    selection_bg: Color,
    /// pill 常态底色
    cursor_bg: Color,
}

impl SbColors {
    fn for_theme(name: ThemeName) -> Self {
        if matches!(name, ThemeName::Default) {
            return Self {
                cyan: Color::Cyan,
                yellow: Color::Yellow,
                green: Color::Green,
                magenta: Color::Magenta,
                muted: sb::MUTED,
                warning: sb::WARNING,
                error: sb::ERROR,
                success: sb::SAGE,
                text: sb::TEXT,
                context_ok: Color::Green,
                context_warn: Color::Yellow,
                context_high: Color::Red,
                selection_bg: sb::SELECTION_BG,
                cursor_bg: sb::CURSOR_BG,
            };
        }
        let p = name.palette();
        Self {
            cyan: p.cyan,
            yellow: p.yellow,
            green: p.success,
            magenta: p.magenta,
            muted: p.inactive,
            warning: p.warning,
            error: p.error,
            success: p.success,
            text: p.text,
            context_ok: p.success,
            context_warn: p.warning,
            context_high: p.error,
            selection_bg: p.selection_bg,
            cursor_bg: p.cursor_bg,
        }
    }

    /// 当前 `/theme` 对应的配色。
    fn current() -> Self {
        Self::for_theme(theme::current_name())
    }
}

const CONTEXT_WARNING_PCT: f64 = 70.0;
const CONTEXT_CRITICAL_PCT: f64 = 85.0;
const TOOLS_MAX_VISIBLE: usize = 4;
/// 状态栏「最近工具」args 摘要的字符数上限（首版 30，后续按用户反馈调整）
const TOOL_TARGET_MAX_LEN: usize = 30;

pub(crate) fn status_bar_height(app: &App) -> u16 {
    if has_hud_activity(app) {
        3
    } else {
        2
    }
}

pub(crate) fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let show_activity = has_hud_activity(app);
    let constraints = if show_activity {
        vec![
            Constraint::Length(1), // 第一行：模型 + context + project + stats
            Constraint::Length(1), // 第二行：工具 / Agent / Todo activity
            Constraint::Length(1), // 第三行：权限/瞬时状态 + CPU/MEM + 快捷键
        ]
    } else {
        vec![
            Constraint::Length(1), // 第一行：模型 + context + project + stats
            Constraint::Length(1), // 第二行折叠，直接显示 Peri 状态行
        ]
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_first_row(f, app, rows[0]);
    if show_activity {
        render_second_row(f, app, rows[1]);
        render_third_row(f, app, rows[2]);
    } else {
        render_third_row(f, app, rows[1]);
    }
}

/// 抽屉导航态的第三行提示。
///
/// 游标停在「当前已经在查看的那一行」时，`Enter to view` 与 `x to stop` 都无意义
/// （已经在看它了 / 只有 `x` 有意义），此时缩短为纯导航提示。
fn drawer_hint(app: &App) -> Option<String> {
    let session = app.session_mgr.current();
    if !session.drawer_nav_active() {
        return None;
    }
    let cursor = session.ui.bg_bar_cursor?;
    // 游标 0 = main 行（对应 focused_instance_id == None）
    let target_id = cursor
        .checked_sub(1)
        .and_then(|index| session.background_agents.get(index))
        .map(|agent| agent.instance_id.as_str());
    let lc = &app.services.lc;
    Some(if target_id == session.focused_instance_id.as_deref() {
        format!(" {}", lc.tr("bg-drawer-hint-short"))
    } else {
        format!(" {}", lc.tr("bg-drawer-hint-full"))
    })
}

fn has_hud_activity(app: &App) -> bool {
    let session = app.session_mgr.current();
    let agent = session.viewport_agent();
    let now = std::time::Instant::now();
    !agent.running_tools.is_empty()
        || agent.has_visible_recent_tools(now)
        || !agent.session_tool_stats.is_empty()
        || !session.todo_items.is_empty()
}

/// 第一行（codebuddy-hud compact）：[model] context | project git:(branch*) | stats
fn render_first_row(f: &mut Frame, app: &App, area: Rect) {
    let colors = SbColors::current();
    let mut spans: Vec<Span> = Vec::new();

    spans.push(Span::styled("  ", plain_style()));

    // Model + context bar 是一个 segment，中间用空格，不插入 ` | `。
    let is_highlight = app
        .global_ui
        .model_highlight_until
        .is_some_and(|until| std::time::Instant::now() < until);
    let mut model_style = fg_style(colors.cyan);
    if is_highlight {
        model_style = model_style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
    }
    spans.push(Span::styled(
        format!(
            "[{}]",
            app.session_mgr
                .current()
                .active_subagent()
                .map(|sub| sub.model.as_str())
                .filter(|model| !model.is_empty())
                .unwrap_or(&app.services.model_name)
        ),
        model_style,
    ));

    {
        let agent = app.session_mgr.current().viewport_agent();
        let tracker = &agent.session_token_tracker;
        let pct = tracker
            .context_usage_percent(agent.context_window)
            .unwrap_or(0.0);
        let color = context_usage_color(pct, &colors);
        spans.push(Span::styled(" ", plain_style()));
        spans.extend(render_context_bar(pct, color));
    }

    spans.push(status_separator());
    append_project_segment(&mut spans, app, &colors);

    let stats = render_stats_segments(app);
    if !stats.is_empty() {
        spans.push(status_separator());
        spans.extend(stats);
    }

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行（codebuddy-hud activity）：最近工具 | completed tools | agents | tasks
///
/// 「最近工具」组由 [`crate::app::AgentComm::recent_tools`] 驱动：最新在最左、最多 2 条，
/// 运行中显示 `◐`、结束后转 `✓` 并短暂停留（见 `TOOL_MIN_VISIBLE_MS`）。
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let colors = SbColors::current();
    let mut left_spans: Vec<Span> = Vec::new();

    let agent = app.session_mgr.current().viewport_agent();
    let now = std::time::Instant::now();
    for entry in agent.visible_recent_tools(now) {
        append_activity_segment(&mut left_spans, render_recent_tool_segment(entry, &colors));
    }

    for segment in render_completed_tool_segments(agent, &colors) {
        append_activity_segment(&mut left_spans, segment);
    }

    if app.session_mgr.current().active_subagent().is_none() {
        if let Some(segment) = render_tasks_segment(app) {
            append_activity_segment(&mut left_spans, segment);
        }
    }
    // 第二行强制单行：超宽时按显示列宽截断并以 … 收口（禁止折行）
    let left_spans = truncate_spans_to_width(left_spans, area.width as usize);
    render_truncated_line(f, left_spans, Vec::new(), area);
}

fn append_project_segment(spans: &mut Vec<Span<'_>>, app: &App, colors: &SbColors) {
    let cwd_short = std::path::Path::new(&app.services.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.services.cwd);
    spans.push(Span::styled(cwd_short.to_string(), fg_style(colors.yellow)));

    // 分支状态只读缓存：探测由 `App::request_git_branch_refresh_if_due` 在事件循环中
    // 异步触发（子进程不占用渲染线程），故此处**无需**按 loading 区分快慢路径，
    // 也不会出现「Agent 工作期间分支名长期陈旧」（issue #277）。
    if let Some(status) = app.services.git_branch_cache.lock().get_cached() {
        append_git_status(spans, status, colors);
    }
}

fn render_stats_segments(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let agent = app.session_mgr.current().viewport_agent();
    let tracker = &agent.session_token_tracker;

    if tracker.total_input_tokens > 0 || tracker.total_output_tokens > 0 {
        spans.push(Span::styled(
            format!(
                "tok: {} (in: {}, out: {})",
                format_token_count(tracker.total_input_tokens + tracker.total_output_tokens),
                format_token_count(
                    tracker
                        .last_usage
                        .as_ref()
                        .map(|usage| usage.input_tokens as u64)
                        .unwrap_or(0)
                ),
                format_token_count(
                    tracker
                        .last_usage
                        .as_ref()
                        .map(|usage| usage.output_tokens as u64)
                        .unwrap_or(0)
                )
            ),
            dim_style(),
        ));
    }

    if let Some(start) = agent.session_start_time {
        if !spans.is_empty() {
            spans.push(status_separator());
        }
        spans.push(Span::styled(
            format!(
                "⏱  {}",
                format_duration_display(
                    app.session_mgr
                        .current()
                        .active_subagent()
                        .map(|sub| sub.elapsed())
                        .unwrap_or_else(|| start.elapsed())
                )
            ),
            dim_style(),
        ));
    }

    spans
}

fn append_activity_segment(spans: &mut Vec<Span<'_>>, segment: Vec<Span<'static>>) {
    if spans.is_empty() {
        spans.push(Span::styled("  ", plain_style()));
    } else {
        spans.push(status_separator());
    }
    spans.extend(segment);
}

/// 最近工具条目：运行中 `◐`（黄）/ 已完成停留期 `✓`（绿），颜色不跟工具身份色走
fn render_recent_tool_segment(
    entry: &crate::app::RecentToolEntry,
    colors: &SbColors,
) -> Vec<Span<'static>> {
    let indicator = if entry.running {
        Span::styled("◐", fg_style(colors.yellow))
    } else {
        Span::styled("✓", fg_style(colors.green))
    };
    let mut spans = vec![
        indicator,
        Span::styled(" ", plain_style()),
        Span::styled(entry.display.clone(), fg_style(colors.cyan)),
    ];
    if !entry.args_summary.is_empty() {
        spans.push(Span::styled(
            format!(
                " : {}",
                truncate_tool_target(&entry.args_summary, TOOL_TARGET_MAX_LEN)
            ),
            dim_style(),
        ));
    }
    spans
}

/// 单行截断：按显示列宽（unicode-width）裁掉超出部分并以 `…` 收口
pub(crate) fn truncate_spans_to_width(
    spans: Vec<Span<'static>>,
    max_width: usize,
) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthChar;

    let total: usize = spans.iter().map(|span| span.width()).sum();
    if total <= max_width || max_width == 0 {
        return spans;
    }

    let limit = max_width.saturating_sub(1);
    let mut used = 0usize;
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut overflowed = false;
    for span in spans {
        let mut buf = String::new();
        for ch in span.content.chars() {
            let width = ch.width().unwrap_or(0);
            if used + width > limit {
                overflowed = true;
                break;
            }
            buf.push(ch);
            used += width;
        }
        if !buf.is_empty() {
            out.push(Span::styled(buf, span.style));
        }
        if overflowed {
            break;
        }
    }
    out.push(Span::styled("…", dim_style()));
    out
}

fn render_completed_tool_segments(
    agent: &crate::app::AgentComm,
    colors: &SbColors,
) -> Vec<Vec<Span<'static>>> {
    if agent.session_tool_stats.is_empty() {
        return Vec::new();
    }

    let mut entries: Vec<_> = agent.session_tool_stats.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let total = entries.len();
    let mut segments = Vec::new();
    for (name, count) in entries.into_iter().take(TOOLS_MAX_VISIBLE) {
        let display = tool_display::format_tool_name(name);
        segments.push(vec![
            Span::styled("✓", fg_style(colors.green)),
            Span::styled(format!(" {}", display), plain_style()),
            Span::styled(format!(" ×{}", count), dim_style()),
        ]);
    }

    if total > TOOLS_MAX_VISIBLE {
        segments.push(vec![Span::styled(
            format!("+{} more", total - TOOLS_MAX_VISIBLE),
            dim_style(),
        )]);
    }

    segments
}

fn render_tasks_segment(app: &App) -> Option<Vec<Span<'static>>> {
    let todos = &app.session_mgr.current().todo_items;
    if todos.is_empty() {
        return None;
    }
    let total = todos.len();
    let completed = todos
        .iter()
        .filter(|todo| matches!(todo.status, cc_middlewares::prelude::TodoStatus::Completed))
        .count();
    let filled = (((completed as f64 / total as f64) * 5.0).round() as usize).min(5);
    let bar = "#".repeat(filled) + &"-".repeat(5 - filled);
    Some(vec![Span::styled(
        format!("📋 [{}] {}/{}", bar, completed, total),
        plain_style(),
    )])
}

/// 第三行：权限/瞬时状态 + CPU/MEM + 快捷键提示
fn render_third_row(f: &mut Frame, app: &App, area: Rect) {
    let colors = SbColors::current();
    let lc = &app.services.lc;
    if let Some(text) = drawer_hint(app) {
        // 提示行属于「辅助说明」，与消息区 `Thought for … (ctrl+o to expand)` 同色（MUTED），
        // 不用抽屉游标的青色——青色只标记键盘游标本身。
        render_truncated_line(
            f,
            vec![Span::styled(text, Style::default().fg(colors.muted))],
            Vec::new(),
            area,
        );
        return;
    }
    let mut left_spans: Vec<Span> = Vec::new();
    let has_content = true;

    // 权限模式
    {
        use cc_middlewares::prelude::PermissionMode;
        let mode = app.services.permission_mode.load();
        let (i18n_key, color) = match mode {
            PermissionMode::AutoMode => ("statusbar-permission-auto", colors.warning),
            PermissionMode::Bypass => ("statusbar-permission-bypass", colors.error),
        };
        let label = lc.tr(i18n_key);
        let is_highlight = app
            .global_ui
            .mode_highlight_until
            .is_some_and(|until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(color);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        left_spans.push(Span::styled(format!("  {} ", label), style));
        if !matches!(
            &app.session_mgr.current().agent.interaction_prompt,
            Some(crate::app::InteractionPrompt::Approval(_))
        ) {
            left_spans.push(Span::styled(
                lc.tr("statusbar-permission-cycle-hint"),
                Style::default().fg(colors.muted),
            ));
        }
    }

    // 瞬时状态（复制提示已移至消息区右下角浮动显示）

    // 后台任务指示器（shell + agent 计数 pill，对齐效果图场景 2/3）
    {
        let bg_shell_count = app.running_background_shell_task_count();
        let bg_agent_count = app.session_mgr.current().background_agents.len();
        if bg_shell_count + bg_agent_count > 0 {
            if has_content {
                left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
            }
            let task_bar_focused =
                app.session_mgr.current().ui.background_tasks_bar_focused && bg_shell_count > 0;
            let mut parts: Vec<String> = Vec::new();
            if bg_shell_count > 0 {
                parts.push(format!(
                    "{} shell{}",
                    bg_shell_count,
                    if bg_shell_count > 1 { "s" } else { "" }
                ));
            }
            if bg_agent_count > 0 {
                parts.push(format!(
                    "{} agent{}",
                    bg_agent_count,
                    if bg_agent_count > 1 { "s" } else { "" }
                ));
            }
            let pill_style = if task_bar_focused {
                Style::default()
                    .bg(colors.selection_bg)
                    .fg(colors.text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(colors.cursor_bg).fg(colors.warning)
            };
            left_spans.push(Span::styled(format!(" {} ", parts.join(", ")), pill_style));
            left_spans.push(Span::styled("↓ to view", Style::default().fg(colors.muted)));
        }
    }

    // 重试状态
    if let Some(ref retry) = app.session_mgr.current().viewport_agent().retry_status {
        if has_content {
            left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
        }
        let delay_sec = retry.delay_ms as f64 / 1000.0;
        let err_preview: String = retry.error.chars().take(60).collect();
        let err_display = if retry.error.chars().count() > 60 {
            format!("{}...", err_preview)
        } else {
            err_preview
        };
        left_spans.push(Span::styled(
            format!(
                " {}",
                lc.tr_args(
                    "statusbar-retrying",
                    &[
                        ("attempt".into(), (retry.attempt as i64).into()),
                        ("max".into(), (retry.max_attempts as i64).into()),
                        ("delay".into(), format!("{:.1}", delay_sec).into()),
                        ("error".into(), err_display.into()),
                    ]
                )
            ),
            Style::default().fg(colors.warning),
        ));
    }

    // 系统级提示（判定服务降级等运行态事实）
    if let Some(ref notice) = app.session_mgr.current().viewport_agent().system_notice {
        if has_content {
            left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
        }
        let color = match notice.level {
            cc_agent::agent::events::NoticeLevel::Info => colors.muted,
            cc_agent::agent::events::NoticeLevel::Warn => colors.warning,
            cc_agent::agent::events::NoticeLevel::Error => colors.error,
        };
        let msg_preview: String = notice.message.chars().take(120).collect();
        let msg_display = if notice.message.chars().count() > 120 {
            format!("{}...", msg_preview)
        } else {
            msg_preview
        };
        left_spans.push(Span::styled(
            format!(
                " {}",
                lc.tr_args(
                    "statusbar-notice",
                    &[("message".into(), msg_display.into())]
                )
            ),
            Style::default().fg(color),
        ));
    }

    // MCP 初始化进度
    if let Some(ref rx) = app.services.mcp_init_rx {
        let status = rx.borrow().clone();
        use cc_middlewares::mcp::McpInitStatus;
        if !matches!(&status, McpInitStatus::Failed(_)) {
            app.global_ui.mcp_failed_shown.borrow_mut().take();
        }
        match status {
            McpInitStatus::Initializing { connected, total } => {
                if has_content {
                    left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
                }
                left_spans.push(Span::styled(
                    lc.tr_args(
                        "statusbar-mcp-connecting",
                        &[
                            ("connected".into(), (connected as i64).into()),
                            ("total".into(), (total as i64).into()),
                        ],
                    ),
                    Style::default().fg(colors.muted),
                ));
            }
            McpInitStatus::Ready { total } if total > 0 => {
                if app.global_ui.mcp_ready_shown_until.get().is_none() {
                    app.global_ui.mcp_ready_shown_until.set(Some(
                        std::time::Instant::now() + std::time::Duration::from_secs(3),
                    ));
                }
                if let Some(until) = app.global_ui.mcp_ready_shown_until.get() {
                    if std::time::Instant::now() < until {
                        if has_content {
                            left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
                        }
                        left_spans.push(Span::styled(
                            lc.tr_args(
                                "statusbar-mcp-ready",
                                &[("total".into(), (total as i64).into())],
                            ),
                            Style::default().fg(colors.success),
                        ));
                    }
                }
            }
            McpInitStatus::Failed(ref msg) => {
                // 截断过长的错误信息，移除内部技术细节
                let simplified = simplify_mcp_error(msg);
                let should_show = {
                    let now = std::time::Instant::now();
                    let mut shown = app.global_ui.mcp_failed_shown.borrow_mut();
                    match shown.as_ref() {
                        Some((shown_msg, until)) if shown_msg == &simplified => now < *until,
                        _ => {
                            *shown =
                                Some((simplified.clone(), now + std::time::Duration::from_secs(3)));
                            true
                        }
                    }
                };
                if should_show {
                    if has_content {
                        left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
                    }
                    left_spans.push(Span::styled(
                        lc.tr_args("statusbar-mcp-failed", &[("msg".into(), simplified.into())]),
                        Style::default().fg(colors.error),
                    ));
                }
            }
            McpInitStatus::Pending | McpInitStatus::Ready { .. } => {}
        }
    }

    // LSP 诊断计数
    {
        let agent = app.session_mgr.current().viewport_agent();
        if agent.lsp_errors > 0 || agent.lsp_warnings > 0 {
            if has_content {
                left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
            }
            left_spans.push(Span::styled(
                lc.tr_args(
                    "statusbar-lsp-diag",
                    &[
                        ("errors".into(), (agent.lsp_errors as i64).into()),
                        ("warnings".into(), (agent.lsp_warnings as i64).into()),
                    ],
                ),
                Style::default().fg(colors.muted),
            ));
        }
    }

    if app.agent_center_navigation_available() {
        left_spans.push(Span::styled(
            format!(" · ← {} agents", app.current_project_ready_count()),
            Style::default().fg(colors.muted),
        ));
    }

    // Cache/MEM（左侧块，右侧留给快捷键）
    {
        let tracker = &app
            .session_mgr
            .current()
            .viewport_agent()
            .session_token_tracker;
        let (cache_str, cache_color) = if tracker.last_usage.is_some() {
            let rate_pct = tracker.cache_hit_rate() * 100.0;
            let color = if rate_pct >= 80.0 {
                colors.success
            } else if rate_pct > 0.0 {
                colors.warning
            } else {
                colors.muted
            };
            (format!("Cache {:.0}%", rate_pct), color)
        } else {
            ("Cache -".to_string(), colors.muted)
        };

        let mut monitor = app.services.resource_monitor.lock();
        monitor.refresh_if_needed();
        let mem = monitor.memory_mb();
        drop(monitor);

        let mem_color = if mem > 1024 {
            colors.error
        } else if mem > 512 {
            colors.warning
        } else {
            colors.success
        };

        left_spans.push(Span::styled("  ", Style::default()));
        left_spans.push(Span::styled(cache_str, Style::default().fg(cache_color)));
        left_spans.push(Span::styled(" · ", Style::default().fg(colors.muted)));
        left_spans.push(Span::styled(
            format!("MEM {}MB", mem),
            Style::default().fg(mem_color),
        ));
    }

    // 右侧：快捷键提示
    let key_style = Style::default()
        .fg(colors.muted)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(colors.muted);

    let right_spans: Vec<Span> = match &app.session_mgr.current().agent.interaction_prompt {
        Some(_) if app.global_ui.oauth_prompt.is_some() => {
            let lc = &app.services.lc;
            format_hints(
                &[
                    ("Ctrl+O".to_string(), lc.tr("key-open-browser")),
                    ("Enter".to_string(), lc.tr("key-submit")),
                    ("Esc".to_string(), lc.tr("key-cancel")),
                ],
                key_style,
                desc_style,
            )
        }
        Some(crate::app::InteractionPrompt::Questions(_)) => {
            let lc = &app.services.lc;
            format_hints(
                &[
                    ("Tab".to_string(), lc.tr("key-switch")),
                    ("↑↓".to_string(), lc.tr("key-move")),
                    ("Space".to_string(), lc.tr("key-select")),
                    ("Enter".to_string(), lc.tr("key-confirm")),
                ],
                key_style,
                desc_style,
            )
        }
        Some(crate::app::InteractionPrompt::Approval(_)) => {
            // 审批面板固定底部已经展示完整快捷键，状态栏不重复占用宽度。
            Vec::new()
        }
        Some(crate::app::InteractionPrompt::Rewind(prompt)) => {
            use crate::app::RewindMode;
            match prompt.mode {
                RewindMode::ConfirmRevert => format_hints(
                    &[
                        ("Enter".to_string(), lc.tr("key-confirm")),
                        ("Esc".to_string(), lc.tr("key-cancel")),
                    ],
                    key_style,
                    desc_style,
                ),
                _ => format_hints(
                    &[
                        ("↑↓".to_string(), lc.tr("key-move")),
                        ("Tab".to_string(), lc.tr("key-rewind-file")),
                        ("Enter".to_string(), lc.tr("key-confirm")),
                        ("Esc".to_string(), lc.tr("key-cancel")),
                    ],
                    key_style,
                    desc_style,
                ),
            }
        }
        None => {
            let lc = &app.services.lc;
            // quit-pending 提示优先级最高：即使面板/详情模式打开，第一次 Ctrl+C 后
            // 也必须显示「再按 Ctrl+C 退出」，否则用户在面板场景下看不到退出反馈。
            // （详见 spec/archive-issues/2026-06-24-panel-swallow-ctrl-c.md）
            let hints = if app.global_ui.quit_pending_since.is_some() {
                vec![
                    ("Ctrl+C".to_string(), lc.tr("key-close")),
                    (lc.tr("key-any-other"), lc.tr("key-cancel")),
                ]
            } else if app.session_mgr.current().session_panels.is_any_open() {
                app.session_mgr
                    .current()
                    .session_panels
                    .status_bar_hints(lc)
            } else if app.global_panels.is_any_open() {
                app.global_panels.status_bar_hints(lc)
            } else if app.session_mgr.current().ui.detail_mode {
                vec![
                    ("● Verbose".to_string(), String::new()),
                    ("Ctrl+O".to_string(), lc.tr("key-exit-detail")),
                ]
            } else {
                vec![
                    ("Ctrl+O".to_string(), lc.tr("key-detail")),
                    ("Ctrl+P".to_string(), lc.tr("key-settings")),
                ]
            };
            format_hints(&hints, key_style, desc_style)
        }
    };

    render_truncated_line(f, left_spans, right_spans, area);
}

/// 上下文进度条渲染
fn render_context_bar(pct: f64, color: Color) -> Vec<Span<'static>> {
    const BAR_WIDTH: usize = 10;
    let filled = ((pct / 100.0) * BAR_WIDTH as f64).round() as usize;
    let filled = filled.min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;

    let bar: String = "█".repeat(filled) + &"░".repeat(empty);

    vec![
        Span::styled(bar, fg_style(color)),
        Span::styled(format!(" {}%", pct.round() as u64), fg_style(color)),
    ]
}

fn append_git_status(
    spans: &mut Vec<Span<'_>>,
    status: &crate::app::GitBranchStatus,
    colors: &SbColors,
) {
    let prefix_style = fg_style(colors.magenta);
    let branch_style = fg_style(colors.cyan);
    spans.push(Span::styled(" git:(", prefix_style));
    spans.push(Span::styled(status.branch.clone(), branch_style));
    if status.dirty {
        spans.push(Span::styled("*", prefix_style));
    }
    spans.push(Span::styled(")", prefix_style));
}

fn context_usage_color(pct: f64, colors: &SbColors) -> Color {
    if pct >= CONTEXT_CRITICAL_PCT {
        colors.context_high
    } else if pct >= CONTEXT_WARNING_PCT {
        colors.context_warn
    } else {
        colors.context_ok
    }
}

pub(crate) fn format_token_count(n: u64) -> String {
    if n >= 1_000_000 {
        let major = n / 1_000_000;
        let remainder = n % 1_000_000;
        if remainder == 0 {
            format!("{}M", major)
        } else {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        }
    } else if n >= 1_000 {
        let major = n / 1_000;
        let remainder = n % 1_000;
        if remainder == 0 {
            format!("{}k", major)
        } else {
            format!("{:.1}k", n as f64 / 1_000.0)
        }
    } else {
        n.to_string()
    }
}

fn format_duration_display(duration: std::time::Duration) -> String {
    let total_sec = duration.as_secs();
    if total_sec < 60 {
        format!("{}s", total_sec)
    } else if total_sec < 3_600 {
        let minutes = total_sec / 60;
        let seconds = total_sec % 60;
        format!("{}m{:02}s", minutes, seconds)
    } else {
        let hours = total_sec / 3_600;
        let minutes = (total_sec % 3_600) / 60;
        format!("{}h {:02}m", hours, minutes)
    }
}

fn truncate_tool_target(target: &str, max_len: usize) -> String {
    let normalized = target.replace('\\', "/");
    if normalized.chars().count() <= max_len {
        return normalized;
    }

    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
    if file_name.chars().count() >= max_len {
        return format!(
            "{}...",
            file_name
                .chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>()
        );
    }

    format!(".../{}", file_name)
}

fn fg_style(color: Color) -> Style {
    plain_style().fg(color)
}

fn plain_style() -> Style {
    Style::default().fg(Color::Reset).bg(Color::Reset)
}

fn dim_style() -> Style {
    plain_style().add_modifier(Modifier::DIM)
}

fn status_separator() -> Span<'static> {
    Span::styled(" | ", plain_style())
}

/// 将 (key, desc) 对列表格式化为 Span 列表
fn format_hints(
    hints: &[(String, String)],
    key_style: Style,
    desc_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = Vec::new();
    for (key, desc) in hints {
        spans.push(Span::styled(format!(" {} ", key), key_style));
        if !desc.is_empty() {
            spans.push(Span::styled(format!(":{} ", desc), desc_style));
        }
    }
    spans
}

/// 渲染一行 spans，左侧左对齐，右侧右对齐，中间填充空格
fn render_truncated_line(f: &mut Frame, left_spans: Vec<Span>, right_spans: Vec<Span>, area: Rect) {
    f.render_widget(Clear, area);

    let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();

    let total_content_width = left_width + right_width;
    let padding = if total_content_width < area.width as usize {
        " ".repeat(area.width as usize - total_content_width)
    } else {
        " ".to_string()
    };

    let mut all_spans = left_spans;
    all_spans.push(Span::styled(padding, plain_style()));
    all_spans.extend(right_spans);

    f.render_widget(Paragraph::new(Line::from(all_spans)), area);
}

/// 简化 MCP 错误信息，移除内部技术细节
///
/// 输入示例: "sentry: Send message error Transport [rmcp::transport::worker::WorkerTransport<rmcp::transport::streamable_http_client::StreamableHttpClientWorker<reqwest::...>>]"
/// 输出示例: "sentry: Send message error"
fn simplify_mcp_error(msg: &str) -> String {
    // 截断过长的错误信息
    let max_len = 80;
    let truncated: String = msg.chars().take(max_len).collect();

    // 移除方括号内的技术细节（如 [rmcp::transport::worker::...]）
    if let Some(bracket_start) = truncated.find('[') {
        let prefix = &truncated[..bracket_start];
        // 移除尾部的空格和标点
        let simplified = prefix.trim_end().trim_end_matches([':', '-']);
        if !simplified.is_empty() {
            return simplified.to_string();
        }
    }

    // 如果没有方括号，直接返回截断后的信息
    if truncated.len() < msg.len() {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn test_context_usage_color_matches_design_thresholds() {
        let colors = SbColors::for_theme(ThemeName::Default);
        assert_eq!(context_usage_color(69.9, &colors), Color::Green);
        assert_eq!(context_usage_color(70.0, &colors), Color::Yellow);
        assert_eq!(context_usage_color(84.9, &colors), Color::Yellow);
        assert_eq!(context_usage_color(85.0, &colors), Color::Red);
    }

    #[test]
    fn test_context_usage_color_follows_palette_on_light_theme() {
        let colors = SbColors::for_theme(ThemeName::Light);
        // 浅色主题下仪表必须取调色板的深色系，否则白底绿/黄几乎不可辨
        assert_eq!(context_usage_color(10.0, &colors), Color::Rgb(44, 122, 57));
        assert_eq!(context_usage_color(70.0, &colors), Color::Rgb(150, 108, 30));
        assert_eq!(context_usage_color(85.0, &colors), Color::Rgb(171, 43, 63));
    }

    #[test]
    fn test_sb_colors_default_theme_keeps_polished_literals() {
        let colors = SbColors::for_theme(ThemeName::Default);
        // default 主题观感锁定：ANSI 名色 + 写死 RGB，与 #509 之前逐值一致
        assert_eq!(colors.cyan, Color::Cyan);
        assert_eq!(colors.yellow, Color::Yellow);
        assert_eq!(colors.green, Color::Green);
        assert_eq!(colors.magenta, Color::Magenta);
        assert_eq!(colors.muted, Color::Rgb(153, 153, 153));
        assert_eq!(colors.warning, Color::Rgb(255, 193, 7));
        assert_eq!(colors.error, Color::Rgb(255, 107, 128));
        assert_eq!(colors.success, Color::Rgb(78, 186, 101));
        assert_eq!(colors.text, Color::Rgb(255, 255, 255));
        assert_eq!(colors.context_ok, Color::Green);
        assert_eq!(colors.context_warn, Color::Yellow);
        assert_eq!(colors.context_high, Color::Red);
        assert_eq!(colors.selection_bg, Color::Rgb(38, 79, 120));
        assert_eq!(colors.cursor_bg, Color::Rgb(38, 38, 38));
    }

    #[test]
    fn test_sb_colors_light_theme_uses_readable_palette_values() {
        let colors = SbColors::for_theme(ThemeName::Light);
        // 浅色主题：主文字转黑、次要信息加深，避免白字贴浅底
        assert_eq!(colors.text, Color::Rgb(0, 0, 0));
        assert_eq!(colors.muted, Color::Rgb(102, 102, 102));
        assert_eq!(colors.success, Color::Rgb(44, 122, 57));
        assert_eq!(colors.warning, Color::Rgb(150, 108, 30));
        assert_eq!(colors.error, Color::Rgb(171, 43, 63));
        assert_eq!(colors.cyan, Color::Rgb(0, 153, 153));
        assert_eq!(colors.yellow, Color::Rgb(150, 108, 30));
        assert_eq!(colors.magenta, Color::Rgb(135, 0, 255));
        assert_eq!(colors.selection_bg, Color::Rgb(180, 213, 255));
        assert_eq!(colors.cursor_bg, Color::Rgb(252, 252, 252));
    }

    #[test]
    fn test_sb_colors_non_default_themes_read_palette_fields() {
        for theme in ThemeName::ALL {
            if matches!(theme, ThemeName::Default) {
                continue;
            }
            let colors = SbColors::for_theme(theme);
            let p = theme.palette();
            assert_eq!(colors.muted, p.inactive, "{:?} muted", theme);
            assert_eq!(colors.text, p.text, "{:?} text", theme);
            assert_eq!(colors.warning, p.warning, "{:?} warning", theme);
            assert_eq!(colors.error, p.error, "{:?} error", theme);
            assert_eq!(colors.success, p.success, "{:?} success", theme);
            assert_eq!(colors.cyan, p.cyan, "{:?} cyan", theme);
            assert_eq!(colors.magenta, p.magenta, "{:?} magenta", theme);
            assert_eq!(
                colors.selection_bg, p.selection_bg,
                "{:?} selection_bg",
                theme
            );
        }
    }

    #[test]
    fn test_render_context_bar_uses_fixed_width_and_percent_only() {
        let spans = render_context_bar(45.4, Color::Green);
        assert_eq!(spans[0].content.as_ref(), "█████░░░░░");
        assert_eq!(spans[1].content.as_ref(), " 45%");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[1].style.fg, Some(Color::Green));
    }

    #[test]
    fn test_format_token_count_matches_codebuddy_hud() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(656), "656");
        assert_eq!(format_token_count(1_000), "1k");
        assert_eq!(format_token_count(60_324), "60.3k");
        assert_eq!(format_token_count(1_000_000), "1M");
        assert_eq!(format_token_count(2_010_866), "2.0M");
    }

    #[test]
    fn test_format_duration_display_matches_codebuddy_hud() {
        assert_eq!(
            format_duration_display(std::time::Duration::from_millis(532)),
            "0s"
        );
        assert_eq!(
            format_duration_display(std::time::Duration::from_secs(14)),
            "14s"
        );
        assert_eq!(
            format_duration_display(std::time::Duration::from_secs(14 * 60 + 7)),
            "14m07s"
        );
        assert_eq!(
            format_duration_display(std::time::Duration::from_secs(14 * 3600 + 43 * 60)),
            "14h 43m"
        );
        assert_eq!(
            format_duration_display(std::time::Duration::from_secs(14 * 3600 + 5 * 60)),
            "14h 05m"
        );
    }

    #[test]
    fn test_truncate_tool_target_matches_codebuddy_hud() {
        assert_eq!(truncate_tool_target("src/main.rs", 20), "src/main.rs");
        assert_eq!(
            truncate_tool_target("src/deep/nested/component.tsx", 20),
            ".../component.tsx"
        );
        assert_eq!(
            truncate_tool_target("very-long-file-name-for-test.rs", 20),
            "very-long-file-na..."
        );
        assert_eq!(
            truncate_tool_target(r"src\windows\path.rs", 20),
            "src/windows/path.rs"
        );
    }

    #[test]
    fn test_render_recent_tool_segment_matches_codebuddy_hud() {
        let segment = render_recent_tool_segment(
            &make_recent_entry("Read", "src/deep/file.rs", true),
            &make_default_colors(),
        );
        assert_eq!(spans_to_plain(&segment), "◐ Read : src/deep/file.rs");
        assert_eq!(segment[0].style.fg, Some(Color::Yellow));
        assert_eq!(segment[2].style.fg, Some(Color::Cyan));
        assert!(segment[3].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn test_render_recent_tool_segment_uses_completed_glyph_after_finish() {
        let segment = render_recent_tool_segment(
            &make_recent_entry("Read", "src/deep/file.rs", false),
            &make_default_colors(),
        );
        assert_eq!(spans_to_plain(&segment), "✓ Read : src/deep/file.rs");
        assert_eq!(segment[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn test_render_recent_tool_segment_omits_args_when_empty() {
        let segment = render_recent_tool_segment(
            &make_recent_entry("Todo", "", true),
            &make_default_colors(),
        );
        assert_eq!(spans_to_plain(&segment), "◐ Todo");
    }

    #[test]
    fn test_render_recent_tool_segment_colors_are_uniform_across_tools() {
        let colors = make_default_colors();
        let bash =
            render_recent_tool_segment(&make_recent_entry("Bash", "sleep 15", true), &colors);
        let read =
            render_recent_tool_segment(&make_recent_entry("Read", "src/main.rs", true), &colors);
        assert_eq!(bash[0].style.fg, read[0].style.fg);
        assert_eq!(bash[2].style.fg, read[2].style.fg);
    }

    #[test]
    fn test_truncate_tool_target_uses_char_count_cap() {
        assert_eq!(
            truncate_tool_target("src/ui/main_ui/status_bar.rs", TOOL_TARGET_MAX_LEN),
            "src/ui/main_ui/status_bar.rs"
        );
        assert_eq!(
            truncate_tool_target("tool_dispatch_test.rs:1580-1599", TOOL_TARGET_MAX_LEN),
            "tool_dispatch_test.rs:1580-..."
        );
    }

    #[test]
    fn test_truncate_spans_to_width_adds_ellipsis_when_narrow() {
        let spans = vec![
            Span::styled("◐ Bash : sleep 15", plain_style()),
            Span::styled(" | ", plain_style()),
            Span::styled("✓ Bash ×21", plain_style()),
        ];
        let plain = spans_to_plain(&truncate_spans_to_width(spans, 20));
        assert_eq!(plain, "◐ Bash : sleep 15 |…");
        assert_eq!(UnicodeWidthStr::width(plain.as_str()), 20);
    }

    #[test]
    fn test_truncate_spans_to_width_keeps_line_when_fits() {
        let spans = vec![Span::styled("◐ Bash : sleep 15", plain_style())];
        let plain = spans_to_plain(&truncate_spans_to_width(spans, 40));
        assert_eq!(plain, "◐ Bash : sleep 15");
    }

    #[test]
    fn test_truncate_spans_to_width_counts_cjk_as_two_columns() {
        let spans = vec![Span::styled("◐ Ask : 选择要修改的渲染层", plain_style())];
        let plain = spans_to_plain(&truncate_spans_to_width(spans, 18));
        assert!(UnicodeWidthStr::width(plain.as_str()) <= 18);
        assert!(plain.ends_with('…'));
    }

    fn make_recent_entry(
        display: &str,
        args_summary: &str,
        running: bool,
    ) -> crate::app::RecentToolEntry {
        crate::app::RecentToolEntry {
            tool_call_id: "tc1".to_string(),
            display: display.to_string(),
            args_summary: args_summary.to_string(),
            running,
            started_at: std::time::Instant::now(),
            visible_until: None,
        }
    }

    #[test]
    fn test_render_completed_tool_segments_limits_top_four() {
        let mut agent = crate::app::AgentComm::default();
        agent.session_tool_stats.insert("Read".to_string(), 10);
        agent.session_tool_stats.insert("Bash".to_string(), 8);
        agent.session_tool_stats.insert("Edit".to_string(), 6);
        agent.session_tool_stats.insert("Write".to_string(), 4);
        agent.session_tool_stats.insert("Grep".to_string(), 2);

        let segments = render_completed_tool_segments(&agent, &make_default_colors());
        assert_eq!(segments.len(), 5);
        assert_eq!(spans_to_plain(&segments[0]), "✓ Read ×10");
        assert_eq!(spans_to_plain(segments.last().unwrap()), "+1 more");
    }

    /// 状态栏默认（`default` 主题）配色——多数渲染断言只关心 default 主题的既有观感
    fn make_default_colors() -> SbColors {
        SbColors::for_theme(ThemeName::Default)
    }

    fn spans_to_plain(spans: &[Span<'_>]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
