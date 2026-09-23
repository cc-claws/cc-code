use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{
    message_view::{AgentSummary, ContentBlockView, MessageViewModel, ToolCategory},
    theme,
};
use crate::app::tool_display::sanitize_display_text;

pub(crate) const CONTROL_B_BACKGROUND_HINT: &str = "(ctrl+b to run in background)";

/// 从 Bash 工具输出中解析 exit code。
///
/// 匹配格式：`[Exit code: N]` 或 `[Command completed with exit code N]`
/// 解析失败返回 None。
fn parse_exit_code(content: &str) -> Option<i32> {
    // 优先匹配非零退出码格式 "[Exit code: N]"
    if let Some(pos) = content.rfind("[Exit code: ") {
        let rest = &content[pos + "[Exit code: ".len()..];
        if let Some(end) = rest.find(']') {
            if let Ok(code) = rest[..end].trim().parse::<i32>() {
                return Some(code);
            }
        }
    }
    // 兜底：空输出时格式为 "[Command completed with exit code N]"
    if let Some(pos) = content.rfind("exit code ") {
        let rest = &content[pos + "exit code ".len()..];
        if let Some(end) = rest.find(']') {
            if let Ok(code) = rest[..end].trim().parse::<i32>() {
                return Some(code);
            }
        }
    }
    None
}

/// 将 markdown 渲染结果的所有 span 颜色压暗到 DIM 色板。
///
/// 保留 syntect 语法高亮的色相，但统一降亮度，
/// 使 reasoning 内容在视觉层级上低于正文。
fn dim_markdown_lines(text: Text<'static>) -> Vec<Line<'static>> {
    text.lines
        .into_iter()
        .map(|line| {
            let dimmed: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|span| {
                    let style = span.style;
                    // 如果 span 有前景色，保持色相但强制加 DIM 修饰
                    // 如果无前景色（默认色），直接设为 DIM
                    let dim_style = if style.fg.is_some() {
                        style.add_modifier(Modifier::DIM)
                    } else {
                        Style::default().fg(theme::DIM)
                    };
                    Span::styled(span.content, dim_style)
                })
                .collect();
            Line::from(dimmed)
        })
        .collect()
}

const SHELL_OUTPUT_COLLAPSED_LINES: usize = 6;
const SHELL_OUTPUT_DETAIL_LINES: usize = 40;

/// 折行输出段：line 为渲染行，其余字段用于链接命中区映射
struct WrappedLineSeg {
    line: Line<'static>,
    /// 输入行 plain_text 的 grapheme 范围 [in_g_start, in_g_end)
    in_g_start: usize,
    in_g_end: usize,
    /// in_g_start 映射到输出行 plain_text 的 grapheme 起点
    out_g_offset: usize,
}

/// 将含多 span 的 Line 按视觉宽度折行，保留各 span 样式。
///
/// 算法：flatten → 贪心宽度折行（单词边界优先）→ reassemble。
/// 用于 reasoning 渲染：每行宽度 ≤ max_width 时不会触发 Paragraph::wrap 二次折行，
/// 避免续行丢失 4 列前缀缩进。CJK 无空格场景按 grapheme 硬断。
/// 以 grapheme 为切分单位，返回段级 g 映射用于链接命中区。
fn wrap_line_spans_rich(line: Line<'static>, max_width: usize) -> Vec<WrappedLineSeg> {
    use unicode_segmentation::UnicodeSegmentation;

    if max_width == 0 || line.spans.is_empty() {
        let g_len = line
            .spans
            .iter()
            .map(|s| s.content.as_ref().graphemes(true).count())
            .sum();
        return vec![WrappedLineSeg {
            line,
            in_g_start: 0,
            in_g_end: g_len,
            out_g_offset: 0,
        }];
    }

    // flatten：spans → Vec<(grapheme, Style)>，消除 span 边界以便任意位置断行
    let flat: Vec<(&str, Style)> = line
        .spans
        .iter()
        .flat_map(|s| s.content.graphemes(true).map(move |g| (g, s.style)))
        .collect();

    // 快速路径：总宽度不超过 max_width 时原样返回
    let total_width: usize = flat.iter().map(|(g, _)| g.width()).sum();
    let total_len = flat.len();
    if total_width <= max_width {
        drop(flat);
        return vec![WrappedLineSeg {
            line,
            in_g_start: 0,
            in_g_end: total_len,
            out_g_offset: 0,
        }];
    }

    let mut result: Vec<WrappedLineSeg> = Vec::new();
    let mut pos = 0;
    while pos < flat.len() {
        // 贪心：从 pos 起尽可能多地装入 grapheme
        let mut cur_width = 0usize;
        let mut content_end = pos;
        for i in pos..flat.len() {
            let cw = flat[i].0.width();
            if content_end > pos && cur_width + cw > max_width {
                break;
            }
            cur_width += cw;
            content_end = i + 1;
        }

        // 单词边界优先：从 content_end 往回找最后一个 whitespace
        let mut break_at = content_end;
        for i in (pos..content_end).rev() {
            if flat[i].0.chars().all(char::is_whitespace) {
                break_at = i;
                break;
            }
        }

        // trim 行首行尾空白
        let mut seg_start = pos;
        while seg_start < break_at && flat[seg_start].0.chars().all(char::is_whitespace) {
            seg_start += 1;
        }
        let mut seg_end = break_at;
        while seg_end > seg_start && flat[seg_end - 1].0.chars().all(char::is_whitespace) {
            seg_end -= 1;
        }

        // reassemble：相邻同 Style grapheme 合并为一个 Span
        if seg_start < seg_end {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut cur_text = String::new();
            let mut cur_style = flat[seg_start].1;
            for &(g, st) in &flat[seg_start..seg_end] {
                if st == cur_style {
                    cur_text.push_str(g);
                } else {
                    spans.push(Span::styled(std::mem::take(&mut cur_text), cur_style));
                    cur_text = g.to_string();
                    cur_style = st;
                }
            }
            if !cur_text.is_empty() {
                spans.push(Span::styled(cur_text, cur_style));
            }
            result.push(WrappedLineSeg {
                line: Line::from(spans),
                in_g_start: seg_start,
                in_g_end: seg_end,
                out_g_offset: 0,
            });
        }

        // 推进 pos，跳过断行点后的连续空白
        pos = break_at;
        while pos < flat.len() && flat[pos].0.chars().all(char::is_whitespace) {
            pos += 1;
        }
    }

    if result.is_empty() {
        vec![WrappedLineSeg {
            line: Line::default(),
            in_g_start: 0,
            in_g_end: 0,
            out_g_offset: 0,
        }]
    } else {
        result
    }
}

/// 兼容包装：只取折行结果
fn wrap_line_spans(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    wrap_line_spans_rich(line, max_width)
        .into_iter()
        .map(|seg| seg.line)
        .collect()
}

/// 把逻辑行上的链接命中区映射到折行后的输出段，累加前缀宽度后推入 out
fn push_link_hits_for_wrapped(
    out: &mut Vec<peri_widgets::markdown::LinkHit>,
    base_line: usize,
    line_links: &[peri_widgets::markdown::LinkHit],
    wrapped: &[WrappedLineSeg],
    prefix_g: usize,
) {
    for hit in line_links {
        for (i, seg) in wrapped.iter().enumerate() {
            let is = hit.g_start.max(seg.in_g_start);
            let ie = hit.g_end.min(seg.in_g_end);
            if is < ie {
                out.push(peri_widgets::markdown::LinkHit {
                    line: base_line + i,
                    g_start: prefix_g + seg.out_g_offset + (is - seg.in_g_start),
                    g_end: prefix_g + seg.out_g_offset + (ie - seg.in_g_start),
                    url: hit.url.clone(),
                });
            }
        }
    }
}

/// 取 rendered_links 中属于第 line_idx 个逻辑行的命中区
fn links_on_line(
    links: &[peri_widgets::markdown::LinkHit],
    line_idx: usize,
) -> impl Iterator<Item = &peri_widgets::markdown::LinkHit> {
    links.iter().filter(move |h| h.line == line_idx)
}

/// Generate always-visible error summary lines (up to 400 Unicode chars).
/// 2-space indent, no vertical bar, no prefix. Preserves newlines (multi-line render).
fn error_summary_lines(content: &str) -> Vec<Line<'static>> {
    let truncated: String = content.chars().take(400).collect();
    truncated
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 { "  ⎿ " } else { "    " };
            Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme::DIM)),
                Span::styled(
                    sanitize_display_text(line),
                    Style::default().fg(theme::ERROR),
                ),
            ])
        })
        .collect()
}

/// 按显示列宽（unicode-width）截断字符串。
/// 若超出 max_width，则截断并追加 '…'（占 1 列宽），确保结果总显示列宽不超过 max_width。
pub(crate) fn truncate_to_display_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let total_width: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total_width <= max_width {
        return s.to_string();
    }

    let target_width = max_width.saturating_sub(1);
    let mut cur_width = 0;
    let mut result = String::new();
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if cur_width + cw > target_width {
            break;
        }
        cur_width += cw;
        result.push(c);
    }
    result.push('…');
    result
}

fn tool_args_header(tool_name: &str, args: &str, max_width: usize) -> String {
    let sanitized_args = sanitize_display_text(args);
    if tool_name == "Glob" {
        // "pattern: \"...\"" 占 11 列固定前缀/后缀宽度
        let inner_max = max_width.saturating_sub(11);
        let summary = truncate_to_display_width(&sanitized_args, inner_max);
        format!("pattern: \"{}\"", summary)
    } else {
        truncate_to_display_width(&sanitized_args, max_width)
    }
}

fn read_summary(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    if let Some(first_line) = content.lines().next() {
        if first_line.starts_with("Read ") && first_line.ends_with(" lines") {
            return Some(sanitize_display_text(first_line));
        }
    }
    Some(format!("Read {} lines", content.lines().count()))
}

fn glob_summary(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    let count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Some(format!("Found {} files", count))
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

/// AskUserQuestion 专用渲染：`● User answered CC Code's questions:` + `⎿ · H → V`
fn render_ask_user_block(content: &str, is_error: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let color = if is_error { theme::ERROR } else { theme::SAGE };
    lines.push(Line::from(vec![
        Span::styled("● ", Style::default().fg(color)),
        Span::styled(
            "User answered CC Code's questions:".to_string(),
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
            let seq_start = i + 2;
            let mut seq_end = None;
            let mut final_char = None;
            for (offset, ch) in line[seq_start..].char_indices() {
                if matches!(ch, '\u{0040}'..='\u{007e}') {
                    seq_end = Some(seq_start + offset);
                    final_char = Some(ch);
                    break;
                }
            }
            if let (Some(end), Some(final_ch)) = (seq_end, final_char) {
                if final_ch == 'm' {
                    if !buf.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut buf), style));
                    }
                    let codes = &line[seq_start..end];
                    apply_sgr_codes(&mut style, default_style, codes);
                }
                i = end + final_ch.len_utf8();
                continue;
            }
        }
        if line[i..].starts_with("\x1b]") {
            let rest = &line[i + 2..];
            if let Some(end) = rest.find('\u{0007}') {
                i += end + 3;
                continue;
            }
            if let Some(end) = rest.find("\x1b\\") {
                i += end + 4;
                continue;
            }
            break;
        }
        let Some(ch) = line[i..].chars().next() else {
            break;
        };
        if ch == '\t' {
            buf.push(' ');
        } else if !ch.is_control() {
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

/// 渲染用户 `!` 本机命令的标题行。
///
/// 标题行使用整行背景和粉色 `!` 标记，保持与 Claude Code 的本地命令块一致。
fn shell_command_header(command: &str, width: usize) -> Line<'static> {
    let command =
        truncate_to_display_width(&sanitize_display_text(command), width.saturating_sub(2));
    let header_bg = Style::default().bg(theme::USER_BG);
    let mut spans = vec![
        Span::styled("! ", header_bg.fg(theme::BASH_BORDER)),
        Span::styled(command.clone(), header_bg.fg(theme::TEXT)),
    ];
    let used_width = 2 + UnicodeWidthStr::width(command.as_str());
    if used_width < width {
        spans.push(Span::styled(" ".repeat(width - used_width), header_bg));
    }
    Line::from(spans)
}

#[allow(clippy::too_many_arguments)]
fn render_shell_command(
    command: &str,
    stdin: &[String],
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    detail_mode: bool,
    width: usize,
    started_at: Option<std::time::Instant>,
    moved_to_background: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(shell_command_header(command, width));

    let mut output_lines: Vec<(String, bool)> = Vec::new();
    for input in stdin {
        output_lines.push((format!("< {}", sanitize_display_text(input)), false));
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
            "(No output)"
        };
        lines.push(shell_output_line(
            "  └ ",
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
                    "    ",
                    &hint,
                    Style::default().fg(theme::DIM),
                ));
                break;
            }
            let default_style = if *is_error && exit_code != Some(0) {
                Style::default().fg(theme::ERROR)
            } else {
                Style::default().fg(theme::MUTED)
            };
            let prefix = if idx == 0 { "  └ " } else { "    " };
            lines.push(shell_output_line(prefix, line, default_style));
        }
    }

    // 2 秒阈值提示：前台 running 超 2 秒显示已运行时间 + "(Ctrl+B to run in background)"（对齐效果图场景 1）
    if !moved_to_background
        && exit_code.is_none()
        && started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2))
    {
        let elapsed = started_at.unwrap().elapsed();
        let secs = elapsed.as_secs();
        let elapsed_str = if secs >= 60 {
            format!("({}m {:02}s)", secs / 60, secs % 60)
        } else {
            format!("({}s)", secs)
        };
        lines.push(shell_output_line(
            "    ",
            &elapsed_str,
            Style::default().fg(theme::MUTED),
        ));
        lines.push(shell_output_line(
            "    ",
            CONTROL_B_BACKGROUND_HINT,
            Style::default().fg(theme::MUTED),
        ));
    }
    lines
}

/// 将单个 ViewModel 渲染为 Vec<Line>
pub fn render_view_model(
    vm: &MessageViewModel,
    index: Option<usize>,
    width: usize,
    detail_mode: bool,
    tick: u64,
) -> Vec<Line<'static>> {
    render_view_model_with_links(vm, index, width, detail_mode, tick).0
}

/// 将单个 ViewModel 渲染为 Vec<Line>，并收集超链接命中区（行号与输出 lines 对齐）
pub fn render_view_model_with_links(
    vm: &MessageViewModel,
    _index: Option<usize>,
    width: usize,
    detail_mode: bool,
    tick: u64,
) -> (Vec<Line<'static>>, Vec<peri_widgets::markdown::LinkHit>) {
    let mut link_hits: Vec<peri_widgets::markdown::LinkHit> = Vec::new();
    match vm {
        MessageViewModel::UserBubble {
            rendered,
            rendered_links,
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
                return (vec![Line::from(hint)], link_hits);
            }

            // 详细模式且有展开内容时，显示完整的粘贴文本
            let (effective_rendered, effective_links) = if detail_mode {
                if let Some(expanded) = expanded_content {
                    // 使用展开后的内容重新解析 markdown
                    let doc = super::markdown::parse_markdown_default_rich(expanded);
                    (doc.text, doc.links)
                } else {
                    (rendered.clone(), rendered_links.clone())
                }
            } else {
                (rendered.clone(), rendered_links.clone())
            };

            // 普通 UserBubble — 原有渲染逻辑不变
            let user_bg: Color = theme::USER_BG;
            // [TRAP] 同 AssistantBubble Text 路径：markdown 普通段落不预折行，
            // 需先按 content_width 预折行，防止 Paragraph::wrap 二次硬折行后续行丢掉 "  " 悬挂缩进
            let content_width = width.saturating_sub(2).max(20);
            let mut lines = Vec::with_capacity(effective_rendered.lines.len() + 1);
            for (i, line) in effective_rendered.lines.iter().enumerate() {
                let wrapped = wrap_line_spans_rich(line.clone(), content_width);
                // 前缀 "❯ " / "  " 均为 2 grapheme
                let line_links: Vec<peri_widgets::markdown::LinkHit> =
                    links_on_line(&effective_links, i).cloned().collect();
                push_link_hits_for_wrapped(&mut link_hits, lines.len(), &line_links, &wrapped, 2);
                for wline in wrapped.into_iter().map(|seg| seg.line) {
                    if i == 0 && lines.is_empty() {
                        // 第一行：用户消息用 ❯ 前缀，带底色
                        let mut spans = vec![Span::styled(
                            "❯ ",
                            Style::default()
                                .fg(theme::ACCENT)
                                .add_modifier(Modifier::BOLD)
                                .bg(user_bg),
                        )];
                        for span in &wline.spans {
                            spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                        }
                        lines.push(Line::from(spans));
                    } else {
                        // 后续行（含预折行产生的续行）：2 空格悬挂缩进，带底色
                        let mut spans = vec![Span::styled("  ", Style::default().bg(user_bg))];
                        for span in &wline.spans {
                            spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                        }
                        lines.push(Line::from(spans));
                    }
                }
            }
            (lines, link_hits)
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            let mut lines = Vec::new();

            for block in blocks {
                match block {
                    ContentBlockView::Text {
                        rendered,
                        rendered_links,
                        raw,
                        ..
                    } => {
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
                            // AI 回复内容：与 Codex 对齐，第一行用 "● " 前缀，后续行用 "  " 缩进
                            // 注意：不能用 lines.is_empty() 判断，因为 Reasoning block 可能已先填充了 lines
                            // 用独立的 text_line_count 追踪 Text block 自身的行数
                            // [TRAP] markdown 层普通段落不预折行（flush_line 仅列表/引用走悬挂缩进），
                            // 超宽 Line 加前缀后会触发 Paragraph::wrap 二次硬折行，续行丢掉 "  " 缩进。
                            // 与 Reasoning 路径一致：先按 content_width 预折行再加前缀
                            let content_width = width.saturating_sub(2).max(20);
                            for (text_line_count, line) in rendered.lines.iter().enumerate() {
                                let wrapped = wrap_line_spans_rich(line.clone(), content_width);
                                // 前缀 "● " / "  " 均为 2 grapheme
                                let line_links: Vec<peri_widgets::markdown::LinkHit> =
                                    links_on_line(rendered_links, text_line_count)
                                        .cloned()
                                        .collect();
                                push_link_hits_for_wrapped(
                                    &mut link_hits,
                                    lines.len(),
                                    &line_links,
                                    &wrapped,
                                    2,
                                );
                                for (j, seg) in wrapped.into_iter().enumerate() {
                                    let wline = seg.line;
                                    let prefix = if text_line_count == 0 && j == 0 {
                                        "● "
                                    } else {
                                        "  "
                                    };
                                    let mut spans = vec![Span::styled(
                                        prefix,
                                        Style::default().fg(Color::White),
                                    )];
                                    for span in &wline.spans {
                                        spans.push(span.clone());
                                    }
                                    lines.push(Line::from(spans));
                                }
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
                        let hint = if detail_mode {
                            ""
                        } else {
                            " (ctrl+o to expand)"
                        };
                        lines.push(Line::from(vec![
                            Span::styled("∴ ", Style::default().fg(theme::DIM)),
                            Span::styled(
                                format!("Thought for {} chars{}", char_count, hint),
                                Style::default().fg(theme::DIM),
                            ),
                        ]));
                        // detail_mode 显示完整 reasoning，否则只显示 tail_lines
                        // 两者都走 markdown 解析 + DIM overlay，代码块获得语法高亮
                        let content = if detail_mode {
                            Some(text.as_str())
                        } else {
                            tail_lines.as_deref()
                        };
                        if let Some(content_text) = content {
                            // 减去前缀宽度（"  ⎿ " 或 "    " = 4 字符）
                            let content_width = width.saturating_sub(4).max(20);
                            let parsed =
                                super::markdown::parse_markdown(content_text, content_width);
                            let dimmed = dim_markdown_lines(parsed);
                            for (i, line) in dimmed.into_iter().enumerate() {
                                // 折行：长段落按 content_width 切成多行，每行加 4 列前缀
                                // 后不会超过 width，避免 Paragraph::wrap 二次折行使续行
                                // 缺少缩进
                                for (j, wline) in
                                    wrap_line_spans(line, content_width).into_iter().enumerate()
                                {
                                    let prefix = if i == 0 && j == 0 { "  ⎿ " } else { "    " };
                                    let mut spans =
                                        vec![Span::styled(prefix, Style::default().fg(theme::DIM))];
                                    spans.extend(wline.spans);
                                    lines.push(Line::from(spans));
                                }
                            }
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

            (lines, link_hits)
        }
        MessageViewModel::ToolBlock {
            collapsed,
            display_name,
            args_display,
            content,
            color: _color,
            is_error,
            tool_name,
            diff_input,
            started_at,
            ..
        } => {
            // AskUserQuestion 专用渲染路径
            if tool_name == "AskUserQuestion" {
                return (render_ask_user_block(content, *is_error), link_hits);
            }

            let is_running = content.is_empty() && !*is_error;

            // 构建状态（仅用于 header/collapse 管理）
            // Bash 工具：从输出中解析 exit code，非零则标记为 Failed
            let bash_failed = tool_name == "Bash"
                && !*is_error
                && !is_running
                && parse_exit_code(content).is_some_and(|c| c != 0);
            let status = if *is_error || bash_failed {
                peri_widgets::ToolCallStatus::Failed
            } else if is_running {
                peri_widgets::ToolCallStatus::Running
            } else {
                peri_widgets::ToolCallStatus::Completed
            };

            // 详细模式：强制展开所有工具；否则 Write/Edit 完成后默认展开
            let effective_collapsed = if detail_mode {
                // Read 始终折叠：内容是文件原文，无需在 detail mode 展开
                tool_name == "Read"
            } else if !is_running && (tool_name == "Write" || tool_name == "Edit") {
                false
            } else {
                *collapsed
            };
            let mut state = peri_widgets::ToolCallState::new(display_name.clone(), theme::TEXT);
            state.status = status.clone();
            state.collapsed = effective_collapsed;
            state.is_error = *is_error || bash_failed;
            if let Some(args) = args_display {
                state.args_summary = args.clone();
            }

            // 复用 widget 层指示器：统一 ● 圆点 + 颜色语义化
            let (indicator, indicator_color) =
                peri_widgets::tool_call::display::format_indicator(status, tick);

            // 工具名颜色：Running=青色 bold（Bash 除外，保持白色），Completed=白色，Error=红色
            let name_style = if is_running && tool_name != "Bash" {
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else if is_running {
                Style::default().fg(theme::TEXT)
            } else if *is_error {
                Style::default().fg(theme::ERROR)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let mut header_spans = vec![
                Span::styled(indicator.to_string(), Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(state.tool_name.clone(), name_style),
            ];
            if !state.args_summary.is_empty() {
                let prefix_width = UnicodeWidthStr::width(indicator)
                    + 1
                    + UnicodeWidthStr::width(state.tool_name.as_str())
                    + 2;
                let max_args_width = width.saturating_sub(prefix_width + 2).min(400);
                let summary = tool_args_header(tool_name, &state.args_summary, max_args_width);
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
                if tool_name == "Glob" && !*is_error {
                    if let Some(summary) = glob_summary(content) {
                        lines.push(Line::from(vec![
                            Span::styled("  ⎿ ", Style::default().fg(border_color)),
                            Span::styled(summary, Style::default().fg(result_color)),
                        ]));
                    }
                }
                for (i, line) in result_lines.iter().enumerate() {
                    if i >= max_lines {
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("... ({} more lines)", result_lines.len() - max_lines),
                                Style::default().fg(theme::DIM),
                            ),
                        ]));
                        break;
                    }
                    let prefix = if i == 0 && tool_name != "Glob" {
                        "  ⎿ "
                    } else {
                        "    "
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(border_color)),
                        Span::styled(
                            sanitize_display_text(line),
                            Style::default().fg(result_color),
                        ),
                    ]));
                }
            } else if *is_error && !content.is_empty() {
                lines.extend(error_summary_lines(content));
            }
            // Read 工具折叠态：显示行数摘要
            if state.collapsed && tool_name == "Read" && !result_lines.is_empty() {
                if let Some(summary) = read_summary(content) {
                    lines.push(Line::from(vec![
                        Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                        Span::styled(summary, Style::default().fg(theme::MUTED)),
                    ]));
                }
            }
            if tool_name == "Bash"
                && is_running
                && started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2))
            {
                let elapsed = started_at.unwrap().elapsed();
                let secs = elapsed.as_secs();
                let elapsed_str = if secs >= 60 {
                    format!("({}m {:02}s)", secs / 60, secs % 60)
                } else {
                    format!("({}s)", secs)
                };
                lines.push(Line::from(vec![
                    Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                    Span::styled(
                        format!("Running… {}", elapsed_str),
                        Style::default().fg(theme::MUTED),
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default().fg(theme::DIM)),
                    Span::styled(CONTROL_B_BACKGROUND_HINT, Style::default().fg(theme::MUTED)),
                ]));
            }
            if detail_mode {
                if let Some(ref diff_input) = diff_input {
                    // 前缀 "  ⎿ " / "    " 占 4 列，diff 内容宽度需减去前缀
                    let diff_width = width.saturating_sub(4);
                    let mut rendered = peri_widgets::diff::render_diff(
                        diff_input,
                        diff_width,
                        &peri_widgets::DarkTheme,
                    );
                    // 去掉 diff 标题行（file_path），header 的 args_display 已经显示了文件路径
                    if !rendered.is_empty() {
                        rendered.remove(0);
                    }
                    for (i, line) in rendered.iter().enumerate() {
                        let mut prefixed = line.clone();
                        let prefix = if i == 0 { "  ⎿ " } else { "    " };
                        prefixed.spans.insert(
                            0,
                            Span::styled(
                                prefix,
                                Style::default().fg(if *is_error {
                                    theme::ERROR
                                } else {
                                    theme::DIM
                                }),
                            ),
                        );
                        lines.push(prefixed);
                    }
                }
            }
            (lines, link_hits)
        }
        MessageViewModel::ShellCommand {
            command,
            stdin,
            stdout,
            stderr,
            exit_code,
            started_at,
            moved_to_background,
            ..
        } => (
            render_shell_command(
                command,
                stdin,
                stdout,
                stderr,
                *exit_code,
                detail_mode,
                width,
                *started_at,
                *moved_to_background,
            ),
            link_hits,
        ),
        MessageViewModel::SubAgentGroup {
            batch_agents,
            collapsed,
            ..
        } if !batch_agents.is_empty() => (render_batch_summary(batch_agents, collapsed), link_hits),
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
                    let inner_lines = render_view_model(inner_vm, None, width, detail_mode, tick);
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

            (lines, link_hits)
        }
        MessageViewModel::SystemNote { content, .. } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                if line.starts_with('✻') {
                    lines.push(Line::from(Span::styled(
                        sanitize_display_text(line),
                        Style::default().fg(theme::DIM),
                    )));
                } else if line.starts_with('⎿') {
                    lines.push(Line::from(Span::styled(
                        sanitize_display_text(line),
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
                        Span::styled(sanitize_display_text(line), Style::default().fg(text_color)),
                    ]));
                }
            }
            (lines, link_hits)
        }
        MessageViewModel::CacheWarning { content, .. } => (
            vec![Line::from(Span::styled(
                content.clone(),
                Style::default().fg(theme::WARNING),
            ))],
            link_hits,
        ),
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
                        "User answered CC Code's questions:".to_string(),
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
                // Read 工具只显示行数摘要，不展开文件内容
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
                    if let Some(args) = &entry.args_display {
                        if !args.is_empty() {
                            let prefix_width = UnicodeWidthStr::width(indicator)
                                + 1
                                + UnicodeWidthStr::width(entry.display_name.as_str())
                                + 2;
                            let max_args_width = width.saturating_sub(prefix_width + 2).min(400);
                            let summary = tool_args_header(&entry.tool_name, args, max_args_width);
                            if let Some(last_line) = lines.last_mut() {
                                last_line.spans.push(Span::styled(
                                    format!("({})", summary),
                                    Style::default().fg(theme::DIM),
                                ));
                            }
                        }
                    }
                    if entry.tool_name == "Read" {
                        // Read 工具：只显示行数摘要
                        if let Some(summary) = read_summary(&entry.content) {
                            lines.push(Line::from(vec![
                                Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                Span::styled(summary, Style::default().fg(theme::MUTED)),
                            ]));
                        }
                    } else if entry.tool_name == "Glob"
                        && !entry.content.is_empty()
                        && !entry.is_error
                    {
                        if let Some(summary) = glob_summary(&entry.content) {
                            lines.push(Line::from(vec![
                                Span::styled("  ⎿ ", Style::default().fg(theme::DIM)),
                                Span::styled(summary, Style::default().fg(theme::MUTED)),
                            ]));
                        }
                        for line in entry.content.lines() {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default().fg(theme::DIM)),
                                Span::styled(
                                    sanitize_display_text(line),
                                    Style::default().fg(theme::MUTED),
                                ),
                            ]));
                        }
                    } else if !entry.content.is_empty() {
                        for (i, line) in entry.content.lines().enumerate() {
                            let prefix = if i == 0 { "  ⎿ " } else { "    " };
                            lines.push(Line::from(vec![
                                Span::styled(prefix, Style::default().fg(theme::DIM)),
                                Span::styled(
                                    sanitize_display_text(line),
                                    Style::default().fg(theme::MUTED),
                                ),
                            ]));
                        }
                    }
                }
            } else {
                // 折叠态：仅显示出错工具的错误摘要（正常工具由工具栏展示，无需汇总行）
                for entry in tools {
                    if entry.is_error && !entry.content.is_empty() {
                        lines.extend(error_summary_lines(&entry.content));
                    }
                }
            }

            (lines, link_hits)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("message_render_test.rs");
}
