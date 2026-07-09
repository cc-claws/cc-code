use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::{
    app::{tool_display, App},
    ui::theme,
};

const CONTEXT_WARNING_PCT: f64 = 70.0;
const CONTEXT_CRITICAL_PCT: f64 = 85.0;
const TOOLS_MAX_VISIBLE: usize = 4;
const RUNNING_TOOLS_MAX_VISIBLE: usize = 2;
const TOOL_TARGET_MAX_LEN: usize = 20;

pub(crate) fn status_bar_height(app: &App) -> u16 {
    if has_hud_activity(app) { 3 } else { 2 }
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

fn has_hud_activity(app: &App) -> bool {
    let session = app.session_mgr.current();
    let agent = &session.agent;
    !agent.running_tools.is_empty()
        || !agent.session_tool_stats.is_empty()
        || !session.background_agents.is_empty()
        || !session.todo_items.is_empty()
}

/// 第一行（codebuddy-hud compact）：[model] context | project git:(branch*) | stats
fn render_first_row(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    spans.push(Span::styled(" ", plain_style()));

    // Model + context bar 是一个 segment，中间用空格，不插入 ` | `。
    let is_highlight = app
        .global_ui
        .model_highlight_until
        .is_some_and(|until| std::time::Instant::now() < until);
    let mut model_style = ansi_style(Color::Cyan);
    if is_highlight {
        model_style = model_style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
    }
    spans.push(Span::styled(
        format!("[{}]", app.services.model_name),
        model_style,
    ));

    {
        let agent = &app.session_mgr.current().agent;
        let tracker = &agent.session_token_tracker;
        let pct = tracker
            .context_usage_percent(agent.context_window)
            .unwrap_or(0.0);
        let color = context_usage_color(pct);
        spans.push(Span::styled(" ", plain_style()));
        spans.extend(render_context_bar(pct, color));
    }

    spans.push(status_separator());
    append_project_segment(&mut spans, app);

    let stats = render_stats_segments(app);
    if !stats.is_empty() {
        spans.push(status_separator());
        spans.extend(stats);
    }

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行（codebuddy-hud activity）：running tools | completed tools | agents | tasks
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let mut left_spans: Vec<Span> = Vec::new();

    let agent = &app.session_mgr.current().agent;
    let running_start = agent
        .running_tools
        .len()
        .saturating_sub(RUNNING_TOOLS_MAX_VISIBLE);
    for active in &agent.running_tools[running_start..] {
        append_activity_segment(&mut left_spans, render_running_tool_segment(active));
    }

    for segment in render_completed_tool_segments(agent) {
        append_activity_segment(&mut left_spans, segment);
    }

    if let Some(segment) = render_agents_segment(app) {
        append_activity_segment(&mut left_spans, segment);
    }

    if let Some(segment) = render_tasks_segment(app) {
        append_activity_segment(&mut left_spans, segment);
    }

    render_truncated_line(f, left_spans, Vec::new(), area);
}

fn append_project_segment(spans: &mut Vec<Span<'_>>, app: &App) {
    let cwd_short = std::path::Path::new(&app.services.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.services.cwd);
    spans.push(Span::styled(
        cwd_short.to_string(),
        ansi_style(Color::Yellow),
    ));

    // loading 期间跳过子进程刷新，避免 `git rev-parse` 阻塞渲染线程导致抖动。
    let loading = app.session_mgr.current().ui.loading;
    let mut cache = app.services.git_branch_cache.lock();
    if loading {
        if let Some(status) = cache.get_cached() {
            append_git_status(spans, status);
        }
    } else if let Some(status) = cache.get_or_refresh(&app.services.cwd) {
        append_git_status(spans, status);
    }
}

fn render_stats_segments(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let agent = &app.session_mgr.current().agent;
    let tracker = &agent.session_token_tracker;

    if tracker.total_input_tokens > 0 || tracker.total_output_tokens > 0 {
        spans.push(Span::styled(
            format!(
                "tok: {} (in: {}, out: {})",
                format_token_count(tracker.total_input_tokens),
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
            format!("⏱️  {}", format_duration_display(start.elapsed())),
            dim_style(),
        ));
    }

    spans
}

fn append_activity_segment(spans: &mut Vec<Span<'_>>, segment: Vec<Span<'static>>) {
    if spans.is_empty() {
        spans.push(Span::styled(" ", plain_style()));
    } else {
        spans.push(status_separator());
    }
    spans.extend(segment);
}

fn render_running_tool_segment(active: &crate::app::ActiveToolInfo) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled("◐", ansi_style(Color::Yellow)),
        Span::styled(" ", plain_style()),
        Span::styled(active.display.clone(), ansi_style(Color::Cyan)),
    ];
    if !active.args_summary.is_empty() {
        spans.push(Span::styled(
            format!(
                " : {}",
                truncate_tool_target(&active.args_summary, TOOL_TARGET_MAX_LEN)
            ),
            dim_style(),
        ));
    }
    spans
}

fn render_completed_tool_segments(agent: &crate::app::AgentComm) -> Vec<Vec<Span<'static>>> {
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
            Span::styled("✓", ansi_style(Color::Green)),
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

fn render_agents_segment(app: &App) -> Option<Vec<Span<'static>>> {
    let agents = &app.session_mgr.current().background_agents;
    if agents.is_empty() {
        return None;
    }
    let labels = agents
        .iter()
        .map(|agent| format!("{}(run)", agent.agent_name))
        .collect::<Vec<_>>()
        .join(" ");
    Some(vec![Span::styled(
        format!("🤖 {}", labels),
        plain_style(),
    )])
}

fn render_tasks_segment(app: &App) -> Option<Vec<Span<'static>>> {
    let todos = &app.session_mgr.current().todo_items;
    if todos.is_empty() {
        return None;
    }
    let total = todos.len();
    let completed = todos
        .iter()
        .filter(|todo| {
            matches!(
                todo.status,
                peri_middlewares::prelude::TodoStatus::Completed
            )
        })
        .count();
    let filled = (((completed as f64 / total as f64) * 5.0).round() as usize).min(5);
    let bar = "#".repeat(filled) + &"-".repeat(5 - filled);
    Some(vec![Span::styled(
        format!(
        "📋 [{}] {}/{}",
        bar, completed, total
        ),
        plain_style(),
    )])
}

/// 第三行：权限/瞬时状态 + CPU/MEM + 快捷键提示
fn render_third_row(f: &mut Frame, app: &App, area: Rect) {
    let lc = &app.services.lc;
    let mut left_spans: Vec<Span> = Vec::new();
    let has_content = true;

    // 权限模式
    {
        use peri_middlewares::prelude::PermissionMode;
        let mode = app.services.permission_mode.load();
        let (i18n_key, color) = match mode {
            PermissionMode::Default => ("statusbar-permission-default", theme::TEXT),
            PermissionMode::DontAsk => ("statusbar-permission-accept-edit", theme::THINKING),
            PermissionMode::AcceptEdit => ("statusbar-permission-accept-edit", theme::THINKING),
            PermissionMode::AutoMode => ("statusbar-permission-auto", theme::WARNING),
            PermissionMode::Bypass => ("statusbar-permission-bypass", theme::ERROR),
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
        left_spans.push(Span::styled(format!(" {} ", label), style));
        left_spans.push(Span::styled(
            lc.tr("statusbar-permission-cycle-hint"),
            Style::default().fg(theme::MUTED),
        ));
    }

    // 瞬时状态（复制提示已移至消息区右下角浮动显示）

    // 后台任务指示器（shell + agent 计数 pill，对齐效果图场景 2/3）
    {
        let bg_shell_count = app.running_background_shell_task_count();
        let bg_agent_count = app.session_mgr.current().background_agents.len();
        if bg_shell_count + bg_agent_count > 0 {
            if has_content {
                left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
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
                    .bg(theme::SELECTION_BG)
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(theme::CURSOR_BG).fg(theme::WARNING)
            };
            left_spans.push(Span::styled(format!(" {} ", parts.join(", ")), pill_style));
            left_spans.push(Span::styled("↓ to view", Style::default().fg(theme::MUTED)));
        }
    }

    // 重试状态
    if let Some(ref retry) = app.session_mgr.current().agent.retry_status {
        if has_content {
            left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
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
            Style::default().fg(theme::WARNING),
        ));
    }

    // MCP 初始化进度
    if let Some(ref rx) = app.services.mcp_init_rx {
        let status = rx.borrow().clone();
        use peri_middlewares::mcp::McpInitStatus;
        if !matches!(&status, McpInitStatus::Failed(_)) {
            app.global_ui.mcp_failed_shown.borrow_mut().take();
        }
        match status {
            McpInitStatus::Initializing { connected, total } => {
                if has_content {
                    left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
                }
                left_spans.push(Span::styled(
                    lc.tr_args(
                        "statusbar-mcp-connecting",
                        &[
                            ("connected".into(), (connected as i64).into()),
                            ("total".into(), (total as i64).into()),
                        ],
                    ),
                    Style::default().fg(theme::MUTED),
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
                            left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
                        }
                        left_spans.push(Span::styled(
                            lc.tr_args(
                                "statusbar-mcp-ready",
                                &[("total".into(), (total as i64).into())],
                            ),
                            Style::default().fg(theme::SAGE),
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
                        left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
                    }
                    left_spans.push(Span::styled(
                        lc.tr_args("statusbar-mcp-failed", &[("msg".into(), simplified.into())]),
                        Style::default().fg(theme::ERROR),
                    ));
                }
            }
            McpInitStatus::Pending | McpInitStatus::Ready { .. } => {}
        }
    }

    // LSP 诊断计数
    {
        let agent = &app.session_mgr.current().agent;
        if agent.lsp_errors > 0 || agent.lsp_warnings > 0 {
            if has_content {
                left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
            }
            left_spans.push(Span::styled(
                lc.tr_args(
                    "statusbar-lsp-diag",
                    &[
                        ("errors".into(), (agent.lsp_errors as i64).into()),
                        ("warnings".into(), (agent.lsp_warnings as i64).into()),
                    ],
                ),
                Style::default().fg(theme::MUTED),
            ));
        }
    }

    // Rewind 忙碌提示
    if let Some(until) = app.global_ui.rewind_busy_hint_until {
        if std::time::Instant::now() < until {
            left_spans.push(Span::styled(
                " Agent 运行中，请等待后再撤销 ",
                Style::default().fg(theme::WARNING),
            ));
        }
    }

    // CPU/MEM（右侧快捷键前面）
    {
        let mut monitor = app.services.resource_monitor.lock();
        monitor.refresh_if_needed();
        let mem = monitor.memory_mb();
        let cpu = monitor.cpu_percent();
        drop(monitor);

        let cpu_color = if cpu > 70.0 {
            theme::ERROR
        } else if cpu > 30.0 {
            theme::WARNING
        } else {
            theme::SAGE
        };
        let mem_color = if mem > 1024 {
            theme::ERROR
        } else if mem > 512 {
            theme::WARNING
        } else {
            theme::SAGE
        };

        left_spans.push(Span::styled("  ", Style::default()));
        left_spans.push(Span::styled(
            format!("CPU {:.0}%", cpu),
            Style::default().fg(cpu_color),
        ));
        left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
        left_spans.push(Span::styled(
            format!("MEM {}MB", mem),
            Style::default().fg(mem_color),
        ));
    }

    // 右侧：快捷键提示
    let key_style = Style::default()
        .fg(theme::MUTED)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme::MUTED);

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
            let lc = &app.services.lc;
            format_hints(
                &[
                    ("↑↓".to_string(), lc.tr("key-move")),
                    ("Space".to_string(), lc.tr("key-switch")),
                    ("Enter".to_string(), lc.tr("key-confirm")),
                ],
                key_style,
                desc_style,
            )
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
                        ("↑↓".to_string(), "移动".to_string()),
                        ("Tab".to_string(), "切换回退文件".to_string()),
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
                    ("其他键".to_string(), lc.tr("key-cancel")),
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
        Span::styled(bar, ansi_style(color)),
        Span::styled(format!(" {}%", pct.round() as u64), ansi_style(color)),
    ]
}

fn append_git_status(spans: &mut Vec<Span<'_>>, status: &crate::app::GitBranchStatus) {
    let prefix_style = ansi_style(Color::Magenta);
    let branch_style = ansi_style(Color::Cyan);
    spans.push(Span::styled(" git:(", prefix_style));
    spans.push(Span::styled(status.branch.clone(), branch_style));
    if status.dirty {
        spans.push(Span::styled("*", prefix_style));
    }
    spans.push(Span::styled(")", prefix_style));
}

fn context_usage_color(pct: f64) -> Color {
    if pct >= CONTEXT_CRITICAL_PCT {
        Color::Red
    } else if pct >= CONTEXT_WARNING_PCT {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn format_token_count(n: u64) -> String {
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
        format!("{}m{}s", minutes, seconds)
    } else {
        let hours = total_sec / 3_600;
        let minutes = (total_sec % 3_600) / 60;
        format!("{}h {}m", hours, minutes)
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

fn ansi_style(color: Color) -> Style {
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

    #[test]
    fn test_context_usage_color_matches_design_thresholds() {
        assert_eq!(context_usage_color(69.9), Color::Green);
        assert_eq!(context_usage_color(70.0), Color::Yellow);
        assert_eq!(context_usage_color(84.9), Color::Yellow);
        assert_eq!(context_usage_color(85.0), Color::Red);
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
            "14m7s"
        );
        assert_eq!(
            format_duration_display(std::time::Duration::from_secs(14 * 3600 + 43 * 60)),
            "14h 43m"
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
    fn test_render_running_tool_segment_matches_codebuddy_hud() {
        let segment = render_running_tool_segment(&crate::app::ActiveToolInfo {
            tool_call_id: "tc1".to_string(),
            name: "Read".to_string(),
            display: "Read".to_string(),
            args_summary: "src/deep/file.rs".to_string(),
        });
        assert_eq!(spans_to_plain(&segment), "◐ Read : src/deep/file.rs");
        assert_eq!(segment[0].style.fg, Some(Color::Yellow));
        assert_eq!(segment[2].style.fg, Some(Color::Cyan));
        assert!(segment[3].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn test_render_completed_tool_segments_limits_top_four() {
        let mut agent = crate::app::AgentComm::default();
        agent.session_tool_stats.insert("Read".to_string(), 10);
        agent.session_tool_stats.insert("Bash".to_string(), 8);
        agent.session_tool_stats.insert("Edit".to_string(), 6);
        agent.session_tool_stats.insert("Write".to_string(), 4);
        agent.session_tool_stats.insert("Grep".to_string(), 2);

        let segments = render_completed_tool_segments(&agent);
        assert_eq!(segments.len(), 5);
        assert_eq!(spans_to_plain(&segments[0]), "✓ Read ×10");
        assert_eq!(spans_to_plain(segments.last().unwrap()), "+1 more");
    }

    fn spans_to_plain(spans: &[Span<'_>]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
