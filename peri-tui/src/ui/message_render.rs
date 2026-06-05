use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::{
    message_view::{AgentSummary, ContentBlockView, MessageViewModel, ToolCategory},
    theme,
};

const SHELL_OUTPUT_COLLAPSED_LINES: usize = 6;
const SHELL_OUTPUT_DETAIL_LINES: usize = 40;

/// Generate always-visible error summary lines (up to 400 Unicode chars).
/// 2-space indent, no vertical bar, no prefix. Preserves newlines (multi-line render).
fn error_summary_lines(content: &str) -> Vec<Line<'static>> {
    let truncated: String = content.chars().take(400).collect();
    truncated
        .lines()
        .map(|line| {
            Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                Span::styled(line.to_string(), Style::default().fg(theme::ERROR)),
            ])
        })
        .collect()
}

/// 批次汇总树形渲染：折叠态显示 header + 每行摘要，展开态显示各 agent 详情。
fn render_batch_summary(agents: &[AgentSummary], collapsed: &bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let total = agents.len();
    let failed_count = agents.iter().filter(|a| a.is_error).count();

    // Header 行
    let header_text = if failed_count == total {
        // 全部失败
        format!("{} agents failed", total)
    } else if failed_count > 0 {
        // 部分失败
        format!("{} agents finished, {} failed", total, failed_count)
    } else {
        format!("{} agents finished", total)
    };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(theme::SAGE)),
        Span::styled(header_text, Style::default().fg(theme::TEXT)),
    ]));

    if *collapsed {
        // 折叠态：每行 agent 摘要
        for (idx, agent) in agents.iter().enumerate() {
            let is_last = idx == total - 1;
            let connector = if is_last { "└─" } else { "├─" };
            let status = if agent.is_error {
                ("Failed", theme::ERROR)
            } else {
                ("Done", theme::SAGE)
            };

            let mut spans = vec![
                Span::styled("   ", Style::default().fg(theme::DIM)),
                Span::styled(connector.to_string(), Style::default().fg(theme::DIM)),
                Span::styled(" ".to_string(), Style::default()),
                Span::styled(agent.task_preview.clone(), Style::default().fg(theme::TEXT)),
            ];

            if agent.tool_count > 0 {
                spans.push(Span::styled(
                    format!(" · {} tool uses", agent.tool_count),
                    Style::default().fg(theme::DIM),
                ));
            }

            spans.push(Span::styled(" · ", Style::default().fg(theme::DIM)));
            spans.push(Span::styled(
                status.0.to_string(),
                Style::default().fg(status.1),
            ));

            lines.push(Line::from(spans));
        }
    } else {
        // 展开态：每个 agent 显示 task_preview + final_result
        for (idx, agent) in agents.iter().enumerate() {
            let is_last = idx == total - 1;
            let connector = if is_last { "└─" } else { "├─" };

            // task_preview 行
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(connector.to_string(), Style::default().fg(theme::DIM)),
                Span::raw(" "),
                Span::styled(agent.task_preview.clone(), Style::default().fg(theme::TEXT)),
            ]));

            // final_result 行（如果有）
            if let Some(ref result) = agent.final_result {
                if !result.is_empty() {
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled("⎿ ", Style::default().fg(theme::DIM)),
                        Span::styled(result.clone(), Style::default().fg(theme::MUTED)),
                    ]));
                }
            }
        }
    }

    lines
}

/// AskUserQuestion 专用渲染：`● User answered Peri's questions:` + `⎿ · H → V`
fn render_ask_user_block(content: &str, is_error: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let color = if is_error { theme::ERROR } else { theme::SAGE };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(color)),
        Span::styled(
            "User answered Peri's questions:".to_string(),
            Style::default().fg(theme::TEXT),
        ),
    ]));

    if content.is_empty() {
        return lines;
    }

    // 解析多问题格式: [问: H]\n回答: V\n\n[问: H2]\n回答: V2
    for block in content.split("\n\n") {
        let mut header = String::new();
        let mut answer = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("[问: ") {
                header = rest.trim_end_matches(']').to_string();
            } else if let Some(a) = line.strip_prefix("回答: ") {
                answer = a.to_string();
            }
        }
        header = header.replace(['\n', '\r'], " ");
        answer = answer.replace(['\n', '\r'], " ");
        let text = if !header.is_empty() {
            format!("{} → {}", header, answer)
        } else if !answer.is_empty() {
            answer
        } else {
            block.lines().collect::<Vec<_>>().join(" ")
        };
        if text.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
            Span::styled(
                text,
                Style::default().fg(if is_error { theme::ERROR } else { theme::MUTED }),
            ),
        ]));
    }

    lines
}

fn shell_fg_color(code: u16) -> Option<Color> {
    match code {
        30 => Some(Color::Black),
        31 => Some(Color::Red),
        32 => Some(Color::Green),
        33 => Some(Color::Yellow),
        34 => Some(Color::Blue),
        35 => Some(Color::Magenta),
        36 => Some(Color::Cyan),
        37 => Some(Color::White),
        90 => Some(Color::DarkGray),
        91 => Some(Color::LightRed),
        92 => Some(Color::LightGreen),
        93 => Some(Color::LightYellow),
        94 => Some(Color::LightBlue),
        95 => Some(Color::LightMagenta),
        96 => Some(Color::LightCyan),
        97 => Some(Color::White),
        _ => None,
    }
}

fn apply_sgr_codes(style: &mut Style, default_style: Style, codes: &str) {
    let parsed: Vec<u16> = if codes.trim().is_empty() {
        vec![0]
    } else {
        codes
            .split(';')
            .filter_map(|part| part.parse::<u16>().ok())
            .collect()
    };
    let mut iter = parsed.into_iter().peekable();
    while let Some(code) = iter.next() {
        match code {
            0 => *style = default_style,
            1 => *style = style.add_modifier(Modifier::BOLD),
            22 => *style = style.remove_modifier(Modifier::BOLD),
            39 => *style = default_style,
            38 => match iter.next() {
                Some(2) => {
                    let (Some(r), Some(g), Some(b)) = (iter.next(), iter.next(), iter.next())
                    else {
                        continue;
                    };
                    *style = style.fg(Color::Rgb(r as u8, g as u8, b as u8));
                }
                Some(5) => {
                    let _ = iter.next();
                }
                _ => {}
            },
            code => {
                if let Some(color) = shell_fg_color(code) {
                    *style = style.fg(color);
                }
            }
        }
    }
}

fn ansi_spans(line: &str, default_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut style = default_style;
    let mut buf = String::new();
    let mut i = 0;
    while i < line.len() {
        if line[i..].starts_with("\x1b[") {
            if let Some(end) = line[i + 2..].find('m') {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), style));
                }
                let codes = &line[i + 2..i + 2 + end];
                apply_sgr_codes(&mut style, default_style, codes);
                i += end + 3;
                continue;
            }
        }
        let Some(ch) = line[i..].chars().next() else {
            break;
        };
        if ch != '\r' {
            buf.push(ch);
        }
        i += ch.len_utf8();
    }
    if !buf.is_empty() || spans.is_empty() {
        spans.push(Span::styled(buf, style));
    }
    spans
}

fn shell_output_line(prefix: &'static str, text: &str, default_style: Style) -> Line<'static> {
    let bg_style = Style::default().bg(theme::SHELL_BG);
    let mut spans = vec![Span::styled(
        prefix,
        Style::default().fg(theme::SHELL_BORDER).bg(theme::SHELL_BG),
    )];
    spans.extend(
        ansi_spans(text, default_style)
            .into_iter()
            .map(|span| span.patch_style(bg_style)),
    );
    Line::from(spans)
}

fn render_shell_command(
    command: &str,
    cwd: &str,
    stdin: &[String],
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    detail_mode: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let status_style = match exit_code {
        None => Style::default().fg(theme::LOADING),
        Some(0) => Style::default().fg(theme::SAGE),
        Some(_) => Style::default().fg(theme::ERROR),
    };
    let status = match exit_code {
        None => " running".to_string(),
        Some(code) => format!(" exit {}", code),
    };
    let cwd_label: String = cwd.chars().take(80).collect();
    lines.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme::DIM)),
        Span::styled(
            format!("!{}", command),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(status, status_style),
        Span::styled(format!(" · {}", cwd_label), Style::default().fg(theme::DIM)),
    ]));

    let mut output_lines: Vec<(String, bool)> = Vec::new();
    for input in stdin {
        output_lines.push((format!("< {}", input), false));
    }
    for line in stdout.lines() {
        output_lines.push((line.to_string(), false));
    }
    for line in stderr.lines() {
        output_lines.push((line.to_string(), true));
    }

    if output_lines.is_empty() {
        let text = if exit_code.is_none() {
            "running..."
        } else {
            "(no output)"
        };
        lines.push(shell_output_line(
            "  │ ",
            text,
            Style::default().fg(theme::DIM),
        ));
    } else {
        let max_lines = if detail_mode {
            SHELL_OUTPUT_DETAIL_LINES
        } else {
            SHELL_OUTPUT_COLLAPSED_LINES
        };
        for (idx, (line, is_error)) in output_lines.iter().enumerate() {
            if idx >= max_lines {
                let hint = if detail_mode {
                    format!(
                        "... output truncated at {} lines ({} more lines hidden)",
                        max_lines,
                        output_lines.len() - max_lines
                    )
                } else {
                    format!(
                        "... {} more lines hidden, Ctrl+O for details",
                        output_lines.len() - max_lines
                    )
                };
                lines.push(shell_output_line(
                    "  │ ",
                    &hint,
                    Style::default().fg(theme::DIM),
                ));
                break;
            }
            let default_style = if *is_error {
                Style::default().fg(theme::ERROR)
            } else {
                Style::default().fg(theme::MUTED)
            };
            lines.push(shell_output_line("  │ ", line, default_style));
        }
    }

    if let Some(code) = exit_code {
        let footer_style = if code == 0 {
            Style::default().fg(theme::SAGE)
        } else {
            Style::default().fg(theme::ERROR)
        };
        lines.push(shell_output_line(
            "  └ ",
            &format!("exit code {}", code),
            footer_style,
        ));
    }
    lines
}

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(
    vm: &MessageViewModel,
    _index: Option<usize>,
    _width: usize,
    detail_mode: bool,
) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble {
            rendered,
            system_reminder,
            expanded_content,
            ..
        } => {
            if *system_reminder {
                // 系统提醒：渲染一行简略提示
                let hint = Span::styled(
                    "\u{1f4cb} 上下文已压缩",
                    Style::default()
                        .fg(theme::DIM)
                        .add_modifier(Modifier::ITALIC),
                );
                return vec![Line::from(hint)];
            }

            // 详细模式且有展开内容时，显示完整的粘贴文本
            let effective_rendered = if detail_mode {
                if let Some(expanded) = expanded_content {
                    // 使用展开后的内容重新解析 markdown
                    super::markdown::parse_markdown_default(expanded)
                } else {
                    rendered.clone()
                }
            } else {
                rendered.clone()
            };

            // 普通 UserBubble — 原有渲染逻辑不变
            let user_bg: Color = theme::USER_BG;
            let mut lines = Vec::with_capacity(effective_rendered.lines.len() + 1);
            for (i, line) in effective_rendered.lines.iter().enumerate() {
                if i == 0 {
                    // 第一行：用户消息用 ❯ 前缀，带底色
                    let mut spans = vec![Span::styled(
                        "❯ ",
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD)
                            .bg(user_bg),
                    )];
                    for span in &line.spans {
                        spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                    }
                    lines.push(Line::from(spans));
                } else {
                    // 后续行：填充 + 原始 spans，带底色
                    let mut spans = vec![Span::styled("  ", Style::default().bg(user_bg))];
                    for span in &line.spans {
                        spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                    }
                    lines.push(Line::from(spans));
                }
            }
            lines
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            let mut lines = Vec::new();

            for block in blocks {
                match block {
                    ContentBlockView::Text { rendered, raw, .. } => {
                        let is_diff = peri_widgets::message_block::highlight::is_diff_content(raw);
                        if is_diff {
                            for l in raw.lines() {
                                let diff_spans =
                                    peri_widgets::message_block::highlight::highlight_diff_line(
                                        l,
                                        &peri_widgets::DarkTheme,
                                    );
                                lines.push(Line::from(diff_spans));
                            }
                        } else {
                            // AI 回复内容：与 Codex 对齐，第一行用 "• " 前缀，后续行用 "  " 缩进
                            // 注意：不能用 lines.is_empty() 判断，因为 Reasoning block 可能已先填充了 lines
                            // 用独立的 text_line_count 追踪 Text block 自身的行数
                            for (text_line_count, line) in rendered.lines.iter().enumerate() {
                                let prefix = if text_line_count == 0 { "● " } else { "  " };
                                let mut spans =
                                    vec![Span::styled(prefix, Style::default().fg(Color::White))];
                                for span in &line.spans {
                                    spans.push(span.clone());
                                }
                                lines.push(Line::from(spans));
                            }
                        }
                    }
                    ContentBlockView::Reasoning {
                        char_count,
                        tail_lines,
                        text,
                        ..
                    } => {
                        // Thought 标题：缩进 2 列对齐
                        lines.push(Line::from(vec![
                            Span::styled("∴ ", Style::default().fg(theme::DIM)),
                            Span::styled(
                                format!("Thought for {} chars (ctrl+o to expand)", char_count),
                                Style::default().fg(theme::DIM),
                            ),
                        ]));
                        // detail_mode 显示完整 reasoning，否则只显示 tail_lines
                        if detail_mode {
                            // 详细模式：显示完整 reasoning 内容
                            for tail_line in text.lines() {
                                lines.push(Line::from(vec![
                                    Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                    Span::styled(
                                        tail_line.to_string(),
                                        Style::default().fg(theme::DIM),
                                    ),
                                ]));
                            }
                        } else if let Some(tail) = tail_lines {
                            // 普通模式：只显示 tail 预览
                            for tail_line in tail.lines() {
                                lines.push(Line::from(vec![
                                    Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                    Span::styled(
                                        tail_line.to_string(),
                                        Style::default().fg(theme::DIM),
                                    ),
                                ]));
                            }
                            // tail 预览后加空行分隔后续内容
                            lines.push(Line::from(""));
                        } else {
                            // 无 tail 预览时，摘要行后加空行分隔
                            lines.push(Line::from(""));
                        }
                    }
                    ContentBlockView::ToolUse { .. } => {
                        // AI 消息不再显示工具调用行
                    }
                }
            }

            lines
        }
        MessageViewModel::ToolBlock {
            collapsed,
            display_name,
            args_display,
            content,
            color: _color,
            is_error,
            tool_name,
            diff_lines,
            ..
        } => {
            // AskUserQuestion 专用渲染路径
            if tool_name == "AskUserQuestion" {
                return render_ask_user_block(content, *is_error);
            }

            let is_running = content.is_empty() && !*is_error;

            // 构建状态（仅用于 header/collapse 管理）
            let status = if *is_error {
                peri_widgets::ToolCallStatus::Failed
            } else if is_running {
                peri_widgets::ToolCallStatus::Running
            } else {
                peri_widgets::ToolCallStatus::Completed
            };

            // 详细模式：强制展开所有工具；否则 Write/Edit 完成后默认展开
            let effective_collapsed = if detail_mode {
                false
            } else if !is_running && (tool_name == "Write" || tool_name == "Edit") {
                false
            } else {
                *collapsed
            };
            let mut state = peri_widgets::ToolCallState::new(display_name.clone(), theme::TEXT);
            state.status = status;
            state.collapsed = effective_collapsed;
            state.is_error = *is_error;
            if let Some(args) = args_display {
                state.args_summary = args.clone();
            }

            // 指示器颜色：Running=黄，Completed/Error=绿（对齐 Claude Hub）
            let indicator_color = if is_running {
                theme::YELLOW
            } else {
                theme::SAGE
            };

            // 图标：Running=◐ 闪烁，Completed=✓，Error=✓（对齐 Claude Hub）
            let indicator = if is_running {
                let tick = std::time::Instant::now().elapsed().as_millis() as u64 / 200;
                if (tick / 4).is_multiple_of(2) {
                    "◐"
                } else {
                    " "
                }
            } else {
                "✓"
            };

            // 工具名颜色：Running=青色 bold，Completed/Error=白色（对齐 Claude Hub）
            let name_style = if is_running {
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let mut header_spans = vec![
                Span::styled(indicator.to_string(), Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(state.tool_name.clone(), name_style),
            ];
            if !state.args_summary.is_empty() {
                let summary =
                    peri_widgets::tool_call::display::format_args_summary(&state.args_summary, 400);
                header_spans.push(Span::styled(
                    format!("({})", summary),
                    Style::default().fg(theme::DIM),
                ));
            }
            let mut lines = vec![Line::from(header_spans)];
            let result_lines: Vec<&str> = if content.is_empty() {
                Vec::new()
            } else {
                content.split('\n').collect()
            };
            if !state.collapsed && !result_lines.is_empty() {
                let result_color = if *is_error {
                    theme::ERROR
                } else {
                    theme::MUTED
                };
                let border_color = if *is_error { theme::ERROR } else { theme::DIM };
                // 详细模式显示完整内容，否则截断
                let max_lines = if detail_mode { usize::MAX } else { 20 };
                for (i, line) in result_lines.iter().enumerate() {
                    if i >= max_lines {
                        lines.push(Line::from(vec![
                            Span::styled("  ⎿ ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("... ({} more lines)", result_lines.len() - max_lines),
                                Style::default().fg(theme::DIM),
                            ),
                        ]));
                        break;
                    }
                    lines.push(Line::from(vec![
                        Span::styled("  ⎿ ".to_string(), Style::default().fg(border_color)),
                        Span::styled((*line).to_string(), Style::default().fg(result_color)),
                    ]));
                }
            } else if *is_error && !content.is_empty() {
                lines.extend(error_summary_lines(content));
            }
            if detail_mode {
                if let Some(ref cached_lines) = diff_lines {
                    lines.extend(cached_lines.iter().cloned());
                }
            }
            lines
        }
        MessageViewModel::ShellCommand {
            command,
            cwd,
            stdin,
            stdout,
            stderr,
            exit_code,
            ..
        } => render_shell_command(command, cwd, stdin, stdout, stderr, *exit_code, detail_mode),
        MessageViewModel::SubAgentGroup {
            batch_agents,
            collapsed,
            ..
        } if !batch_agents.is_empty() => render_batch_summary(batch_agents, collapsed),
        MessageViewModel::SubAgentGroup {
            agent_id,
            task_preview,
            recent_messages,
            collapsed,
            is_error,
            is_running,
            is_background: _,
            bg_hash,
            final_result,
            ..
        } => {
            let mut lines: Vec<Line<'static>> = Vec::new();

            // 状态指示器颜色（对齐 Claude Hub）
            let indicator_icon = if *is_running { "◐" } else { "✓" };
            let indicator_color = if *is_running {
                theme::YELLOW
            } else {
                theme::SAGE
            };
            let agent_name_color = if *is_error {
                theme::ERROR
            } else {
                theme::MAGENTA
            };

            if *collapsed {
                // 折叠状态：两行显示
                // Header: ◐ Agent(type) #hash
                let mut header_spans = vec![
                    Span::styled(
                        format!("{} ", indicator_icon),
                        Style::default().fg(indicator_color),
                    ),
                    Span::styled(
                        "Agent".to_string(),
                        Style::default()
                            .fg(agent_name_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("({})", agent_id), Style::default().fg(theme::MUTED)),
                ];
                // 折叠状态显示短 hash
                if let Some(ref hash) = bg_hash {
                    header_spans.push(Span::styled(
                        format!(" #{}", hash),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                lines.push(Line::from(header_spans));

                let task_label: String = task_preview.chars().take(50).collect();
                let suffix = if task_preview.chars().count() > 50 {
                    "…"
                } else {
                    ""
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}{}", task_label, suffix),
                    Style::default().fg(theme::MUTED),
                )]));
                if *is_error {
                    if let Some(ref result) = final_result {
                        if !result.is_empty() {
                            lines.extend(error_summary_lines(result));
                        }
                    }
                }
            } else {
                // 展开状态：名称 + 任务描述
                // Header: ◐ Agent(type) #hash
                let mut header_spans = vec![
                    Span::styled(
                        format!("{} ", indicator_icon),
                        Style::default().fg(indicator_color),
                    ),
                    Span::styled(
                        "Agent".to_string(),
                        Style::default()
                            .fg(agent_name_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("({})", agent_id), Style::default().fg(theme::MUTED)),
                ];
                // 展开状态显示短 hash
                if let Some(ref hash) = bg_hash {
                    header_spans.push(Span::styled(
                        format!(" #{}", hash),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                lines.push(Line::from(header_spans));

                let task_label: String = task_preview.chars().take(50).collect();
                let suffix = if task_preview.chars().count() > 50 {
                    "…"
                } else {
                    ""
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}{}", task_label, suffix),
                    Style::default().fg(theme::MUTED),
                )]));

                // 嵌套消息（不渲染序号），跳过无可见内容的条目
                // 当有 final_result 时，跳过最后一条消息（其内容已包含在 final_result 中）
                let has_final = final_result.as_ref().is_some_and(|r| !r.is_empty());
                let skip_last = has_final && recent_messages.len() > 1;
                let iter_messages: &[MessageViewModel] = if skip_last {
                    &recent_messages[..recent_messages.len() - 1]
                } else {
                    recent_messages
                };
                for inner_vm in iter_messages.iter() {
                    // SubAgent 内部跳过 AssistantBubble，只显示工具调用
                    if matches!(inner_vm, MessageViewModel::AssistantBubble { .. }) {
                        continue;
                    }
                    let inner_lines = render_view_model(inner_vm, None, _width, detail_mode);
                    if inner_lines.is_empty() {
                        continue;
                    }
                    for line in inner_lines {
                        // 每行前缀 2 空格缩进
                        let mut new_spans = vec![Span::raw("  ")];
                        new_spans.extend(line.spans);
                        lines.push(Line::from(new_spans));
                    }
                }
                // 移除尾部空行
                while lines.last().is_some_and(|l| l.spans.is_empty()) {
                    lines.pop();
                }

                // 子 agent 完成后，渲染 final_result 摘要（仅第一行）
                if let Some(ref result) = final_result {
                    if !result.is_empty() {
                        if let Some(first_line) = result.lines().next() {
                            if !first_line.is_empty() {
                                let text: String = first_line.chars().take(80).collect();
                                lines.push(Line::from(vec![
                                    Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                    Span::styled(text, Style::default().fg(theme::MUTED)),
                                ]));
                            }
                        }
                    }
                }
            }

            lines
        }
        MessageViewModel::SystemNote { content, .. } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                if line.starts_with('✻') {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme::DIM),
                    )));
                } else if line.starts_with('⎿') {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(theme::MUTED),
                    )));
                } else {
                    let is_error =
                        line.contains("❌") || line.contains("失败") || line.contains("错误");
                    let is_warn = line.contains("⚠") || line.contains("已中断");
                    let text_color = if is_error {
                        theme::ERROR
                    } else if is_warn {
                        theme::WARNING
                    } else {
                        theme::MUTED
                    };
                    lines.push(Line::from(vec![
                        Span::styled("· ", Style::default().fg(theme::DIM)),
                        Span::styled(line.to_string(), Style::default().fg(text_color)),
                    ]));
                }
            }
            lines
        }
        MessageViewModel::CacheWarning { content, .. } => {
            vec![Line::from(Span::styled(
                content.clone(),
                Style::default().fg(theme::WARNING),
            ))]
        }
        MessageViewModel::ToolCallGroup {
            category,
            tools,
            collapsed: _collapsed,
            ..
        } => {
            let mut lines = Vec::new();

            if *category == ToolCategory::AskUser {
                // AskUserQuestion 聚合：统一标题 + 所有问答对
                let has_error = tools.iter().any(|t| t.is_error);
                let color = if has_error { theme::ERROR } else { theme::SAGE };
                lines.push(Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(
                        "User answered Peri's questions:".to_string(),
                        Style::default().fg(theme::TEXT),
                    ),
                ]));

                for entry in tools {
                    let entry_color = if entry.is_error {
                        theme::ERROR
                    } else {
                        theme::MUTED
                    };
                    if entry.content.is_empty() {
                        continue;
                    }
                    // 解析每个工具结果中的问答对
                    for block in entry.content.split("\n\n") {
                        let mut header = String::new();
                        let mut answer = String::new();
                        for line in block.lines() {
                            if let Some(rest) = line.strip_prefix("[问: ") {
                                header = rest.trim_end_matches(']').to_string();
                            } else if let Some(a) = line.strip_prefix("回答: ") {
                                answer = a.to_string();
                            }
                        }
                        header = header.replace(['\n', '\r'], " ");
                        answer = answer.replace(['\n', '\r'], " ");
                        let text = if !header.is_empty() {
                            format!("{} → {}", header, answer)
                        } else if !answer.is_empty() {
                            answer
                        } else {
                            block.lines().collect::<Vec<_>>().join(" ")
                        };
                        if text.is_empty() {
                            continue;
                        }
                        lines.push(Line::from(vec![
                            Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                            Span::styled(text, Style::default().fg(entry_color)),
                        ]));
                    }
                }
            } else if detail_mode {
                // 详细模式：显示每条工具的名称和结果
                for entry in tools {
                    let entry_color = if entry.is_error {
                        theme::ERROR
                    } else {
                        theme::SAGE
                    };
                    let indicator = if entry.is_error { "✗" } else { "●" };
                    lines.push(Line::from(vec![
                        Span::styled(indicator.to_string(), Style::default().fg(entry_color)),
                        Span::raw(" "),
                        Span::styled(
                            entry.display_name.clone(),
                            Style::default()
                                .fg(theme::TEXT)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    if !entry.content.is_empty() {
                        for line in entry.content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                Span::styled(line.to_string(), Style::default().fg(theme::MUTED)),
                            ]));
                        }
                    }
                }
            } else {
                let summary = ToolCategory::summary_for_tools(tools);

                // 统一 ● 前缀，仅显示汇总行
                lines.push(Line::from(vec![
                    Span::styled("● ", Style::default().fg(theme::SAGE)),
                    Span::styled(summary, Style::default().fg(theme::MUTED)),
                ]));
                // 显示出错工具的错误摘要
                for entry in tools {
                    if entry.is_error && !entry.content.is_empty() {
                        lines.extend(error_summary_lines(&entry.content));
                    }
                }
            }

            lines
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::message_view::AgentSummary;
    include!("message_render_test.rs");
}
