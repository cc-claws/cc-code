use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod decorators;
use decorators::{
    ask_user::parse_ask_user_qa_items,
    bash::{parse_exit_code, transform_bash_output_lines},
    glob::glob_summary,
    read::{format_display_path_for_memory, read_summary},
    ToolDisplayLine,
};

use super::{
    message_view::{
        AgentSummary, BackgroundTaskStarted, ContentBlockView, MessageViewModel, ToolCategory,
    },
    theme,
};
use crate::app::tool_display::sanitize_display_text;

pub(crate) const CONTROL_B_BACKGROUND_HINT: &str = "(ctrl+b to run in background)";
const BACKGROUND_RUNNING_STATUS: &str = "Running in the background (↓ to manage)";

/// 把秒数格式化为人类可读时长，供 Bash 运行状态行「已运行」与「超时期限」共用。
///
/// ≥3600s 显示小时，≥60s 显示分钟，<60s 直接写秒数。
/// 统一口径避免同一行出现「已运行 9m 20s、期限却显示 560s」这类两种风格。
fn humanize_secs(secs: u64) -> String {
    if secs >= 3600 {
        let (hours, minutes) = (secs / 3600, secs % 3600 / 60);
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else if secs >= 60 {
        let (minutes, rest) = (secs / 60, secs % 60);
        if rest == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m {rest:02}s")
        }
    } else {
        format!("{secs}s")
    }
}

/// 模型请求的 `timeout`（毫秒）→ `(timeout 9m 20s)`。
///
/// 不足 1 分钟的期限保留原有精度：整千毫秒写 `Ns`，否则写原始毫秒。
fn shell_timeout_text(timeout_ms: Option<u64>) -> Option<String> {
    timeout_ms.filter(|ms| *ms > 0).map(|ms| {
        let limit = if ms >= 60_000 {
            humanize_secs(ms / 1_000)
        } else if ms.is_multiple_of(1_000) {
            format!("{}s", ms / 1_000)
        } else {
            format!("{ms}ms")
        };
        format!("(timeout {limit})")
    })
}

pub(crate) fn shell_running_text(
    started_at: std::time::Instant,
    timeout_ms: Option<u64>,
) -> String {
    let elapsed = humanize_secs(started_at.elapsed().as_secs());
    let timeout = shell_timeout_text(timeout_ms)
        .map(|text| format!("    {text}"))
        .unwrap_or_default();
    format!("Running… ({elapsed}){timeout}")
}

/// Bash 运行状态行（`"  ⎿ Running… (Xs)"`）中状态文本的固定前缀。
pub(crate) const SHELL_RUNNING_TEXT_PREFIX: &str = "Running…";

/// 判断某渲染行是否为 Bash "⎿ Running…" 运行状态行。
///
/// 供渲染线程在增量刷新时**按内容定位**状态行。详细模式下超长命令会让 header
/// 折成多行，状态行下标不再固定为 1（issue：固定写下标 1 会覆盖命令续行）。
pub(crate) fn is_shell_running_status_line(line: &Line<'static>) -> bool {
    line.spans.len() >= 2
        && line.spans[0].content == "  ⎿ "
        && line.spans[1].content.starts_with(SHELL_RUNNING_TEXT_PREFIX)
}

/// 非详细模式下错误摘要的最大显示行数（避免长错误污染页面）
const ERROR_SUMMARY_MAX_LINES: usize = 3;

/// 非详细模式下工具 Header 命令摘要占可用宽度的比例（16:19 ≈ 84%）。
/// 命令不再一路顶到消息区最右侧才截断，避免用户需要在屏幕最右边缘阅读
/// （issue #258）。详细模式不受此限制，改为完整折行展示。
const HEADER_ARGS_WIDTH_NUM: usize = 16;
const HEADER_ARGS_WIDTH_DEN: usize = 19;

/// 计算工具 Header 参数摘要的可用显示宽度。
///
/// 非详细模式：在扣除指示器/工具名前缀后，进一步把可用宽度收缩到
/// `HEADER_ARGS_WIDTH_NUM / HEADER_ARGS_WIDTH_DEN`，并保留一个最小宽度，
/// 避免窄终端下被压得过短。
/// 详细模式：返回整段剩余宽度（上限 400），供调用方折行展示完整命令使用。
fn header_args_width(width: usize, prefix_width: usize, detail_mode: bool) -> usize {
    let avail = width.saturating_sub(prefix_width + 2);
    if detail_mode {
        avail.clamp(1, 400)
    } else {
        // 收缩到 avail 的 16/19，同时不低于 32 列、不超过 avail
        (avail * HEADER_ARGS_WIDTH_NUM / HEADER_ARGS_WIDTH_DEN)
            .max(32)
            .min(avail)
            .max(1)
    }
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
                        Style::default().fg(theme::cur().subtle)
                    };
                    Span::styled(span.content, dim_style)
                })
                .collect();
            Line::from(dimmed)
        })
        .collect()
}

/// Bash 工具 / `!` 本地命令输出行上限。折叠态 3 行，详细模式不截断。
/// 两者共用同一口径，避免同一屏上 `!` 块与 Bash 工具行可见行数不一致。
const SHELL_OUTPUT_COLLAPSED_LINES: usize = 3;
const SHELL_OUTPUT_DETAIL_LINES: usize = usize::MAX;

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
///
/// 默认 trim 首段行首空白（见 [`wrap_line_spans_rich_impl`]）。需要保留首段
/// 行首结构性空白（如工具头指示器）时改用 [`wrap_line_spans_rich_keep_first_lead`]。
fn wrap_line_spans_rich(line: Line<'static>, max_width: usize) -> Vec<WrappedLineSeg> {
    wrap_line_spans_rich_impl(line, max_width, true)
}

/// 同 [`wrap_line_spans_rich`]，但**保留首段的行首空白**。
///
/// 工具头（`● Bash(...)`）在运行中指示器闪烁时会以空白帧渲染（`●` → `" "`）。
/// 详细模式超长命令折行若 trim 首段行首空白，会让整个 header 前缀（指示器 + 分隔
/// 空格）左移甚至消失，随闪烁抖动（表现为「详细模式看不到工具名前缀」）。
/// header 的结构性前缀必须原样保留。（issue #342）
fn wrap_line_spans_rich_keep_first_lead(
    line: Line<'static>,
    max_width: usize,
) -> Vec<WrappedLineSeg> {
    wrap_line_spans_rich_impl(line, max_width, false)
}

/// [`wrap_line_spans_rich`] 的底层实现。
///
/// `trim_first_lead` 控制**首段**是否 trim 行首空白；续行（非首段）始终不额外
/// trim——断行点在推进 `pos` 时其后的空白已被跳过。
fn wrap_line_spans_rich_impl(
    line: Line<'static>,
    max_width: usize,
    trim_first_lead: bool,
) -> Vec<WrappedLineSeg> {
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
        // [TRAP] 仅当还有内容需要带到下一行时才回退：否则末段本可整段容纳，
        // 仍回退会命中段内最后一个空白，把完整末段多拆一刀（如 "PR" 孤儿行）。
        // [TRAP] 仅当断点后的词本身能放进一整行时才回退：若该词比 max_width 还长，
        // 它反正要被硬断，此时回退会把整个长词推走、留下近乎空的行
        // （如 header 首行只剩 "●"，而 "Bash(AAAA…" 被整体挤到下一行）。
        let mut break_at = content_end;
        if content_end < flat.len() {
            for i in (pos..content_end).rev() {
                if flat[i].0.chars().all(char::is_whitespace) {
                    // 量出断点后那个词（到下一个空白为止）的完整显示宽度
                    let mut word_start = i;
                    while word_start < flat.len()
                        && flat[word_start].0.chars().all(char::is_whitespace)
                    {
                        word_start += 1;
                    }
                    let word_width: usize = flat[word_start..]
                        .iter()
                        .take_while(|(g, _)| !g.chars().all(char::is_whitespace))
                        .map(|(g, _)| g.width())
                        .sum();
                    if word_width <= max_width {
                        break_at = i;
                    }
                    break;
                }
            }
        }

        // trim 行首行尾空白。
        // 行首空白仅在没有内容语义时才可丢；首段（本函数首轮循环）若被调用方
        // 标记为「保留行首空白」（工具头指示器场景），则跳过行首 trim，
        // 避免闪烁空白帧把 header 前缀整段吃掉。续行不受影响。
        let mut seg_start = pos;
        if trim_first_lead || pos > 0 {
            while seg_start < break_at && flat[seg_start].0.chars().all(char::is_whitespace) {
                seg_start += 1;
            }
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
pub(crate) fn wrap_line_spans(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    wrap_line_spans_rich(line, max_width)
        .into_iter()
        .map(|seg| seg.line)
        .collect()
}

/// 同 [`wrap_line_spans`]，但保留首段行首空白（工具头指示器场景）。
fn wrap_line_spans_keep_first_lead(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    wrap_line_spans_rich_keep_first_lead(line, max_width)
        .into_iter()
        .map(|seg| seg.line)
        .collect()
}

/// 把逻辑行上的链接命中区映射到折行后的输出段，累加前缀宽度后推入 out
fn push_link_hits_for_wrapped(
    out: &mut Vec<cc_widgets::markdown::LinkHit>,
    base_line: usize,
    line_links: &[cc_widgets::markdown::LinkHit],
    wrapped: &[WrappedLineSeg],
    prefix_g: usize,
) {
    for hit in line_links {
        for (i, seg) in wrapped.iter().enumerate() {
            let is = hit.g_start.max(seg.in_g_start);
            let ie = hit.g_end.min(seg.in_g_end);
            if is < ie {
                out.push(cc_widgets::markdown::LinkHit {
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
    links: &[cc_widgets::markdown::LinkHit],
    line_idx: usize,
) -> impl Iterator<Item = &cc_widgets::markdown::LinkHit> {
    links.iter().filter(move |h| h.line == line_idx)
}

/// 将一行（可含多 span，如 ANSI 着色）按可用宽度预折行后追加到 `out`。
///
/// 首行使用 `first_prefix`，所有续行使用 `cont_prefix`（应与 `first_prefix` 等宽，
/// 通常为等宽空格，形成悬挂缩进）。预折行保证行宽不超过视口，避免 `Paragraph::wrap`
/// 二次硬折行导致续行顶格、丢失悬挂缩进（尤其是长 JSON / 长 URL 等无空格内容）。
/// `content_width` 为扣除前缀后的可用内容宽度。折行后保留各 span 原有样式。
fn push_wrapped_line(
    out: &mut Vec<Line<'static>>,
    line: Line<'static>,
    first_prefix: &str,
    cont_prefix: &str,
    prefix_style: Style,
    content_width: usize,
) {
    for (j, wline) in wrap_line_spans(line, content_width).into_iter().enumerate() {
        let prefix = if j == 0 { first_prefix } else { cont_prefix };
        let mut spans = Vec::with_capacity(wline.spans.len() + 1);
        // 空前缀（无缩进样式的行）不插入额外 span，保持与非折行路径完全一致的输出
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix.to_string(), prefix_style));
        }
        spans.extend(wline.spans);
        out.push(Line::from(spans));
    }
}

/// 同 [`push_wrapped_line`]，但**保留首段行首空白**（工具头指示器场景）。
///
/// 工具头在运行中指示器闪烁时会以空白帧渲染（`●` → `" "`）；详细模式超长命令
/// 折行若 trim 首段行首空白，会让 `●`/工具名前缀随闪烁左移抖动（issue #342）。
fn push_wrapped_line_keep_first_lead(
    out: &mut Vec<Line<'static>>,
    line: Line<'static>,
    first_prefix: &str,
    cont_prefix: &str,
    prefix_style: Style,
    content_width: usize,
) {
    for (j, wline) in wrap_line_spans_keep_first_lead(line, content_width)
        .into_iter()
        .enumerate()
    {
        let prefix = if j == 0 { first_prefix } else { cont_prefix };
        let mut spans = Vec::with_capacity(wline.spans.len() + 1);
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix.to_string(), prefix_style));
        }
        spans.extend(wline.spans);
        out.push(Line::from(spans));
    }
}

/// 剥离 Line 的前导空白，返回 (前导空白字符串, 首个非空白 span 的样式, 剩余 Line)。
///
/// 前导空白无内容语义，但需保留其样式（如 `!` 命令块的整行背景色），故一并返回。
fn take_leading_blank(line: Line<'static>) -> (String, Style, Line<'static>) {
    use unicode_segmentation::UnicodeSegmentation;

    let mut lead = String::new();
    let mut lead_style = Style::default();
    let mut lead_style_set = false;
    let mut rest: Vec<Span<'static>> = Vec::new();
    let mut leading = true;
    for span in line.spans {
        if !leading {
            rest.push(span);
            continue;
        }
        let mut kept = String::new();
        for g in span.content.graphemes(true) {
            if leading && g.chars().all(char::is_whitespace) {
                if !lead_style_set {
                    lead_style = span.style;
                    lead_style_set = true;
                }
                lead.push_str(g);
            } else {
                leading = false;
                kept.push_str(g);
            }
        }
        if !kept.is_empty() {
            rest.push(Span::styled(kept, span.style));
        }
    }
    (lead, lead_style, Line::from(rest))
}

/// 在纯文本里识别 http(s) URL 的字节区间。
///
/// 与 CC `OutputLine.tsx::URL_IN_JSON`（`/https?:\/\/[^\s"'<>\\]+/g`）同一口径：
/// 只认 `http://` / `https://` 起头，遇到空白、引号、尖括号、反斜杠即结束。
/// 额外再切掉句末常见的 `)`/`]`/`}`/`,`/`;`/`.`/`:`，避免把中文标点旁的右括号也吞进链接
/// （终端自身识别的链接同样不会包含它们）。
///
/// 不用 `regex` crate：这里只需要一条极简前缀扫描，手写一遍省一个依赖。
fn find_url_ranges(text: &str) -> Vec<(usize, usize)> {
    const SCHEMES: [&str; 2] = ["https://", "http://"];
    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(found) = SCHEMES
        .iter()
        .filter_map(|scheme| text[search_from..].find(scheme).map(|at| (search_from + at, *scheme)))
        .min_by_key(|(at, _)| *at)
    {
        let (start, scheme) = found;
        let mut end = start + scheme.len();
        for ch in text[end..].chars() {
            if ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>' | '\\') {
                break;
            }
            end += ch.len_utf8();
        }
        // scheme 后必须真有内容（`https://` 后面跟空白不算 URL）
        let trimmed_end = text[start..end]
            .trim_end_matches([')', ']', '}', ',', ';', '.', ':'])
            .len()
            + start;
        if trimmed_end > start + scheme.len() {
            ranges.push((start, trimmed_end));
        }
        search_from = end.max(start + scheme.len());
    }
    ranges
}

/// 给一行纯文本里的 URL 套上「链接色 + 下划线」。
///
/// 说明：这不是 OSC 8 真超链接（ratatui 的 cell 模型不支持塞转义序列，
/// 且 `sanitize_display_text` 会剥掉 `\x1b`），只是视觉上把 URL 与正文区分开。
/// 点击能力仍由终端自身的 URL 识别提供。
fn style_urls(text: &str, base: Style) -> Vec<Span<'static>> {
    let ranges = find_url_ranges(text);
    if ranges.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let link = base
        .fg(theme::cur().ide)
        .add_modifier(Modifier::UNDERLINED);
    let mut spans = Vec::with_capacity(ranges.len() * 2 + 1);
    let mut cursor = 0;
    for (start, end) in ranges {
        if start > cursor {
            spans.push(Span::styled(text[cursor..start].to_string(), base));
        }
        spans.push(Span::styled(text[start..end].to_string(), link));
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(Span::styled(text[cursor..].to_string(), base));
    }
    spans
}

/// 同 [`push_wrapped_line`]，但保留原行的前导空白缩进层级并返回折行集合。
fn format_wrapped_line_keep_lead(
    line: Line<'static>,
    first_prefix: &str,
    cont_prefix: &str,
    prefix_style: Style,
    content_width: usize,
) -> Vec<Line<'static>> {
    let (lead, lead_style, rest) = take_leading_blank(line);
    let lead_width = UnicodeWidthStr::width(lead.as_str());
    let inner = content_width.saturating_sub(lead_width).max(1);
    let wrapped = wrap_line_spans(rest, inner);
    let mut out = Vec::with_capacity(wrapped.len());
    for (j, wline) in wrapped.into_iter().enumerate() {
        let prefix = if j == 0 { first_prefix } else { cont_prefix };
        let mut spans = Vec::with_capacity(wline.spans.len() + 2);
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix.to_string(), prefix_style));
        }
        if !lead.is_empty() {
            spans.push(Span::styled(lead.clone(), lead_style));
        }
        spans.extend(wline.spans);
        out.push(Line::from(spans));
    }
    out
}

/// 同 [`push_wrapped_line`]，但保留原行的前导空白缩进层级。
///
/// `wrap_line_spans_rich` 会 trim 每段行首空白，导致带缩进的长行（美化 JSON / YAML /
/// 缩进代码）折行后丢失缩进、与同级别相邻行错位。此变体先剥离前导空白，按扣除其
/// 宽度后的可用宽度折行，再把缩进拼回每个输出行的前缀之后，保持层级一致。
fn push_wrapped_line_keep_lead(
    out: &mut Vec<Line<'static>>,
    line: Line<'static>,
    first_prefix: &str,
    cont_prefix: &str,
    prefix_style: Style,
    content_width: usize,
) {
    out.extend(format_wrapped_line_keep_lead(
        line,
        first_prefix,
        cont_prefix,
        prefix_style,
        content_width,
    ));
}

/// 便捷包装：单一样式的纯文本行（先做终端转义清理）预折行，续行固定 4 列悬挂缩进，
/// 并保留原行的前导空白缩进。
/// 识别「后台子 Agent 启动摘要」的树形行（`  ├── 任务` / `  └── 任务`）。
///
/// 返回 (缩进, 分支前缀, 正文)——分支前缀含字形与虚线（如 `├── `），逐段着色；
/// 不是树形行时返回 None，由调用方的通用分支处理。
fn split_tree_row(line: &str) -> Option<(&str, &str, &str)> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let glyph = trimmed.chars().next()?;
    if glyph != '├' && glyph != '└' {
        return None;
    }
    let prefix_len = trimmed
        .find(|c: char| !matches!(c, '├' | '└' | '─' | ' '))
        .unwrap_or(trimmed.len());
    Some((indent, &trimmed[..prefix_len], &trimmed[prefix_len..]))
}

fn format_prefixed_text(
    text: &str,
    first_prefix: &str,
    prefix_style: Style,
    text_style: Style,
    content_width: usize,
) -> Vec<Line<'static>> {
    let line = Line::from(style_urls(&sanitize_display_text(text), text_style));
    format_wrapped_line_keep_lead(line, first_prefix, "    ", prefix_style, content_width)
}

/// 把 Write 的 `Wrote N lines <path>` 结果行重绘为 CC 的样式
/// `Wrote <bold>N</bold> lines to <bold>path</bold>`（`FileWriteTool/UI.tsx:58-61`）。
///
/// 只处理精确匹配的单行文本；其它输出（如 append 的 `Appended ...`）返回 None，
/// 由调用方回退为普通纯文本渲染。
fn format_write_summary_line(
    text: &str,
    text_color: ratatui::style::Color,
) -> Option<Line<'static>> {
    let rest = text.strip_prefix("Wrote ")?;
    let (count, rest) = rest.split_once(' ')?;
    count.parse::<usize>().ok()?;
    let (unit, path) = rest.split_once(' ')?;
    if (unit != "line" && unit != "lines") || path.is_empty() {
        return None;
    }
    let base = Style::default().fg(text_color);
    let bold = base.add_modifier(ratatui::style::Modifier::BOLD);
    Some(Line::from(vec![
        Span::styled("Wrote ", base),
        Span::styled(count.to_string(), bold),
        Span::styled(format!(" {unit} to "), base),
        Span::styled(path.to_string(), bold),
    ]))
}

fn push_prefixed_text(
    out: &mut Vec<Line<'static>>,
    text: &str,
    first_prefix: &str,
    prefix_style: Style,
    text_style: Style,
    content_width: usize,
) {
    out.extend(format_prefixed_text(
        text,
        first_prefix,
        prefix_style,
        text_style,
        content_width,
    ));
}

/// 按视觉行预算推入内容行，若超出预算则在最后追加展开提示行。
///
/// 保留前 `max_lines` 个视觉行。若发生截断：
/// - 若有未展示的后续逻辑行，提示 `... ({remaining} more lines) (ctrl+o to expand)`；
/// - 若所有逻辑行已进入但因单行折行超出配额被截断，提示 `... (ctrl+o to expand)`。
fn push_visual_capped_output<T, F>(
    out: &mut Vec<Line<'static>>,
    items: &[T],
    max_lines: usize,
    border_color: Color,
    mut format_line: F,
) where
    F: FnMut(&T, usize) -> Vec<Line<'static>>,
{
    let total = items.len();
    let mut rendered_visual = 0;
    for (i, item) in items.iter().enumerate() {
        let available = max_lines.saturating_sub(rendered_visual);
        if available == 0 {
            let remaining_logical = total.saturating_sub(i);
            let hint = if remaining_logical > 0 {
                format!("... ({remaining_logical} more lines) (ctrl+o to expand)")
            } else {
                "... (ctrl+o to expand)".to_string()
            };
            out.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(border_color)),
                Span::styled(hint, Style::default().fg(theme::cur().subtle)),
            ]));
            return;
        }

        let wlines = format_line(item, i);
        if wlines.len() <= available {
            rendered_visual += wlines.len();
            out.extend(wlines);
            if rendered_visual >= max_lines && i + 1 < total {
                let remaining_logical = total - (i + 1);
                let hint = format!("... ({remaining_logical} more lines) (ctrl+o to expand)");
                out.push(Line::from(vec![
                    Span::styled("    ", Style::default().fg(border_color)),
                    Span::styled(hint, Style::default().fg(theme::cur().subtle)),
                ]));
                return;
            }
        } else {
            out.extend(wlines.into_iter().take(available));
            let remaining_logical = total.saturating_sub(i + 1);
            let hint = if remaining_logical > 0 {
                format!("... ({remaining_logical} more lines) (ctrl+o to expand)")
            } else {
                "... (ctrl+o to expand)".to_string()
            };
            out.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(border_color)),
                Span::styled(hint, Style::default().fg(theme::cur().subtle)),
            ]));
            return;
        }
    }
}

/// Generate always-visible error summary lines (up to 400 Unicode chars).
/// 2-space indent, no vertical bar, no prefix. Preserves newlines (multi-line render).
/// 错误摘要渲染。
///
/// `max_lines` 控制**显示行数上限**（非详细模式传小值避免污染页面；传入 `usize::MAX` 显示完整）。
/// 超出时追加 `... (N more lines) (ctrl+o to expand)` 提示。字符级截断兜底防止超长单行。
fn error_summary_lines(content: &str, width: usize, max_lines: usize) -> Vec<Line<'static>> {
    let truncated: String = content.chars().take(400).collect();
    let content_width = width.saturating_sub(4).max(20);
    let all: Vec<&str> = truncated.lines().collect();
    let mut out = Vec::new();
    for (i, line) in all.iter().enumerate() {
        if i >= max_lines {
            let hidden = all.len() - max_lines;
            out.push(Line::from(Span::styled(
                format!("    ... ({hidden} more lines) (ctrl+o to expand)"),
                Style::default().fg(theme::cur().subtle),
            )));
            break;
        }
        let first_prefix = if i == 0 { "  ⎿ " } else { "    " };
        push_prefixed_text(
            &mut out,
            line,
            first_prefix,
            Style::default().fg(theme::cur().subtle),
            Style::default().fg(theme::cur().error),
            content_width,
        );
    }
    out
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
    let sanitized_args = if let Some(mem_path) = format_display_path_for_memory(&sanitized_args) {
        mem_path
    } else {
        sanitized_args
    };
    if tool_name == "Glob" {
        // "pattern: \"...\"" 占 11 列固定前缀/后缀宽度
        let inner_max = max_width.saturating_sub(11);
        let summary = truncate_to_display_width(&sanitized_args, inner_max);
        format!("pattern: \"{}\"", summary)
    } else {
        truncate_to_display_width(&sanitized_args, max_width)
    }
}

/// 批次汇总树形渲染：折叠态显示 header + 每行摘要，展开态显示各 agent 详情。
fn render_batch_summary(
    agents: &[AgentSummary],
    collapsed: &bool,
    width: usize,
) -> Vec<Line<'static>> {
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
        Span::styled(
            theme::SYMBOL_DOT_SPACED,
            Style::default().fg(theme::cur().success),
        ),
        Span::styled(header_text, Style::default().fg(theme::cur().text)),
    ]));

    if *collapsed {
        // 折叠态：每行 agent 摘要
        for (idx, agent) in agents.iter().enumerate() {
            let is_last = idx == total - 1;
            let connector = if is_last { "└─" } else { "├─" };
            let status = if agent.is_error {
                ("Failed", theme::cur().error)
            } else {
                ("Done", theme::cur().success)
            };

            let mut spans = vec![
                Span::styled("   ", Style::default().fg(theme::cur().subtle)),
                Span::styled(
                    connector.to_string(),
                    Style::default().fg(theme::cur().subtle),
                ),
                Span::styled(" ".to_string(), Style::default()),
                Span::styled(
                    agent.task_preview.clone(),
                    Style::default().fg(theme::cur().text),
                ),
            ];

            if agent.tool_count > 0 {
                spans.push(Span::styled(
                    format!(
                        " · {}",
                        crate::i18n::global().tr_args(
                            "subagent-tool-uses",
                            &[("count".into(), agent.tool_count.to_string().into())],
                        )
                    ),
                    Style::default().fg(theme::cur().subtle),
                ));
            }

            spans.push(Span::styled(
                " · ",
                Style::default().fg(theme::cur().subtle),
            ));
            spans.push(Span::styled(
                status.0.to_string(),
                Style::default().fg(status.1),
            ));

            push_wrapped_line(
                &mut lines,
                Line::from(spans),
                "",
                "      ",
                Style::default().fg(theme::cur().subtle),
                width.saturating_sub(6).max(20),
            );
        }
    } else {
        // 展开态：每个 agent 显示 task_preview + final_result
        for (idx, agent) in agents.iter().enumerate() {
            let is_last = idx == total - 1;
            let connector = if is_last { "└─" } else { "├─" };

            // task_preview 行（首行前缀 "   " + connector(2) + " " = 6 列）
            push_wrapped_line(
                &mut lines,
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        connector.to_string(),
                        Style::default().fg(theme::cur().subtle),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        agent.task_preview.clone(),
                        Style::default().fg(theme::cur().text),
                    ),
                ]),
                "",
                "      ",
                Style::default().fg(theme::cur().subtle),
                width.saturating_sub(6).max(20),
            );

            // final_result 行（首行前缀 "     ⎿ " = 7 列，续行对齐 7 列）
            if let Some(ref result) = agent.final_result {
                if !result.is_empty() {
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        Line::from(Span::styled(
                            sanitize_display_text(result),
                            Style::default().fg(theme::cur().inactive),
                        )),
                        "     ⎿ ",
                        "       ",
                        Style::default().fg(theme::cur().subtle),
                        width.saturating_sub(7).max(20),
                    );
                }
            }
        }
    }

    lines
}

/// AskUserQuestion 专用渲染：`● User answered CC Code's questions:` + `⎿ · H → V`
fn render_ask_user_block(content: &str, is_error: bool, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let color = if is_error {
        theme::cur().error
    } else {
        theme::cur().success
    };
    lines.push(Line::from(vec![
        Span::styled(theme::SYMBOL_DOT_SPACED, Style::default().fg(color)),
        Span::styled(
            "User answered CC Code's questions:".to_string(),
            Style::default().fg(theme::cur().text),
        ),
    ]));

    if content.is_empty() {
        return lines;
    }

    for text in parse_ask_user_qa_items(content) {
        push_wrapped_line_keep_lead(
            &mut lines,
            Line::from(Span::styled(
                text,
                Style::default().fg(if is_error {
                    theme::cur().error
                } else {
                    theme::cur().inactive
                }),
            )),
            "  ⎿ ",
            "    ",
            Style::default().fg(theme::cur().subtle),
            width.saturating_sub(4).max(20),
        );
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

/// 渲染一行 `!` 命令输出（保留 ANSI 着色）并按视口宽度预折行。
///
/// 首行用 `prefix`、续行用 4 列空格，形成悬挂缩进；不设背景色，与工具结果行
/// （`⎿` 前缀 + DIM/ERROR 前景色）保持一致。
/// 预折行避免 `Paragraph::wrap` 二次硬折行使长输出续行顶格。
fn format_shell_output_lines(
    prefix: &str,
    text: &str,
    default_style: Style,
    prefix_color: Color,
    width: usize,
) -> Vec<Line<'static>> {
    let prefix_style = Style::default().fg(prefix_color);
    // 先按 ANSI 切段（保留命令自带着色），再把每段里的 URL 单独挑出来上链接色：
    // 用户 `!gh pr checks` 这类命令的输出主要是 Git 分支、URL，链接色能明显提速扫读。
    let content: Vec<Span<'static>> = ansi_spans(text, default_style)
        .into_iter()
        .flat_map(|span| style_urls(&span.content, span.style))
        .collect();
    format_wrapped_line_keep_lead(
        Line::from(content),
        prefix,
        "    ",
        prefix_style,
        width.saturating_sub(4).max(20),
    )
}

fn shell_output_lines(
    out: &mut Vec<Line<'static>>,
    prefix: &'static str,
    text: &str,
    default_style: Style,
    prefix_color: Color,
    width: usize,
) {
    out.extend(format_shell_output_lines(
        prefix,
        text,
        default_style,
        prefix_color,
        width,
    ));
}

/// 渲染用户 `!` 本机命令的标题行。
///
/// 标题行使用整行背景和粉色 `!` 标记，保持与 CC 的本地命令块一致。
fn shell_command_header(command: &str, width: usize) -> Line<'static> {
    let command =
        truncate_to_display_width(&sanitize_display_text(command), width.saturating_sub(2));
    let header_bg = Style::default().bg(theme::cur().user_message_background);
    let mut spans = vec![
        Span::styled("! ", header_bg.fg(theme::cur().bash_border)),
        Span::styled(command.clone(), header_bg.fg(theme::cur().text)),
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
        shell_output_lines(
            &mut lines,
            "  ⎿ ",
            text,
            Style::default().fg(theme::cur().subtle),
            theme::cur().subtle,
            width,
        );
    } else {
        let max_lines = if detail_mode {
            SHELL_OUTPUT_DETAIL_LINES
        } else {
            SHELL_OUTPUT_COLLAPSED_LINES
        };
        push_visual_capped_output(
            &mut lines,
            &output_lines,
            max_lines,
            theme::cur().subtle,
            |(line, is_error), idx| {
                // 与 Bash 工具结果行口径一致：成功输出用 TEXT_SOFT，非零退出码用 ERROR。
                let default_style = if *is_error && exit_code != Some(0) {
                    Style::default().fg(theme::cur().error)
                } else {
                    Style::default().fg(theme::cur().text_soft)
                };
                // 与 Bash 工具结果行口径一致：非零退出码时前缀转 ERROR 红。
                let prefix_color = if *is_error && exit_code != Some(0) {
                    theme::cur().error
                } else {
                    theme::cur().subtle
                };
                let prefix = if idx == 0 { "  ⎿ " } else { "    " };
                format_shell_output_lines(prefix, line, default_style, prefix_color, width)
            },
        );
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
        shell_output_lines(
            &mut lines,
            "    ",
            &elapsed_str,
            Style::default().fg(theme::cur().inactive),
            theme::cur().subtle,
            width,
        );
        shell_output_lines(
            &mut lines,
            "    ",
            CONTROL_B_BACKGROUND_HINT,
            Style::default().fg(theme::cur().inactive),
            theme::cur().subtle,
            width,
        );
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
    index: Option<usize>,
    width: usize,
    detail_mode: bool,
    tick: u64,
) -> (Vec<Line<'static>>, Vec<cc_widgets::markdown::LinkHit>) {
    render_view_model_with_flags(vm, index, width, detail_mode, false, tick)
}

/// 带完整渲染标志的入口。
///
/// 两个标志语义不同，必须分开传：
/// - `detail_mode`（Ctrl+O 详细模式）：控制 shell 输出行数、粘贴内容、reasoning、
///   工具折叠状态等**整页展开**行为；
/// - `diff_visible`（配置 `diff_enabled`）：只控制**普通页面**是否内联显示 Write/Edit
///   的 diff 块，不影响上面任何一项（详细模式下 diff 恒可见）。
pub fn render_view_model_with_flags(
    vm: &MessageViewModel,
    _index: Option<usize>,
    width: usize,
    detail_mode: bool,
    diff_visible: bool,
    tick: u64,
) -> (Vec<Line<'static>>, Vec<cc_widgets::markdown::LinkHit>) {
    let mut link_hits: Vec<cc_widgets::markdown::LinkHit> = Vec::new();
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
                    crate::i18n::global().tr("compact-context-summarized"),
                    Style::default()
                        .fg(theme::cur().subtle)
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
            let user_bg: Color = theme::cur().user_message_background;
            // [TRAP] 同 AssistantBubble Text 路径：markdown 普通段落不预折行，
            // 需先按 content_width 预折行，防止 Paragraph::wrap 二次硬折行后续行丢掉 "  " 悬挂缩进
            let content_width = width.saturating_sub(2).max(20);
            let mut lines = Vec::with_capacity(effective_rendered.lines.len() + 1);
            for (i, line) in effective_rendered.lines.iter().enumerate() {
                let wrapped = wrap_line_spans_rich(line.clone(), content_width);
                // 前缀 "❯ " / "  " 均为 2 grapheme
                let line_links: Vec<cc_widgets::markdown::LinkHit> =
                    links_on_line(&effective_links, i).cloned().collect();
                push_link_hits_for_wrapped(&mut link_hits, lines.len(), &line_links, &wrapped, 2);
                for wline in wrapped.into_iter().map(|seg| seg.line) {
                    if i == 0 && lines.is_empty() {
                        // 第一行：用户消息用 ❯ 前缀，带底色
                        let mut spans = vec![Span::styled(
                            "❯ ",
                            Style::default()
                                .fg(theme::cur().claude)
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
                        let is_diff = cc_widgets::message_block::highlight::is_diff_content(raw);
                        if is_diff {
                            let widget_theme = theme::widgets_theme();
                            for l in raw.lines() {
                                let diff_spans =
                                    cc_widgets::message_block::highlight::highlight_diff_line(
                                        l,
                                        &widget_theme,
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
                                let line_links: Vec<cc_widgets::markdown::LinkHit> =
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
                                        theme::SYMBOL_DOT_SPACED
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
                        duration_ms,
                        action_summary,
                        text,
                        ..
                    } => {
                        // Thought 标题：缩进 2 列对齐
                        let hint = if detail_mode {
                            ""
                        } else {
                            " (ctrl+o to expand)"
                        };
                        // 有耗时 → `Thought for Ns`；无耗时（流式中/历史旧数据）→ 回退字数
                        let timing = match duration_ms {
                            Some(ms) => format!("Thought for {}s", ms.div_ceil(1000).max(1)),
                            None => format!("Thought for {char_count} chars"),
                        };
                        // 紧随的只读工具计数（如 `read 1 file, listed 1 directory`）
                        let actions = action_summary
                            .as_deref()
                            .map(|s| format!(", {s}"))
                            .unwrap_or_default();
                        let title = format!("  {timing}{actions}{hint}");
                        lines.push(Line::from(vec![Span::styled(
                            title,
                            // 与回合结束总结行（✻ … · done HH:MM）同色 MUTED
                            Style::default().fg(theme::cur().inactive),
                        )]));
                        // 非详细模式：仅显示摘要行，**不渲染**思考内容预览
                        // （历史上曾显示 tail_lines 尾部 3 行预览，流式期间每次
                        //  chunk 都重算重绘、行数跳动，已移除；展开请按 Ctrl+O）
                        // 详细模式：显示完整 reasoning，走 markdown 解析 + DIM overlay
                        let content = if detail_mode {
                            Some(text.as_str())
                        } else {
                            None
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
                                    let mut spans = vec![Span::styled(
                                        prefix,
                                        Style::default().fg(theme::cur().subtle),
                                    )];
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
            timeout_ms,
            shell_backgrounded,
            ..
        } => {
            // AskUserQuestion 专用渲染路径
            if tool_name == "AskUserQuestion" {
                return (render_ask_user_block(content, *is_error, width), link_hits);
            }

            let is_running = content.is_empty() && !*is_error;
            let background_task = if tool_name == "Bash" && !*is_error {
                BackgroundTaskStarted::parse(content)
            } else {
                None
            };

            // 构建状态（仅用于 header/collapse 管理）
            // Bash 工具：从输出中解析 exit code，非零则标记为 Failed
            let bash_failed = tool_name == "Bash"
                && background_task.is_none()
                && !*is_error
                && !is_running
                && parse_exit_code(content).is_some_and(|c| c != 0);
            let status = if *is_error || bash_failed {
                cc_widgets::ToolCallStatus::Failed
            } else if is_running {
                cc_widgets::ToolCallStatus::Running
            } else {
                cc_widgets::ToolCallStatus::Completed
            };

            // 详细模式：强制展开所有工具；否则 Write/Edit 完成后默认展开
            // 非详细模式：已完成且无错的 Bash 也「展开」（仅显示前 3 行摘要，见下方 max_lines）
            let effective_collapsed = if detail_mode {
                // Read 始终折叠：内容是文件原文，无需在 detail mode 展开
                tool_name == "Read"
            } else if !is_running && (tool_name == "Write" || tool_name == "Edit") {
                false
            } else if tool_name == "Bash" && !is_running && !content.is_empty() {
                // 非详细 Bash：有输出即展开为摘要（前 3 行，见下方 max_lines）；
                // 单行输出同样要显示（P2-6：原 `content.contains('\n')` 会吞掉单行结果）
                false
            } else {
                *collapsed
            };
            let mut state = cc_widgets::ToolCallState::new(display_name.clone(), theme::cur().text);
            state.status = status.clone();
            state.collapsed = effective_collapsed;
            state.is_error = *is_error || bash_failed;
            if let Some(args) = args_display {
                state.args_summary = args.clone();
            }

            // 复用 widget 层指示器：统一 ● 圆点 + 颜色语义化
            let (indicator, indicator_color) = cc_widgets::tool_call::display::format_indicator(
                status,
                tick,
                theme::cur().inactive,
                theme::cur().success,
                theme::cur().error,
            );
            // 工具调用已交回，但进程结果未知；不要用成功绿色暗示命令已完成。
            let indicator_color = if background_task.is_some() {
                theme::cur().cyan
            } else {
                indicator_color
            };

            // 工具名配色统一为中性色：成败与运行状态语义全部由 ● 指示器承载，
            // 工具名字母不随状态变色（此前 Bash 恒定灰、其他工具运行中青、报错红，
            // 规则不一致且同一行出现多种颜色，易误读）。
            let name_style = Style::default().fg(theme::cur().text_soft);

            let mut header_spans = vec![
                Span::styled(indicator.to_string(), Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(state.tool_name.clone(), name_style),
            ];
            let args = &state.args_summary;
            let mut lines = Vec::new();
            if !args.is_empty() {
                let name_width = UnicodeWidthStr::width(indicator)
                    + 1
                    + UnicodeWidthStr::width(state.tool_name.as_str());
                let args_color = Style::default().fg(theme::cur().text_soft);
                // 详细模式且非 Glob：完整展示命令摘要，能放下则与 header 同行，
                // 超出时折行且续行缩进到首行命令起始列（issue #258）；
                // Glob 保留带 "pattern:" 前缀的单行摘要路径。
                let wrap_full = detail_mode && tool_name != "Glob";
                if wrap_full {
                    let full = sanitize_display_text(args);
                    let indent = " ".repeat(name_width + 1);
                    let mut spans = header_spans.clone();
                    spans.push(Span::raw("("));
                    spans.push(Span::styled(full, args_color));
                    spans.push(Span::raw(")"));
                    let content_width = width.saturating_sub(name_width + 1).max(20);
                    // 用 keep_first_lead 变体：运行中指示器闪烁到空白帧时，header 首段
                    // 的行首空白（指示器占位）不能被 trim，否则整个前缀随闪烁左移抖动
                    // （issue #342：详细模式看不到 `● Bash(` 前缀）。
                    push_wrapped_line_keep_first_lead(
                        &mut lines,
                        Line::from(spans),
                        "",
                        &indent,
                        Style::default(),
                        content_width,
                    );
                } else {
                    let max_args_width = header_args_width(width, name_width + 2, detail_mode);
                    let summary = tool_args_header(tool_name, args, max_args_width);
                    header_spans.push(Span::styled(format!("({})", summary), args_color));
                    lines.push(Line::from(header_spans));
                }
            } else {
                lines.push(Line::from(header_spans));
            }
            if let Some(task) = background_task {
                let summary = if *shell_backgrounded {
                    BACKGROUND_RUNNING_STATUS.to_string()
                } else {
                    crate::i18n::global().tr_args(
                        "bg-moved-to-background",
                        &[("task".into(), task.task_id.into())],
                    )
                };
                lines.push(Line::from(vec![
                    Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                    Span::styled(
                        truncate_to_display_width(&summary, width.saturating_sub(4)),
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if *shell_backgrounded {
                    if let Some(timeout) = shell_timeout_text(*timeout_ms) {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(timeout, Style::default().fg(theme::cur().inactive)),
                        ]));
                    }
                }
                if detail_mode {
                    let output = crate::i18n::global().tr_args(
                        "bg-output-file",
                        &[("path".into(), sanitize_display_text(task.output).into())],
                    );
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            truncate_to_display_width(&output, width.saturating_sub(4)),
                            Style::default().fg(theme::cur().subtle),
                        ),
                    ]));
                }
                // 实时、历史重建和子 Agent 展开都共用此渲染入口。
                // 不改 content，因此模型仍能使用完整的任务 ID 和输出路径。
                return (lines, link_hits);
            }
            let result_lines: Vec<&str> = if content.is_empty() {
                Vec::new()
            } else {
                content.split('\n').collect()
            };
            // 对齐 CC FileEditToolUpdatedMessage：Edit/Write 更新类 diff 渲染时，
            // 工具结果文本（"Replaced text ..." / "Wrote N lines ..."）不再单独成行，
            // 由 diff 摘要行（Added N lines, removed M lines）替代。
            // 新建文件（Write create）仍保留工具结果行，对齐 CC 的 "Wrote N lines to <path>"。
            let diff_will_render = (detail_mode || diff_visible)
                && diff_input.as_ref().is_some_and(|d| !d.is_new_file);
            if !state.collapsed && !result_lines.is_empty() && !diff_will_render {
                // 口径统一：使用 state.is_error（含 Bash 非零 exit code 的 bash_failed），
                // 与 header 指示器/状态保持一致；仅用 *is_error 会让失败 Bash 的输出行仍为灰白。
                let result_color = if state.is_error {
                    theme::cur().error
                } else {
                    theme::cur().text_soft
                };
                let border_color = if state.is_error {
                    theme::cur().error
                } else {
                    theme::cur().subtle
                };
                // 详细模式显示完整内容；非详细模式：Bash 只显示前 3 行摘要（PRD §2.4），其余工具 20 行
                let max_lines = if detail_mode {
                    usize::MAX
                } else if tool_name == "Bash" {
                    3
                } else {
                    20
                };
                if tool_name == "Glob" && !*is_error {
                    if let Some(summary) = glob_summary(content) {
                        lines.push(Line::from(vec![
                            Span::styled("  ⎿ ", Style::default().fg(border_color)),
                            Span::styled(summary, Style::default().fg(result_color)),
                        ]));
                    }
                }
                let mut error_header = String::new();
                let bash_display_lines: Option<Vec<ToolDisplayLine>> = if tool_name == "Bash" {
                    Some(transform_bash_output_lines(
                        content,
                        parse_exit_code(content),
                        &mut error_header,
                    ))
                } else {
                    None
                };

                let content_width = width.saturating_sub(4).max(20);
                if let Some(ref b_lines) = bash_display_lines {
                    push_visual_capped_output(
                        &mut lines,
                        b_lines,
                        max_lines,
                        border_color,
                        |item, _i| {
                            let text_color = if item.is_error_header {
                                theme::cur().error
                            } else {
                                result_color
                            };
                            format_prefixed_text(
                                item.text,
                                item.prefix,
                                Style::default().fg(border_color),
                                Style::default().fg(text_color),
                                content_width,
                            )
                        },
                    );
                } else {
                    push_visual_capped_output(
                        &mut lines,
                        &result_lines,
                        max_lines,
                        border_color,
                        |line, i| {
                            let first_prefix = if i == 0 && tool_name != "Glob" {
                                "  ⎿ "
                            } else {
                                "    "
                            };
                            // 对齐 CC：Write create 的结果行为 "Wrote N lines to <path>"，
                            // 数字与路径加粗（FileWriteTool/UI.tsx:58-61）
                            if tool_name == "Write" {
                                if let Some(styled) = format_write_summary_line(line, result_color)
                                {
                                    return format_wrapped_line_keep_lead(
                                        styled,
                                        first_prefix,
                                        "    ",
                                        Style::default().fg(border_color),
                                        content_width,
                                    );
                                }
                            }
                            format_prefixed_text(
                                line,
                                first_prefix,
                                Style::default().fg(border_color),
                                Style::default().fg(result_color),
                                content_width,
                            )
                        },
                    );
                }
            } else if *is_error && !content.is_empty() {
                lines.extend(error_summary_lines(
                    content,
                    width,
                    if detail_mode {
                        usize::MAX
                    } else {
                        ERROR_SUMMARY_MAX_LINES
                    },
                ));
            }
            // Read 工具折叠态：显示行数或记忆召回摘要
            if state.collapsed && tool_name == "Read" && !result_lines.is_empty() {
                let is_mem = format_display_path_for_memory(&state.args_summary).is_some();
                if let Some(summary) = read_summary(content, is_mem) {
                    lines.push(Line::from(vec![
                        Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                        Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                    ]));
                }
            }
            if tool_name == "Bash" && *shell_backgrounded && started_at.is_some() {
                lines.push(Line::from(vec![
                    Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                    Span::styled(
                        BACKGROUND_RUNNING_STATUS,
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if let Some(timeout) = shell_timeout_text(*timeout_ms) {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(timeout, Style::default().fg(theme::cur().inactive)),
                    ]));
                }
            } else if tool_name == "Bash"
                && is_running
                && started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2))
            {
                lines.push(Line::from(vec![
                    Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                    Span::styled(
                        shell_running_text(started_at.unwrap(), *timeout_ms),
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default().fg(theme::cur().subtle)),
                    Span::styled(
                        CONTROL_B_BACKGROUND_HINT,
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
            }
            // 普通页面是否内联展示 diff 由 `diff_visible`（配置 `diff_enabled`）决定，
            // 详细模式（Ctrl+O）下恒展示——这是该标志**唯一**的消费点，
            // 不得把它并进 detail_mode，否则会顺带展开 shell 输出 / reasoning / 工具折叠。
            if detail_mode || diff_visible {
                if let Some(ref diff_input) = diff_input {
                    // 前缀 "  ⎿ " / "    " 占 4 列，diff 内容宽度需减去前缀
                    let diff_width = width.saturating_sub(4);
                    let widget_theme = theme::widgets_theme();
                    let prefix_style = Style::default().fg(if *is_error {
                        theme::cur().error
                    } else {
                        theme::cur().subtle
                    });
                    if diff_input.is_new_file {
                        // 对齐 CC FileWriteTool 的 create 分支：不渲染 diff，
                        // 显示无底色的语法高亮代码；非详细模式截断前 10 行。
                        let max_code_lines = if detail_mode { usize::MAX } else { 10 };
                        let rendered = cc_widgets::diff::render_new_file_code(
                            &diff_input.file_path,
                            &diff_input.new_content,
                            diff_width,
                            &widget_theme,
                            max_code_lines,
                        );
                        for line in rendered {
                            let mut prefixed = line;
                            prefixed.spans.insert(0, Span::styled("    ", prefix_style));
                            lines.push(prefixed);
                        }
                    } else {
                        let mut rendered =
                            cc_widgets::diff::render_diff(diff_input, diff_width, &widget_theme);
                        // 去掉 diff 标题行（file_path），header 的 args_display 已经显示了文件路径
                        if !rendered.is_empty() {
                            rendered.remove(0);
                        }
                        for (i, line) in rendered.iter().enumerate() {
                            let mut prefixed = line.clone();
                            let prefix = if i == 0 { "  ⎿ " } else { "    " };
                            prefixed.spans.insert(0, Span::styled(prefix, prefix_style));
                            lines.push(prefixed);
                        }
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
        } if !batch_agents.is_empty() => (
            render_batch_summary(batch_agents, collapsed, width),
            link_hits,
        ),
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

            // 状态指示器颜色（对齐 Claude Hub）。
            // 完成但报错的子 Agent 必须显示 `✗` + 红色：否则会出现
            // `✓ Agent(...)` 紧跟 `⎿ Error: …` 的自相矛盾画面（真机症状）。
            let indicator_icon = if *is_running {
                "◐"
            } else if *is_error {
                "✗"
            } else {
                "✓"
            };
            let indicator_color = if *is_running {
                theme::cur().yellow
            } else if *is_error {
                theme::cur().error
            } else {
                theme::cur().success
            };
            let agent_name_color = if *is_error {
                theme::cur().error
            } else {
                theme::cur().magenta
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
                    Span::styled(
                        format!("({})", agent_id),
                        Style::default().fg(theme::cur().inactive),
                    ),
                ];
                // 折叠状态显示短 hash
                if let Some(ref hash) = bg_hash {
                    header_spans.push(Span::styled(
                        format!(" #{}", hash),
                        Style::default().fg(theme::cur().inactive),
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
                    Style::default().fg(theme::cur().inactive),
                )]));
                if *is_error {
                    if let Some(ref result) = final_result {
                        if !result.is_empty() {
                            lines.extend(error_summary_lines(
                                result,
                                width,
                                if detail_mode {
                                    usize::MAX
                                } else {
                                    ERROR_SUMMARY_MAX_LINES
                                },
                            ));
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
                    Span::styled(
                        format!("({})", agent_id),
                        Style::default().fg(theme::cur().inactive),
                    ),
                ];
                // 展开状态显示短 hash
                if let Some(ref hash) = bg_hash {
                    header_spans.push(Span::styled(
                        format!(" #{}", hash),
                        Style::default().fg(theme::cur().inactive),
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
                    Style::default().fg(theme::cur().inactive),
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
                    // 外层会再补 2 列缩进，这里传入扣除 2 列后的宽度，
                    // 保证嵌套渲染的行宽（含外层缩进）不超过视口
                    let inner_lines = render_view_model_with_flags(
                        inner_vm,
                        None,
                        width.saturating_sub(2),
                        detail_mode,
                        diff_visible,
                        tick,
                    )
                    .0;
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

                // 子 agent 完成后，渲染 final_result 摘要（仅第一行）。按显示宽度截断
                if let Some(ref result) = final_result {
                    if !result.is_empty() {
                        if let Some(first_line) = result.lines().next() {
                            if !first_line.is_empty() {
                                push_prefixed_text(
                                    &mut lines,
                                    first_line,
                                    "  ⎿ ",
                                    Style::default().fg(theme::cur().subtle),
                                    Style::default().fg(theme::cur().inactive),
                                    width.saturating_sub(4).max(20),
                                );
                            }
                        }
                    }
                }
            }

            (lines, link_hits)
        }
        MessageViewModel::SystemNote {
            content,
            detail_content,
            ..
        } => {
            let active_content = if detail_mode && detail_content.is_some() {
                detail_content.as_deref().unwrap()
            } else {
                content.as_str()
            };
            let mut lines = Vec::new();
            for line in active_content.lines() {
                if line.is_empty() {
                    lines.push(Line::default());
                    continue;
                }
                if line.starts_with("  └ Prompt:") {
                    // 详细模式下高亮绿色的 Prompt 标签（对齐用户参考图方案 B）
                    lines.push(Line::from(vec![
                        Span::styled("  └ ", Style::default().fg(theme::cur().subtle)),
                        Span::styled(
                            "Prompt:",
                            Style::default()
                                .fg(theme::cur().success)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                } else if let Some(rest) = line.strip_prefix("  └ ") {
                    lines.push(Line::from(vec![
                        Span::styled("  └ ", Style::default().fg(theme::cur().subtle)),
                        Span::styled(
                            sanitize_display_text(rest),
                            Style::default().fg(theme::cur().inactive),
                        ),
                    ]));
                } else if let Some(text) = line.strip_prefix("      ") {
                    // Prompt 详细正文行（缩进 6 列）
                    let row = Line::from(Span::styled(
                        sanitize_display_text(text),
                        Style::default().fg(theme::cur().text_soft),
                    ));
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        row,
                        "      ",
                        "      ",
                        Style::default().fg(theme::cur().subtle),
                        width.saturating_sub(6).max(20),
                    );
                } else if let Some(rest) = line
                    .strip_prefix("● Agent(")
                    .or_else(|| line.strip_prefix("• Agent("))
                {
                    // 详细模式卡片标题：● Agent(任务预览)（对齐系统 Bash 工具统一 ● 圆点符号）
                    let rest_args = rest.strip_suffix(')').unwrap_or(rest);
                    lines.push(Line::from(vec![
                        Span::styled(
                            theme::SYMBOL_DOT_SPACED,
                            Style::default().fg(theme::cur().success),
                        ),
                        Span::styled(
                            "Agent",
                            Style::default()
                                .fg(theme::cur().text)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("({})", sanitize_display_text(rest_args)),
                            Style::default().fg(theme::cur().text_soft),
                        ),
                    ]));
                } else if line.starts_with('✻') {
                    let l = Line::from(Span::styled(
                        sanitize_display_text(line),
                        Style::default().fg(theme::cur().subtle),
                    ));
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        l,
                        "",
                        "  ",
                        Style::default().fg(theme::cur().subtle),
                        width.saturating_sub(2).max(20),
                    );
                } else if line.starts_with('⎿') {
                    let l = Line::from(Span::styled(
                        sanitize_display_text(line),
                        Style::default().fg(theme::cur().inactive),
                    ));
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        l,
                        "",
                        "  ",
                        Style::default().fg(theme::cur().inactive),
                        width.saturating_sub(2).max(20),
                    );
                } else if let Some((indent, branch, text)) = split_tree_row(line) {
                    // 后台子 Agent 启动摘要的树形行：保留自身缩进与 `├──` 分支、**不再注入
                    // `· ` 前缀**，让字形与标题文字同列（对齐 docs/subagent-mock.html）。
                    let row = Line::from(vec![
                        Span::styled(indent.to_string(), Style::default().fg(theme::cur().subtle)),
                        Span::styled(branch.to_string(), Style::default().fg(theme::cur().subtle)),
                        Span::styled(
                            sanitize_display_text(text),
                            Style::default().fg(theme::cur().text_soft),
                        ),
                    ]);
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        row,
                        "",
                        "    ",
                        Style::default().fg(theme::cur().subtle),
                        width.saturating_sub(4).max(20),
                    );
                } else if let Some(rest) = line
                    .strip_prefix(theme::SYMBOL_DOT)
                    .or_else(|| line.strip_prefix('•'))
                {
                    // 行内已自带圆点（启动摘要标题 / 子 Agent 完成行）：直接渲染全局统一圆点 SYMBOL_DOT_SPACED，
                    // 统一对齐系统 Bash 工具的 ● 字形，杜绝一大一小。
                    let row = Line::from(vec![
                        Span::styled(
                            theme::SYMBOL_DOT_SPACED,
                            Style::default().fg(theme::cur().success),
                        ),
                        Span::styled(
                            sanitize_display_text(rest.trim_start()),
                            Style::default().fg(theme::cur().text_soft),
                        ),
                    ]);
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        row,
                        "",
                        "  ",
                        Style::default().fg(theme::cur().subtle),
                        width.saturating_sub(2).max(20),
                    );
                } else {
                    let is_error =
                        line.contains("❌") || line.contains("失败") || line.contains("错误");
                    let is_warn = line.contains("⚠") || line.contains("已中断");
                    let text_color = if is_error {
                        theme::cur().error
                    } else if is_warn {
                        theme::cur().warning
                    } else {
                        theme::cur().inactive
                    };
                    push_prefixed_text(
                        &mut lines,
                        line,
                        "· ",
                        Style::default().fg(theme::cur().subtle),
                        Style::default().fg(text_color),
                        width.saturating_sub(4).max(20),
                    );
                }
            }
            (lines, link_hits)
        }
        MessageViewModel::CacheWarning { content, .. } => {
            let mut lines = Vec::new();
            for line in content.lines() {
                let l = Line::from(Span::styled(
                    sanitize_display_text(line),
                    Style::default().fg(theme::cur().warning),
                ));
                push_wrapped_line_keep_lead(
                    &mut lines,
                    l,
                    "",
                    "  ",
                    Style::default().fg(theme::cur().warning),
                    width.saturating_sub(2).max(20),
                );
            }
            (lines, link_hits)
        }
        MessageViewModel::ToolCallGroup {
            category,
            tools,
            collapsed: _collapsed,
            standalone_action,
            ..
        } => {
            let mut lines = Vec::new();

            if *category == ToolCategory::AskUser {
                // AskUserQuestion 聚合：统一标题 + 所有问答对
                let has_error = tools.iter().any(|t| t.is_error);
                let color = if has_error {
                    theme::cur().error
                } else {
                    theme::cur().success
                };
                lines.push(Line::from(vec![
                    Span::styled(theme::SYMBOL_DOT_SPACED, Style::default().fg(color)),
                    Span::styled(
                        "User answered CC Code's questions:".to_string(),
                        Style::default().fg(theme::cur().text),
                    ),
                ]));

                for entry in tools {
                    let entry_color = if entry.is_error {
                        theme::cur().error
                    } else {
                        theme::cur().inactive
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
                        push_wrapped_line_keep_lead(
                            &mut lines,
                            Line::from(Span::styled(text, Style::default().fg(entry_color))),
                            "  ⎿ ",
                            "    ",
                            Style::default().fg(theme::cur().subtle),
                            width.saturating_sub(4).max(20),
                        );
                    }
                }
            } else if detail_mode {
                // 详细模式：显示每条工具的名称和结果
                // Read 工具只显示行数摘要，不展开文件内容
                for entry in tools {
                    let entry_color = if entry.is_error {
                        theme::cur().error
                    } else {
                        theme::cur().success
                    };
                    // 与主路径（单条 ToolBlock）统一：状态只由 ● 表达，工具名恒为中性色
                    // （此前失败用 ✗、名字为白色粗体，与主路径两套体系）
                    let indicator = theme::SYMBOL_DOT;
                    lines.push(Line::from(vec![
                        Span::styled(indicator.to_string(), Style::default().fg(entry_color)),
                        Span::raw(" "),
                        Span::styled(
                            entry.display_name.clone(),
                            Style::default().fg(theme::cur().text_soft),
                        ),
                    ]));
                    if let Some(args) = &entry.args_display {
                        if !args.is_empty() {
                            let prefix_width = UnicodeWidthStr::width(indicator)
                                + 1
                                + UnicodeWidthStr::width(entry.display_name.as_str())
                                + 2;
                            let max_args_width =
                                header_args_width(width, prefix_width + 2, detail_mode);
                            let summary = tool_args_header(&entry.tool_name, args, max_args_width);
                            if let Some(last_line) = lines.last_mut() {
                                last_line.spans.push(Span::styled(
                                    format!("({})", summary),
                                    Style::default().fg(theme::cur().text_soft),
                                ));
                            }
                        }
                    }
                    if entry.tool_name == "Read" {
                        // Read 工具：显示行数或记忆召回摘要
                        let is_mem = entry
                            .args_display
                            .as_deref()
                            .and_then(format_display_path_for_memory)
                            .is_some();
                        if let Some(summary) = read_summary(&entry.content, is_mem) {
                            lines.push(Line::from(vec![
                                Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                                Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                            ]));
                        }
                    } else if entry.tool_name == "Glob"
                        && !entry.content.is_empty()
                        && !entry.is_error
                    {
                        if let Some(summary) = glob_summary(&entry.content) {
                            lines.push(Line::from(vec![
                                Span::styled("  ⎿ ", Style::default().fg(theme::cur().subtle)),
                                Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                            ]));
                        }
                        for line in entry.content.lines() {
                            push_prefixed_text(
                                &mut lines,
                                line,
                                "    ",
                                Style::default().fg(theme::cur().subtle),
                                Style::default().fg(theme::cur().inactive),
                                width.saturating_sub(4).max(20),
                            );
                        }
                    } else if !entry.content.is_empty() {
                        for (i, line) in entry.content.lines().enumerate() {
                            let first_prefix = if i == 0 { "  ⎿ " } else { "    " };
                            push_prefixed_text(
                                &mut lines,
                                line,
                                first_prefix,
                                Style::default().fg(theme::cur().subtle),
                                Style::default().fg(theme::cur().inactive),
                                width.saturating_sub(4).max(20),
                            );
                        }
                    }
                }
            } else {
                // 折叠态：显示**纯动作行**（PRD §2.3，仅前面无 thinking bubble 的独立只读组）
                // 以及出错工具的错误摘要。
                if let Some(summary) = standalone_action {
                    lines.push(Line::from(Span::styled(
                        format!("  {summary}"),
                        Style::default().fg(theme::cur().inactive),
                    )));
                }
                for entry in tools {
                    if entry.is_error && !entry.content.is_empty() {
                        lines.extend(error_summary_lines(
                            &entry.content,
                            width,
                            if detail_mode {
                                usize::MAX
                            } else {
                                ERROR_SUMMARY_MAX_LINES
                            },
                        ));
                    }
                }
            }

            (lines, link_hits)
        }
    }
}

#[cfg(test)]
#[path = "../background_task_render_test.rs"]
mod background_task_tests;

#[cfg(test)]
mod tests {
    use super::*;
    include!("message_render_test.rs");
}
