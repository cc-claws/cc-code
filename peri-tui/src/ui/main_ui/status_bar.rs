use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::{app::App, ui::theme};

pub(crate) fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 第一行：模型 + 进度条 + git + 会话 + token
            Constraint::Length(1), // 第二行：执行中工具 + 工具历史
            Constraint::Length(1), // 第三行：权限/瞬时状态 + CPU/MEM + 快捷键
        ])
        .split(area);

    render_first_row(f, app, rows[0]);
    render_second_row(f, app, rows[1]);
    render_third_row(f, app, rows[2]);
}

/// 第一行：[model] progress | dir git:(branch) | session name | ⏱️ duration tok:detail
fn render_first_row(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    // 模型名（方括号）— 对齐 Claude Hub CYAN
    {
        let is_highlight = app
            .global_ui
            .model_highlight_until
            .is_some_and(|until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(theme::CYAN);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        spans.push(Span::styled(format!(" {}", app.services.model_name), style));
    }

    // 上下文进度条
    {
        let agent = &app.session_mgr.current().agent;
        let tracker = &agent.session_token_tracker;
        if let Some(pct) = tracker.context_usage_percent(agent.context_window) {
            let total = agent.context_window;
            let color = if pct >= 85.0 {
                theme::ERROR
            } else if pct >= 70.0 {
                theme::WARNING
            } else {
                theme::SAGE
            };
            spans.push(Span::styled(" ", Style::default()));
            spans.extend(render_context_bar(pct, total, color));
        }
    }

    // 分隔符 + 工作目录
    spans.push(Span::styled(" | ", Style::default().fg(theme::DIM)));
    let cwd_short = std::path::Path::new(&app.services.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&app.services.cwd);
    spans.push(Span::styled(
        cwd_short.to_string(),
        Style::default().fg(theme::WARNING),
    ));

    // Git 分支 — 对齐 Claude Hub: git:( MAGENTA + branch CYAN + ) MAGENTA
    {
        let mut cache = app.services.git_branch_cache.lock();
        if let Some(branch) = cache.get_or_refresh(&app.services.cwd) {
            spans.push(Span::styled(" git:(", Style::default().fg(theme::MAGENTA)));
            spans.push(Span::styled(
                branch.to_string(),
                Style::default().fg(theme::CYAN),
            ));
            spans.push(Span::styled(")", Style::default().fg(theme::MAGENTA)));
        }
    }

    // 分隔符 + ⏱️ 会话时长 + token 明细
    {
        let agent = &app.session_mgr.current().agent;
        let tracker = &agent.session_token_tracker;
        let has_duration = agent.session_start_time.is_some();
        let total_tokens = tracker.total_input_tokens + tracker.total_output_tokens;
        if has_duration || total_tokens > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(theme::DIM)));
        }

        // 会话时长
        if let Some(start) = agent.session_start_time {
            let s = start.elapsed().as_secs();
            let text = if s >= 3600 {
                format!("{}h{}m", s / 3600, (s % 3600) / 60)
            } else if s >= 60 {
                format!("{}m", s / 60)
            } else {
                format!("{}s", s)
            };
            spans.push(Span::styled(
                format!("⏱  {} ", text),
                Style::default().fg(theme::DIM),
            ));
        }

        // Token 明细
        if total_tokens > 0 {
            spans.push(Span::styled(
                "tok:".to_string(),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                format_tokens_compact(total_tokens),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                " (in:".to_string(),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                format_tokens_compact(tracker.total_input_tokens),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                ", out:".to_string(),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                format_tokens_compact(tracker.total_output_tokens),
                Style::default().fg(theme::DIM),
            ));
            let cache_rate = tracker.cache_hit_rate();
            spans.push(Span::styled(
                ", cached:".to_string(),
                Style::default().fg(theme::DIM),
            ));
            spans.push(Span::styled(
                format!("{:.0}%", cache_rate * 100.0),
                Style::default().fg(theme::SAGE),
            ));
            spans.push(Span::styled(
                ")".to_string(),
                Style::default().fg(theme::DIM),
            ));
        }
    }

    render_truncated_line(f, spans, Vec::new(), area);
}

/// 第二行：◐ active tool | ✓ tool history
fn render_second_row(f: &mut Frame, app: &App, area: Rect) {
    let mut left_spans: Vec<Span> = Vec::new();
    let mut has_content = false;

    // 执行中工具指示器 — 对齐 Claude Hub: ◐ YELLOW + name CYAN + args DIM
    {
        let agent = &app.session_mgr.current().agent;
        if let Some(ref active) = agent.active_tool {
            left_spans.push(Span::styled(" ◐ ", Style::default().fg(theme::YELLOW)));
            left_spans.push(Span::styled(
                active.display.clone(),
                Style::default().fg(theme::CYAN),
            ));
            if active.args_summary.is_empty() {
                left_spans.push(Span::styled("…", Style::default().fg(theme::DIM)));
            } else {
                left_spans.push(Span::styled(
                    format!(":{}", active.args_summary),
                    Style::default().fg(theme::DIM),
                ));
            }
            has_content = true;
        } else if !agent.session_tool_stats.is_empty() {
            // 工具执行完后保留最近工具名，用 DIM 色
            let last = agent
                .session_tool_stats
                .iter()
                .max_by_key(|(_, c)| *c)
                .map(|(n, _)| n.clone());
            if let Some(name) = last {
                let display = format_tool_display_name(&name);
                left_spans.push(Span::styled(
                    format!(" ◐ {}", display),
                    Style::default().fg(theme::DIM),
                ));
                has_content = true;
            }
        } else {
            // 默认占位：还没有执行过工具时显示
            left_spans.push(Span::styled(" ◐ Tool", Style::default().fg(theme::DIM)));
            has_content = true;
        }
    }

    // 工具历史计数（按次数降序，最多 5 个）
    {
        let agent = &app.session_mgr.current().agent;
        if !agent.session_tool_stats.is_empty() {
            if has_content {
                left_spans.push(Span::styled(" | ", Style::default().fg(theme::DIM)));
            }
            let mut entries: Vec<_> = agent.session_tool_stats.iter().collect();
            entries.sort_by(|a, b| b.1.cmp(a.1));
            for (i, (name, count)) in entries.iter().take(5).enumerate() {
                if i > 0 {
                    left_spans.push(Span::styled(" | ", Style::default().fg(theme::DIM)));
                }
                let display = format_tool_display_name(name);
                left_spans.push(Span::styled("✓", Style::default().fg(theme::SAGE)));
                left_spans.push(Span::styled(format!(" {}", display), Style::default()));
                left_spans.push(Span::styled(
                    format!(" ×{}", count),
                    Style::default().fg(theme::DIM),
                ));
            }
        }
    }

    render_truncated_line(f, left_spans, Vec::new(), area);
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
            PermissionMode::DontAsk => ("statusbar-permission-dont-ask", theme::WARNING),
            PermissionMode::AcceptEdit => ("statusbar-permission-accept-edit", theme::THINKING),
            PermissionMode::AutoMode => ("statusbar-permission-auto", theme::WARNING),
            PermissionMode::Bypass => ("statusbar-permission-bypass", theme::ERROR),
        };
        let label = lc.tr(i18n_key);
        let hint = lc.tr("statusbar-permission-cycle-hint");
        let is_highlight = app
            .global_ui
            .mode_highlight_until
            .is_some_and(|until| std::time::Instant::now() < until);
        let mut style = Style::default().fg(color);
        if is_highlight {
            style = style.add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        }
        left_spans.push(Span::styled(format!(" {} ", label), style));
        left_spans.push(Span::styled(hint, Style::default().fg(theme::DIM)));
    }

    // 瞬时状态
    // 复制成功提示
    if let Some(until) = app.session_mgr.current().ui.copy_message_until {
        if std::time::Instant::now() < until {
            if has_content {
                left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
            }
            let count = app.session_mgr.current().ui.copy_char_count;
            left_spans.push(Span::styled(
                format!(
                    " {}",
                    lc.tr_args(
                        "statusbar-copied",
                        &[("count".into(), (count as i64).into()),]
                    )
                ),
                Style::default().fg(theme::MUTED),
            ));
        }
    }

    // 后台任务指示器
    if !app.session_mgr.current().background_agents.is_empty() {
        if has_content {
            left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
        }
        left_spans.push(Span::styled(
            lc.tr_args(
                "statusbar-bg-indicator",
                &[(
                    "count".into(),
                    (app.session_mgr.current().background_agents.len() as i64).into(),
                )],
            ),
            Style::default().fg(theme::WARNING),
        ));
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
                if has_content {
                    left_spans.push(Span::styled(" · ", Style::default().fg(theme::MUTED)));
                }
                // 截断过长的错误信息，移除内部技术细节
                let simplified = simplify_mcp_error(msg);
                left_spans.push(Span::styled(
                    lc.tr_args("statusbar-mcp-failed", &[("msg".into(), simplified.into())]),
                    Style::default().fg(theme::ERROR),
                ));
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
                    ("Home/End".to_string(), lc.tr("key-jump")),
                    ("PgUp/PgDn".to_string(), lc.tr("key-scroll")),
                ]
            } else {
                vec![
                    ("/".to_string(), lc.tr("key-command")),
                    ("Shift+Enter".to_string(), lc.tr("key-newline")),
                    ("Ctrl+T".to_string(), lc.tr("key-switch-model")),
                    ("Ctrl+U/D".to_string(), lc.tr("key-scroll")),
                ]
            };
            format_hints(&hints, key_style, desc_style)
        }
    };

    render_truncated_line(f, left_spans, right_spans, area);
}

/// 上下文进度条渲染
fn render_context_bar(pct: f64, total: u32, color: ratatui::style::Color) -> Vec<Span<'static>> {
    const BAR_WIDTH: usize = 10;
    let filled = ((pct / 100.0) * BAR_WIDTH as f64).round() as usize;
    let filled = filled.min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;

    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    let total_display = if total >= 1_000_000 {
        format!("{:.0}M", total as f64 / 1_000_000.0)
    } else {
        format!("{:.0}k", total as f64 / 1000.0)
    };

    vec![
        Span::styled(bar, Style::default().fg(color)),
        Span::styled(
            format!(" {:.0}% {}", pct, total_display),
            Style::default().fg(color),
        ),
    ]
}

/// 工具显示名映射
fn format_tool_display_name(name: &str) -> &str {
    match name {
        "Read" => "Read",
        "Write" => "Write",
        "Edit" => "Edit",
        "Glob" => "Glob",
        "Grep" => "Grep",
        "Bash" => "Bash",
        "WebFetch" => "WebFetch",
        "WebSearch" => "WebSearch",
        "Agent" => "Agent",
        "AskUser" => "AskUser",
        "AskUserQuestion" => "Ask",
        "TodoWrite" => "Todo",
        "SearchExtraTools" => "Search",
        "ExecuteExtraTool" => "Exec",
        "LspTool" => "LSP",
        other => other,
    }
}

/// Token 数量简写
fn format_tokens_compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
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
        spans.push(Span::styled(format!(":{} ", desc), desc_style));
    }
    spans
}

/// 渲染一行 spans，左侧左对齐，右侧右对齐，中间填充空格
fn render_truncated_line(f: &mut Frame, left_spans: Vec<Span>, right_spans: Vec<Span>, area: Rect) {
    let left_width: usize = left_spans.iter().map(|s| s.width()).sum();
    let right_width: usize = right_spans.iter().map(|s| s.width()).sum();

    let total_content_width = left_width + right_width;
    let padding = if total_content_width < area.width as usize {
        " ".repeat(area.width as usize - total_content_width)
    } else {
        " ".to_string()
    };

    let mut all_spans = left_spans;
    all_spans.push(Span::raw(padding));
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
