use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod decorators;
mod image_references;
mod skill;
mod streaming_bash;
use decorators::{
    ask_user::parse_ask_user_qa_items,
    bash::{parse_exit_code, strip_exit_code_suffix, transform_bash_output_lines},
    glob::glob_summary,
    read::{format_display_path_for_memory, read_summary},
    ToolDisplayLine,
};
pub(crate) use streaming_bash::render_live_output;
pub(crate) use streaming_bash::shell_output_status_text;

use super::{
    branch_prefix::BranchPrefix,
    message_view::{
        AgentSummary, BackgroundTaskStarted, ContentBlockView, HookRow, MessageViewModel, NoteTone,
        ToolCategory, ToolEntry,
    },
    theme,
};
use crate::app::tool_display::sanitize_display_text;

pub(crate) const CONTROL_B_BACKGROUND_HINT: &str = "(ctrl+b to run in background)";
const BACKGROUND_RUNNING_STATUS: &str = "Running in the background (↓ to manage)";

/// hook 汇总行（`N <Event> hooks ran · Xms` / `Async hook <Event> completed`）的折叠上限。
const HOOK_SUMMARY_MAX_ROWS: usize = 4;
/// hook 阻断/失败行的折叠上限。
const HOOK_FAILURE_MAX_ROWS: usize = 3;

/// hook 行语义色：成功汇总用 inactive，阻断用 warning，失败用 error。
///
/// `Success` 当前没有被 `HookRow` 的构造器产出（汇总/异步行是 `Neutral`，
/// 阻断行是 `Warning`，自身失败行是 `Error`），但 `HookRow.tone` 类型上仍可达；
/// `NoteTone` 又是全 UI 共享枚举，此处保留穷举映射而不是 `_` 兜底，
/// 以免将来新增 tone 时被静默吞成 inactive。
fn hook_row_color(tone: NoteTone) -> Color {
    match tone {
        NoteTone::Warning => theme::cur().warning,
        NoteTone::Error => theme::cur().error,
        NoteTone::Success => theme::cur().success,
        NoteTone::Neutral => theme::cur().inactive,
    }
}

/// 渲染一行 hook 结果：`  ⎿  <text>`，`⎿` 与正文同色（跟随 tone）。
///
/// 文本先经 `sanitize_display_text`：hook 的 reason 来自 stderr，可能带换行、
/// 制表符与 ANSI 转义——直接塞进单个 Span 时 `\n` 宽度为 0，不会当换行，
/// 会导致行内容错位，甚至把终端转义序列泄漏到屏幕上。
/// 超宽时按内容宽度**截断**（`…` 收尾）而不是折行——hook 行是状态提示，
/// 折成多行会把正文挤出视口，而且 20 个 hook 时行数会翻倍。
fn push_hook_line(lines: &mut Vec<Line<'static>>, text: &str, color: Color, width: usize) {
    let content_width = BranchPrefix::content_width(width);
    let body = truncate_to_display_width(&sanitize_display_text(text), content_width);
    lines.push(Line::from(vec![
        Span::styled(BranchPrefix::PREFIX, Style::default().fg(color)),
        Span::styled(body, Style::default().fg(color)),
    ]));
}

/// 追加该工具调用触发的 hook 行（规格见 spec/global/domains/hooks-display.md）。
///
/// - 阻断行两种模式都显示；hook 自身失败、汇总、异步完成行仅详细模式显示。
/// - `hook_rows_visible` 为 false 时整段跳过（配置 `hooks_ui` / /config 面板开关）。
///   数据本身保留在 `ToolBlock.hook_rows` 里，开关切回来历史行立即恢复。
/// - 折叠口径：同步汇总最多 4 行（其余折成 `… +N events`），失败最多 3 行
///   （其余折成 `… +N more`）。异步完成行不参与折叠，每个异步 hook 各占一行。
///   行数不随 hook 数量线性增长。
fn append_hook_rows(
    lines: &mut Vec<Line<'static>>,
    hook_rows: &[HookRow],
    detail_mode: bool,
    hook_rows_visible: bool,
    width: usize,
) {
    if hook_rows.is_empty() || !hook_rows_visible {
        return;
    }
    let mut failures = 0usize;
    let mut summaries = 0usize;
    for row in hook_rows {
        if row.detail_only() && !detail_mode {
            continue;
        }
        // 失败类行（阻断 / 自身失败）走 3 行上限，各自按类别独立折叠；
        // 异步逐条行不参与任何上限。
        if row.kind.is_failure() {
            if failures >= HOOK_FAILURE_MAX_ROWS {
                continue;
            }
            failures += 1;
        } else if !row.async_row() {
            if summaries >= HOOK_SUMMARY_MAX_ROWS {
                continue;
            }
            summaries += 1;
        }
        push_hook_line(lines, &row.text, hook_row_color(row.tone), width);
    }

    let visible_failures = hook_rows
        .iter()
        .filter(|row| row.kind.is_failure() && (!row.detail_only() || detail_mode))
        .count();
    let hidden_failures = visible_failures.saturating_sub(HOOK_FAILURE_MAX_ROWS);
    if hidden_failures > 0 {
        push_hook_line(
            lines,
            &format!("… +{hidden_failures} more"),
            theme::cur().subtle,
            width,
        );
    }
    let hidden_summaries = if detail_mode {
        hook_rows
            .iter()
            .filter(|row| !row.kind.is_failure() && !row.async_row())
            .count()
            .saturating_sub(HOOK_SUMMARY_MAX_ROWS)
    } else {
        0
    };
    if hidden_summaries > 0 {
        push_hook_line(
            lines,
            &format!("… +{hidden_summaries} events"),
            theme::cur().subtle,
            width,
        );
    }
}

/// 动态走秒时长格式化直接复用底层的 [`cc_widgets::format_elapsed_secs`]。
pub(crate) use cc_widgets::format_elapsed_secs;

/// 把秒数格式化为人类可读的紧凑时长，专用于超时期限（timeout）等静态上限配置展示。
///
/// 整分钟（如 10m、2m）或整小时（如 1h）省略尾随零，非整分时保留秒数（如 9m 20s）。
pub(crate) fn format_timeout_secs(secs: u64) -> String {
    if secs >= 3600 {
        let (hours, minutes, rest) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        if minutes == 0 && rest == 0 {
            format!("{hours}h")
        } else if rest == 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h {minutes}m {rest}s")
        }
    } else if secs >= 60 {
        let (minutes, rest) = (secs / 60, secs % 60);
        if rest == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m {rest}s")
        }
    } else {
        format!("{secs}s")
    }
}

/// 模型请求的 `timeout`（毫秒）→ `"timeout 9m 20s"`（不含外层括号）。
///
/// 不足 1 分钟的期限保留原有精度：整千毫秒写 `Ns`，否则写原始毫秒。
pub(crate) fn shell_timeout_label(timeout_ms: Option<u64>) -> Option<String> {
    timeout_ms.filter(|ms| *ms > 0).map(|ms| {
        let limit = if ms >= 60_000 {
            format_timeout_secs(ms / 1_000)
        } else if ms.is_multiple_of(1_000) {
            format!("{}s", ms / 1_000)
        } else {
            format!("{ms}ms")
        };
        format!("timeout {limit}")
    })
}

/// 模型请求的 `timeout`（毫秒）→ `(timeout 9m 20s)`。
pub(crate) fn shell_timeout_text(timeout_ms: Option<u64>) -> Option<String> {
    shell_timeout_label(timeout_ms).map(|label| format!("({label})"))
}

pub(crate) fn shell_running_text(
    started_at: std::time::Instant,
    timeout_ms: Option<u64>,
) -> String {
    let elapsed = format_elapsed_secs(started_at.elapsed().as_secs());
    match shell_timeout_label(timeout_ms) {
        Some(timeout) => format!("Running… ({elapsed} · {timeout})"),
        None => format!("Running… ({elapsed})"),
    }
}

/// Bash 运行状态行（`"  ⎿  Running… (Xs)"`）中状态文本的固定前缀。
pub(crate) const SHELL_RUNNING_TEXT_PREFIX: &str = "Running…";

/// 命令已有输出、无法占用状态行时单独显示的耗时文本：`(22s)` / `(1m 05s)` / `(5m 00s)`。
///
/// 渲染与渲染线程的增量刷新共用同一份文本，两边口径必须一致。
pub(crate) fn shell_elapsed_text(started_at: std::time::Instant) -> String {
    format!("({})", format_elapsed_secs(started_at.elapsed().as_secs()))
}

/// 判断某渲染行是否为 Bash "⎿ Running…" 运行状态行。
///
/// 供渲染线程在增量刷新时**按内容定位**状态行。详细模式下超长命令会让 header
/// 折成多行，状态行下标不再固定为 1（issue：固定写下标 1 会覆盖命令续行）。
pub(crate) fn is_shell_running_status_line(line: &Line<'static>) -> bool {
    line.spans.len() >= 2
        && ((BranchPrefix::is_branch_line(line)
            && line.spans[1].content.starts_with(SHELL_RUNNING_TEXT_PREFIX))
            || (line.spans[0].content == BranchPrefix::HANG
                && line.spans[1].content.starts_with('(')
                && line.spans[1].content.ends_with("B)")
                && line.spans[1].style.fg == Some(theme::cur().inactive)))
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

/// 详细模式下渲染思考正文：markdown 解析 + DIM 压暗 + 折行，末尾补一行空行。
///
/// 正文为空（模型只回了耗时、没有可读思考）时不能只画一个孤零零的 `⎿`，
/// 按 Bash 空输出 `(No output)` 的同款口径补 `(No content)`。
fn push_thinking_body(lines: &mut Vec<Line<'static>>, text: &str, width: usize) {
    let prefix_style = Style::default().fg(theme::cur().inactive);
    if text.trim().is_empty() {
        lines.push(Line::from(vec![
            Span::styled(BranchPrefix::PREFIX, prefix_style),
            Span::styled("(No content)", prefix_style),
        ]));
        lines.push(Line::from(""));
        return;
    }
    let content_width = BranchPrefix::content_width(width);
    let parsed = super::markdown::parse_markdown(text, content_width);
    for (i, line) in dim_markdown_lines(parsed).into_iter().enumerate() {
        // 折行：长段落按 content_width 切成多行，每行加分支前缀
        // 后不会超过 width，避免 Paragraph::wrap 二次折行使续行缺少缩进
        for (j, wline) in wrap_line_spans(line, content_width).into_iter().enumerate() {
            let prefix = if i == 0 && j == 0 {
                BranchPrefix::PREFIX
            } else {
                BranchPrefix::HANG
            };
            let mut spans = vec![Span::styled(prefix, prefix_style)];
            spans.extend(wline.spans);
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::from(""));
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
                    recolor: hit.recolor,
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
/// 折行 `line` 并推入 `out`，返回段级 grapheme 映射供调用方换算链接命中区。
///
/// 返回值的 [`WrappedLineSeg::line`] **已被清空**——spans 已移入 `out` 的行，
/// 这里只承载 `in_g_start` / `in_g_end` / `out_g_offset` 坐标语义，不能用于绘制。
fn push_wrapped_line_keep_first_lead(
    out: &mut Vec<Line<'static>>,
    line: Line<'static>,
    first_prefix: &str,
    cont_prefix: &str,
    prefix_style: Style,
    content_width: usize,
) -> Vec<WrappedLineSeg> {
    let mut wrapped = wrap_line_spans_rich_keep_first_lead(line, content_width);
    for (j, seg) in wrapped.iter_mut().enumerate() {
        let prefix = if j == 0 { first_prefix } else { cont_prefix };
        let mut spans = Vec::with_capacity(seg.line.spans.len() + 1);
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix.to_string(), prefix_style));
        }
        spans.extend(std::mem::take(&mut seg.line.spans));
        out.push(Line::from(spans));
    }
    wrapped
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
pub(crate) fn find_url_ranges(text: &str) -> Vec<(usize, usize)> {
    const SCHEMES: [&str; 2] = ["https://", "http://"];
    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(found) = SCHEMES
        .iter()
        .filter_map(|scheme| {
            text[search_from..]
                .find(scheme)
                .map(|at| (search_from + at, *scheme))
        })
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
    let line = Line::from(Span::styled(sanitize_display_text(text), text_style));
    // 续行悬挂缩进与首行前缀等宽，保证正文左对齐且总宽不溢出。
    let hang = " ".repeat(UnicodeWidthStr::width(first_prefix));
    format_wrapped_line_keep_lead(line, first_prefix, &hang, prefix_style, content_width)
}

/// 把 Write 的 `Wrote N lines <path>` 结果行重绘为 CC 的样式
/// `Wrote <bold>N</bold> lines to <bold>path</bold>`（`FileWriteTool/UI.tsx:58-61`）。
///
/// 只处理精确匹配的单行文本；其它输出（如 append 的 `Appended ...`）返回 None，
/// 由调用方回退为普通纯文本渲染。
/// Write 的 `Wrote N lines <path>` 结果行 → 重绘后的行、路径在逻辑行中的
/// grapheme 起点、路径原文。
///
/// 参考实现（2.1.310 `FileWriteToolCreatedMessage`）把这一行的路径也包进了
/// `FilePathLink`，因此路径需要参与命中区登记。
fn format_write_summary_line(
    text: &str,
    text_color: ratatui::style::Color,
) -> Option<(Line<'static>, usize, String)> {
    let rest = text.strip_prefix("Wrote ")?;
    let (count, rest) = rest.split_once(' ')?;
    count.parse::<usize>().ok()?;
    let (unit, path) = rest.split_once(' ')?;
    if (unit != "line" && unit != "lines") || path.is_empty() {
        return None;
    }
    let base = Style::default().fg(text_color);
    let bold = base.add_modifier(ratatui::style::Modifier::BOLD);
    let path_start = format!("Wrote {count} {unit} to ").chars().count();
    let line = Line::from(vec![
        Span::styled("Wrote ", base),
        Span::styled(count.to_string(), bold),
        Span::styled(format!(" {unit} to "), base),
        Span::styled(path.to_string(), bold),
    ]);
    Some((line, path_start, path.to_string()))
}

/// Write 结果行里的路径 → `file://` 链接目标；路径无法绝对化时返回 None。
///
/// 与 [`tool_file_url`] 一致：命中区登记不受 `osc8::enabled()` 影响。
fn write_target_url(path_text: &str) -> Option<String> {
    Some(
        url::Url::from_file_path(absolute_link_path(path_text)?)
            .ok()?
            .to_string(),
    )
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

/// 格式化折叠截断提示语：统一对齐为 `… +N lines (ctrl+o to expand)`。
///
/// - `remaining == 0`（仅因单行折行超配额）：`… (ctrl+o to expand)`
/// - `remaining == 1`：`… +1 line (ctrl+o to expand)`
/// - `remaining > 1`：`… +N lines (ctrl+o to expand)`
fn format_expand_hint(remaining: usize) -> String {
    if remaining == 0 {
        "… (ctrl+o to expand)".to_string()
    } else if remaining == 1 {
        "… +1 line (ctrl+o to expand)".to_string()
    } else {
        format!("… +{remaining} lines (ctrl+o to expand)")
    }
}

/// 按视觉行预算推入内容行，若超出预算则在最后追加展开提示行。
///
/// 保留前 `max_lines` 个视觉行。若发生截断：
/// - 若有未展示的后续逻辑行，提示 `… +{remaining} lines (ctrl+o to expand)`；
/// - 若所有逻辑行已进入但因单行折行超出配额被截断，提示 `… (ctrl+o to expand)`。
fn push_visual_capped_output<T, F>(
    out: &mut Vec<Line<'static>>,
    items: &[T],
    max_lines: usize,
    border_color: Color,
    mut format_line: F,
) where
    F: FnMut(&T, usize, usize) -> Vec<Line<'static>>,
{
    let total = items.len();
    let mut rendered_visual = 0;
    for (i, item) in items.iter().enumerate() {
        let available = max_lines.saturating_sub(rendered_visual);
        if available == 0 {
            let remaining_logical = total.saturating_sub(i);
            let hint = format_expand_hint(remaining_logical);
            out.push(Line::from(vec![
                Span::styled(BranchPrefix::HANG, Style::default().fg(border_color)),
                Span::styled(hint, Style::default().fg(theme::cur().subtle)),
            ]));
            return;
        }

        // 第三个参数是该项首个输出行在 `out` 中的下标，供需要登记超链接命中区
        // （行号相对的 `LinkHit`）的调用方换算。
        let wlines = format_line(item, i, out.len());
        if wlines.len() <= available {
            rendered_visual += wlines.len();
            out.extend(wlines);
            if rendered_visual >= max_lines && i + 1 < total {
                let remaining_logical = total - (i + 1);
                let hint = format_expand_hint(remaining_logical);
                out.push(Line::from(vec![
                    Span::styled(BranchPrefix::HANG, Style::default().fg(border_color)),
                    Span::styled(hint, Style::default().fg(theme::cur().subtle)),
                ]));
                return;
            }
        } else {
            out.extend(wlines.into_iter().take(available));
            let remaining_logical = total.saturating_sub(i + 1);
            let hint = format_expand_hint(remaining_logical);
            out.push(Line::from(vec![
                Span::styled(BranchPrefix::HANG, Style::default().fg(border_color)),
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
/// 超出时追加 `… +N lines (ctrl+o to expand)` 提示。字符级截断兜底防止超长单行。
fn error_summary_lines(content: &str, width: usize, max_lines: usize) -> Vec<Line<'static>> {
    let truncated: String = content.chars().take(400).collect();
    let content_width = BranchPrefix::content_width(width);
    let all: Vec<&str> = truncated.lines().collect();
    let mut out = Vec::new();
    for (i, line) in all.iter().enumerate() {
        if i >= max_lines {
            let hidden = all.len() - max_lines;
            let hint = format_expand_hint(hidden);
            out.push(Line::from(Span::styled(
                format!("{}{hint}", BranchPrefix::HANG),
                Style::default().fg(theme::cur().subtle),
            )));
            break;
        }
        let first_prefix = if i == 0 {
            BranchPrefix::PREFIX
        } else {
            BranchPrefix::HANG
        };
        push_prefixed_text(
            &mut out,
            line,
            first_prefix,
            Style::default().fg(theme::cur().error),
            Style::default().fg(theme::cur().error),
            content_width,
        );
    }
    out
}

fn tool_entry_is_error(entry: &ToolEntry) -> bool {
    entry.is_error
        || (entry.tool_name == "Bash"
            && parse_exit_code(&entry.content).is_some_and(|code| code != 0))
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

/// 把字符串首字符转大写，其余保持不变。
///
/// 用于**独立成行**的动作摘要（如 `read 1 file` → `Read 1 file`）：
/// 同一份文案在另一处被 `", {s}"` 拼到 `Thought for 3s` 之后，
/// 那里必须保持小写，因此大小写调整只能在渲染侧按上下文做，不能改文案产出层。
fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 把已完成态的动作摘要改成进行时（`read 2 files` → `reading 2 files`），
/// 用于运行中的 Thinking 行，对齐参考实现的 `Thinking for 1s, reading 2 files…`。
fn present_tense_summary(summary: &str) -> String {
    const VERBS: [(&str, &str); 6] = [
        ("searched for ", "searching for "),
        ("listed ", "listing "),
        ("recalled ", "recalling "),
        ("updated ", "updating "),
        ("ran ", "running "),
        ("read ", "reading "),
    ];
    summary
        .split(", ")
        .map(|part| {
            for (from, to) in VERBS {
                if let Some(rest) = part.strip_prefix(from) {
                    return format!("{to}{rest}");
                }
            }
            part.to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Write/Edit/Read 工具头里承载路径的文本部分：Write/Edit 是整段摘要，
/// Read 是 `路径 · lines 起-止` 去掉行范围后的前缀。其余工具返回 None。
fn tool_path_text<'a>(tool: &str, args: &'a str) -> Option<&'a str> {
    match tool {
        "Write" | "Edit" => Some(args),
        "Read" => Some(split_read_range(args).0),
        _ => None,
    }
}

/// 工具头路径的 OSC 8 `file://` 链接目标；路径无法绝对化时返回 None。
///
/// 注意：这里**不**看 `osc8::enabled()`。该开关只控制输出层是否写 OSC 8 序列
/// （`hyperlink::begin_frame`），点击命中区是独立数据——与 `bare_url_hits` 等
/// 既有链接一致。若把两者绑在一起，`DISABLE_HYPERLINKS=1` 会让点击一并失效。
fn tool_file_url(tool: &str, args: &str) -> Option<String> {
    let path = absolute_link_path(tool_path_text(tool, args)?)?;
    Some(url::Url::from_file_path(path).ok()?.to_string())
}

/// 详细模式下的路径显示：对齐参考实现的 verbose 分支——一律完整绝对路径。
///
/// 已是绝对路径的原样返回（保留输入的分隔符风格），cwd/home 内的相对或 `~`
/// 记法展开成绝对路径并把分隔符归一为 `/`。
fn verbose_display_path(text: &str) -> Option<String> {
    if std::path::Path::new(text).is_absolute() {
        return Some(text.to_string());
    }
    absolute_link_path(text).map(|path| path.to_string_lossy().replace('\\', "/"))
}

/// 详细模式下的工具参数显示。
///
/// 对齐参考实现：Write/Edit/Read 显示完整绝对路径；Read 的行范围以
/// ` · lines 起-止`（只有起点时 ` · from line 起`）追加在路径之后，
/// 其余工具的摘要沿用原有 sanitize 结果。
fn verbose_tool_args(tool: &str, args: &str) -> String {
    let Some(path_text) = tool_path_text(tool, args) else {
        return sanitize_display_text(args);
    };
    let display = verbose_display_path(path_text).unwrap_or_else(|| path_text.to_string());
    // 只有 Read 带行范围文案；Write/Edit 的路径即使恰好含 ` · lines …` 也不追加。
    match (tool, split_read_range(args).1) {
        ("Read", Some(range)) => format!("{display} · {range}"),
        _ => display,
    }
}

/// 把 Read 摘要切成「路径」与「行范围文案」。
///
/// `format_tool_args` 追加的是参考实现的固定文案（` · lines …` / ` · from line …`），
/// 分隔标记是文字而非数字位置，因此文件名自带 `:123` 时不会再被误判成行号范围。
fn split_read_range(args: &str) -> (&str, Option<&str>) {
    let Some((path, range)) = args.rsplit_once(" · ") else {
        return (args, None);
    };
    if range.starts_with("lines ") || range.starts_with("from line ") {
        (path, Some(range))
    } else {
        (args, None)
    }
}

/// 展示路径 → 可打开的文件路径：`~/` 展开到 home，相对路径按进程 cwd 补齐，
/// 绝对路径原样。与参考实现的 `FileWriteTool/UI.tsx` 一致——显示文本可以是
/// 相对路径，链接目标必须是绝对路径。
fn absolute_link_path(text: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(text);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    if text == "~" {
        return dirs_next::home_dir();
    }
    if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        return dirs_next::home_dir().map(|home| home.join(rest));
    }
    std::env::current_dir().ok().map(|cwd| cwd.join(path))
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

            // final_result 行（嵌套折角 = NESTED_WIDTH 列，续行对齐同宽）
            if let Some(ref result) = agent.final_result {
                if !result.is_empty() {
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        Line::from(Span::styled(
                            sanitize_display_text(result),
                            Style::default().fg(theme::cur().inactive),
                        )),
                        BranchPrefix::NESTED_PREFIX,
                        BranchPrefix::NESTED_HANG,
                        Style::default().fg(theme::cur().inactive),
                        BranchPrefix::nested_content_width(width),
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
            BranchPrefix::PREFIX,
            BranchPrefix::HANG,
            Style::default().fg(if is_error {
                theme::cur().error
            } else {
                theme::cur().inactive
            }),
            BranchPrefix::content_width(width),
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
/// 首行用 `prefix`、续行用 [`BranchPrefix::HANG`]，形成悬挂缩进；不设背景色，与工具结果行
/// （`⎿` 前缀 + INACTIVE/ERROR 前景色）保持一致。
/// 预折行避免 `Paragraph::wrap` 二次硬折行使长输出续行顶格。
fn format_shell_output_lines(
    prefix: &str,
    text: &str,
    default_style: Style,
    prefix_color: Color,
    width: usize,
) -> Vec<Line<'static>> {
    let prefix_style = Style::default().fg(prefix_color);
    // 先按 ANSI 切段保留命令自带着色。URL 不再单独着色：可点击性由消息区
    // 注入的 OSC 8 给出（见 `crate::ui::osc8`），视觉上与正文一致（对齐 CC）。
    let content: Vec<Span<'static>> = ansi_spans(text, default_style);
    format_wrapped_line_keep_lead(
        Line::from(content),
        prefix,
        BranchPrefix::HANG,
        prefix_style,
        BranchPrefix::content_width(width),
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

/// 用 `user_message_background` 补空格到视口整宽，让底色铺满整行。
///
/// 用户消息、`!` 本机命令、排队消息与 sticky header 共用这一口径
/// （TUI-STYLE：「用户消息底色 —— 所有行带底色」）。行尾补出的空格不参与选区与复制
/// （`app::text_selection` 已对每行做 `trim_end`），因此不会污染剪贴板。
fn push_bg_fill(spans: &mut Vec<Span<'static>>, width: usize) {
    let used: usize = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(theme::cur().user_message_background),
        ));
    }
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

    // 与 Bash 工具同一口径：前 2 秒只显示裸 `Running…`，满 2 秒后才把已运行时间
    // 拼进状态行。`!` 命令没有 timeout 参数，故不传时长上限。
    let past_threshold =
        started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2));
    let running_text = |started: std::time::Instant| {
        if past_threshold {
            shell_running_text(started, None)
        } else {
            SHELL_RUNNING_TEXT_PREFIX.to_string()
        }
    };

    if output_lines.is_empty() {
        let text = match (exit_code, started_at) {
            (None, Some(started)) => running_text(started),
            _ => "(No output)".to_string(),
        };
        shell_output_lines(
            &mut lines,
            BranchPrefix::PREFIX,
            &text,
            Style::default().fg(theme::cur().text_soft),
            theme::cur().inactive,
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
            |(line, is_error), idx, _out_base| {
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
                    theme::cur().inactive
                };
                let prefix = if idx == 0 {
                    BranchPrefix::PREFIX
                } else {
                    BranchPrefix::HANG
                };
                format_shell_output_lines(prefix, line, default_style, prefix_color, width)
            },
        );
    }

    // 2 秒阈值提示：耗时已并入状态行，这里只补后台化提示；命令已有输出（没有
    // 状态行）时仍单独给出已运行时间，避免丢掉「跑了多久」。
    let status_line_shown = output_lines.is_empty();
    if !moved_to_background && exit_code.is_none() && past_threshold {
        if !status_line_shown {
            if let Some(start) = started_at {
                let elapsed_str = shell_elapsed_text(start);
                shell_output_lines(
                    &mut lines,
                    BranchPrefix::HANG,
                    &elapsed_str,
                    Style::default().fg(theme::cur().inactive),
                    theme::cur().subtle,
                    width,
                );
            }
        }
        shell_output_lines(
            &mut lines,
            BranchPrefix::HANG,
            CONTROL_B_BACKGROUND_HINT,
            Style::default().fg(theme::cur().inactive),
            theme::cur().subtle,
            width,
        );
    }
    lines
}

/// 系统提示的语义色 → 当前主题色。
///
/// 渲染期解析而非构造期固化，`/theme` 切换后已有 VM 也能立刻换色。
fn note_tone_color(tone: crate::ui::message_view::NoteTone) -> ratatui::style::Color {
    use crate::ui::message_view::NoteTone;
    match tone {
        NoteTone::Success => theme::cur().success,
        NoteTone::Error => theme::cur().error,
        NoteTone::Warning => theme::cur().warning,
        // stall 通知在参考实现里没有 status，落到默认前景色。
        NoteTone::Neutral => theme::cur().text,
    }
}

/// 语义提示的行首符号。
///
/// 警告/错误/成功各用自己的符号，用户不必读正文就能分辨严重程度；
/// 中性语义没有专门符号，沿用普通系统提示的 `·`（它本身不表达状态）。
fn note_tone_prefix(tone: Option<crate::ui::message_view::NoteTone>) -> &'static str {
    use crate::ui::message_view::NoteTone;
    match tone {
        Some(NoteTone::Warning) => "⚠ ",
        Some(NoteTone::Error) => "✗ ",
        Some(NoteTone::Success) => "✓ ",
        Some(NoteTone::Neutral) | None => "· ",
    }
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
    render_view_model_with_flags(vm, index, width, detail_mode, false, true, tick)
}

/// 带完整渲染标志的入口。
///
/// 三个标志语义不同，必须分开传：
/// - `detail_mode`（Ctrl+O 详细模式）：控制 shell 输出行数、粘贴内容、reasoning、
///   工具折叠状态等**整页展开**行为；
/// - `diff_visible`（配置 `diff_enabled`）：只控制**普通页面**是否内联显示 Write/Edit
///   的 diff 块，不影响上面任何一项（详细模式下 diff 恒可见）；
/// - `hook_rows_visible`（配置 `hooks_ui`）：只控制 hook 结果行是否渲染，
///   VM 里的 hook 数据始终保留（开关切回来历史行恢复）。
pub fn render_view_model_with_flags(
    vm: &MessageViewModel,
    _index: Option<usize>,
    width: usize,
    detail_mode: bool,
    diff_visible: bool,
    hook_rows_visible: bool,
    tick: u64,
) -> (Vec<Line<'static>>, Vec<cc_widgets::markdown::LinkHit>) {
    use unicode_segmentation::UnicodeSegmentation;

    let mut link_hits: Vec<cc_widgets::markdown::LinkHit> = Vec::new();
    match vm {
        MessageViewModel::UserBubble {
            content,
            rendered,
            rendered_links,
            image_paths,
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
                        // 补空格到视口整宽，让底色铺满整行（与 `shell_command_header` /
                        // `queued_messages` / `sticky_header` 的用户消息底色口径一致，
                        // 见 TUI-STYLE「所有行带底色」）。行尾空白不参与选区复制
                        // （`text_selection` 已 trim_end）。
                        push_bg_fill(&mut spans, width);
                        lines.push(Line::from(spans));
                    } else {
                        // 后续行（含预折行产生的续行）：2 空格悬挂缩进，带底色
                        let mut spans = vec![Span::styled("  ", Style::default().bg(user_bg))];
                        for span in &wline.spans {
                            spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
                        }
                        push_bg_fill(&mut spans, width);
                        lines.push(Line::from(spans));
                    }
                }
            }
            image_references::append(
                &mut lines,
                &mut link_hits,
                expanded_content.as_deref().unwrap_or(content),
                image_paths,
                width,
                super::osc8::enabled(),
            );
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
                        active: block_active,
                        active_since,
                        ..
                    } => {
                        // 运行中（本轮未结束且非详细模式）：对齐参考实现——
                        // 行首圆点 + 现在时 + 省略号 + 走秒，**不吐思考正文**。
                        let active = *block_active && !detail_mode;
                        // Thought 标题：缩进 2 列对齐。活跃态没有可展开的正文，不给提示
                        let hint = if detail_mode || active {
                            ""
                        } else {
                            " (ctrl+o to expand)"
                        };
                        // 走秒口径同参考实现 Jc：已完成累计 + 距锚点（最后一段思考结束）的增量，
                        // 至少显示 1s。
                        let live_ms = active.then(|| {
                            let base = duration_ms.unwrap_or(0);
                            let extra = active_since
                                .map(|anchor| anchor.elapsed().as_millis() as u64)
                                .unwrap_or(0);
                            base + extra
                        });
                        let live_secs = live_ms.map(|ms| ms.max(1000) / 1000);
                        let (prefix, timing) = if let Some(secs) = live_secs {
                            // 活跃态恒显 `∴ ` 前缀（不再做 500ms 显隐闪烁）
                            let dot = "∴ ".to_string();
                            (dot, format!("Thinking for {secs}s"))
                        } else {
                            // 有耗时 → `Thought for Ns`；无耗时（历史旧数据）→ 回退字数
                            // 冻结后不再带符号，只保留缩进（与活跃行对齐）
                            (
                                "  ".to_string(),
                                match duration_ms {
                                    Some(ms) => {
                                        format!("Thought for {}s", ms.div_ceil(1000).max(1))
                                    }
                                    None => format!("Thought for {char_count} chars"),
                                },
                            )
                        };
                        // 紧随的只读工具计数（如 `read 1 file, listed 1 directory`）
                        let actions = action_summary
                            .as_deref()
                            .map(|s| {
                                if active {
                                    format!(", {}", present_tense_summary(s))
                                } else {
                                    format!(", {s}")
                                }
                            })
                            .unwrap_or_default();
                        let ellipsis = if active { "…" } else { "" };
                        let title = format!("{prefix}{timing}{actions}{ellipsis}{hint}");
                        // 颜色对齐参考实现 `dimColor={!isActiveGroup}`：
                        // 活跃用正常亮度（text_soft），结束后才压暗（inactive）。
                        let title_color = if live_secs.is_some() {
                            theme::cur().text_soft
                        } else {
                            theme::cur().inactive
                        };
                        lines.push(Line::from(vec![Span::styled(
                            title,
                            Style::default().fg(title_color),
                        )]));
                        // 普通模式：只留这一行摘要，**不渲染任何思考正文**。
                        // 详细模式：显示完整 reasoning，走 markdown 解析 + DIM overlay
                        if detail_mode {
                            push_thinking_body(&mut lines, text, width);
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
            phase,
            source_agent_id,
            live_output,
            hook_rows,
            ..
        } => {
            // AskUserQuestion 专用渲染路径
            if tool_name == "AskUserQuestion" {
                // 与其余提前返回分支一致：hook 行挂在工具内容之后，否则整段丢失。
                let mut lines = render_ask_user_block(content, *is_error, width);
                append_hook_rows(&mut lines, hook_rows, detail_mode, hook_rows_visible, width);
                return (lines, link_hits);
            }
            if tool_name == "Skill" && *collapsed && !detail_mode {
                let mut lines = skill::render_collapsed_skill(
                    args_display.as_deref(),
                    content,
                    *is_error,
                    *phase,
                    width,
                    tick,
                );
                // 折叠 Skill 分支在臂末尾之前提前返回：hook 失败/阻断行仍要跟上。
                append_hook_rows(&mut lines, hook_rows, detail_mode, hook_rows_visible, width);
                return (lines, link_hits);
            }
            let skill_instruction = if tool_name == "Skill" && !*is_error {
                skill::instruction_for_display(content)
            } else {
                None
            };
            let content = skill_instruction.as_ref().unwrap_or(content);

            let is_running = *phase != crate::ui::message_view::ToolPhase::Finished && !*is_error;
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
            } else if *phase == crate::ui::message_view::ToolPhase::Running {
                cc_widgets::ToolCallStatus::Running
            } else if is_running {
                cc_widgets::ToolCallStatus::Pending
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
                    // 详细模式：Write/Edit/Read 显示完整绝对路径，Read 的行范围以
                    // ` · lines 起-止` 追加在路径之后（对齐参考实现的 verbose 分支）。
                    let full = verbose_tool_args(tool_name, args);
                    let indent = " ".repeat(name_width + 1);
                    let mut spans = header_spans.clone();
                    let args_start_g = spans
                        .iter()
                        .map(|span| span.content.graphemes(true).count())
                        .sum::<usize>()
                        + 1;
                    spans.push(Span::raw("("));
                    spans.push(Span::styled(full, args_color));
                    spans.push(Span::raw(")"));
                    let content_width = width.saturating_sub(name_width + 1).max(20);
                    let header_line = lines.len();
                    // 用 keep_first_lead 变体：运行中指示器闪烁到空白帧时，header 首段
                    // 的行首空白（指示器占位）不能被 trim，否则整个前缀随闪烁左移抖动
                    // （issue #342：详细模式看不到 `● Bash(` 前缀）。
                    let wrapped = push_wrapped_line_keep_first_lead(
                        &mut lines,
                        Line::from(spans),
                        "",
                        &indent,
                        Style::default(),
                        content_width,
                    );
                    // 链接只覆盖绝对路径本身，` · lines …` 后缀不参与；折行后按段映射，
                    // 续行补上缩进宽度。
                    if let Some(url) = tool_file_url(tool_name, args) {
                        let path_end = tool_path_text(tool_name, args)
                            .map(|text| {
                                verbose_display_path(text)
                                    .unwrap_or_else(|| text.to_string())
                                    .graphemes(true)
                                    .count()
                            })
                            .map_or(args_start_g, |path_g| args_start_g + path_g);
                        let cont_g = indent.graphemes(true).count();
                        for (i, seg) in wrapped.iter().enumerate() {
                            let is = args_start_g.max(seg.in_g_start);
                            let ie = path_end.min(seg.in_g_end);
                            if is >= ie {
                                continue;
                            }
                            let base = if i == 0 { 0 } else { cont_g };
                            link_hits.push(cc_widgets::markdown::LinkHit {
                                line: header_line + i,
                                g_start: base + seg.out_g_offset + (is - seg.in_g_start),
                                g_end: base + seg.out_g_offset + (ie - seg.in_g_start),
                                url: url.clone(),
                                // 工具头路径已由 args_color 着色，只借 OSC 8 获得可点性，
                                // 不得被输出层改写成链接蓝。
                                recolor: false,
                            });
                        }
                    }
                } else {
                    let path_text = tool_path_text(tool_name, args);
                    let header_args = path_text.unwrap_or(args);
                    let max_args_width = header_args_width(width, name_width + 2, detail_mode);
                    let summary = tool_args_header(tool_name, header_args, max_args_width);
                    let args_start_g = header_spans
                        .iter()
                        .map(|span| span.content.graphemes(true).count())
                        .sum::<usize>()
                        + 1;
                    header_spans.push(Span::styled(format!("({})", summary), args_color));
                    let header_line = lines.len();
                    lines.push(Line::from(header_spans));
                    // 链接只覆盖仍可见的路径前缀；摘要被截断时以「…」收尾，
                    // 省略号不属于路径。memory 标签替换后文本已不是原路径，
                    // 不再挂链接（与 `tool_args_header` 的替换条件一致）。
                    if let Some(url) = tool_file_url(tool_name, args) {
                        if format_display_path_for_memory(args).is_none() {
                            let visible_g = summary.graphemes(true).count();
                            let path_g = header_args.graphemes(true).count();
                            let truncated =
                                sanitize_display_text(header_args).graphemes(true).count()
                                    > visible_g;
                            let visible =
                                path_g.min(visible_g.saturating_sub(usize::from(truncated)));
                            if visible > 0 {
                                link_hits.push(cc_widgets::markdown::LinkHit {
                                    line: header_line,
                                    g_start: args_start_g,
                                    g_end: args_start_g + visible,
                                    url,
                                    // 同详细模式：保留工具头自身的路径配色。
                                    recolor: false,
                                });
                            }
                        }
                    }
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
                    BranchPrefix::span(false),
                    Span::styled(
                        truncate_to_display_width(
                            &summary,
                            width.saturating_sub(BranchPrefix::WIDTH),
                        ),
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if *shell_backgrounded {
                    if let Some(timeout) = shell_timeout_text(*timeout_ms) {
                        lines.push(Line::from(vec![
                            Span::raw(BranchPrefix::HANG),
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
                        Span::raw(BranchPrefix::HANG),
                        Span::styled(
                            truncate_to_display_width(
                                &output,
                                width.saturating_sub(BranchPrefix::WIDTH),
                            ),
                            Style::default().fg(theme::cur().subtle),
                        ),
                    ]));
                }
                // 实时、历史重建和子 Agent 展开都共用此渲染入口。
                // 不改 content，因此模型仍能使用完整的任务 ID 和输出路径。
                // 转后台同样可能带阻断/失败行（PRD「与 Bash 执行结果的组合」），
                // 必须在提前返回前追加，否则这些行会整段丢失。
                append_hook_rows(&mut lines, hook_rows, detail_mode, hook_rows_visible, width);
                return (lines, link_hits);
            }
            if tool_name == "Bash"
                && *phase == crate::ui::message_view::ToolPhase::Running
                && !*is_error
                && !*shell_backgrounded
                && started_at
                    .is_some_and(|start| start.elapsed() >= std::time::Duration::from_secs(2))
            {
                if let Some(output) = live_output
                    .as_ref()
                    .filter(|output| !output.tail.is_empty())
                {
                    lines.extend(render_live_output(output, width, detail_mode));
                    lines.push(Line::from(vec![
                        Span::raw(BranchPrefix::HANG),
                        Span::styled(
                            shell_output_status_text(started_at.unwrap(), *timeout_ms, output),
                            Style::default().fg(theme::cur().inactive),
                        ),
                    ]));
                    if source_agent_id.is_none() {
                        lines.push(Line::from(vec![
                            Span::raw(BranchPrefix::HANG),
                            Span::styled(
                                CONTROL_B_BACKGROUND_HINT,
                                Style::default().fg(theme::cur().inactive),
                            ),
                        ]));
                    }
                    // 运行中带实时输出的分支同样提前返回：hook 行必须挂在输出之后。
                    append_hook_rows(&mut lines, hook_rows, detail_mode, hook_rows_visible, width);
                    return (lines, link_hits);
                }
            }
            let empty_output = content.trim().is_empty()
                || (tool_name == "Bash"
                    && !*is_error
                    && !bash_failed
                    && strip_exit_code_suffix(content).is_empty());
            let result_lines: Vec<&str> = if empty_output {
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
                    theme::cur().inactive
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
                            BranchPrefix::span(false),
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

                let content_width = BranchPrefix::content_width(width);
                if let Some(ref b_lines) = bash_display_lines {
                    push_visual_capped_output(
                        &mut lines,
                        b_lines,
                        max_lines,
                        border_color,
                        |item, _i, _out_base| {
                            let text_color = if item.is_error_header {
                                theme::cur().error
                            } else {
                                result_color
                            };
                            format_prefixed_text(
                                item.text.as_ref(),
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
                        |line, i, out_base| {
                            let first_prefix = if i == 0 && tool_name != "Glob" {
                                BranchPrefix::PREFIX
                            } else {
                                BranchPrefix::HANG
                            };
                            // 对齐参考实现的 Write create 结果行：`Wrote N lines to <path>`，
                            // 数字与路径加粗，且路径同样包成 file:// 链接（可点击打开）。
                            if tool_name == "Write" {
                                if let Some((styled, path_start, path_text)) =
                                    format_write_summary_line(line, result_color)
                                {
                                    let wrapped = wrap_line_spans_rich(styled, content_width);
                                    if let Some(url) = write_target_url(&path_text) {
                                        let path_end = path_start + path_text.chars().count();
                                        let cont_g = BranchPrefix::HANG.chars().count();
                                        for (j, seg) in wrapped.iter().enumerate() {
                                            let is = path_start.max(seg.in_g_start);
                                            let ie = path_end.min(seg.in_g_end);
                                            if is >= ie {
                                                continue;
                                            }
                                            // 首段带 `first_prefix`、续行带悬挂缩进，两者宽度相同。
                                            let base = if j == 0 {
                                                first_prefix.chars().count()
                                            } else {
                                                cont_g
                                            };
                                            link_hits.push(cc_widgets::markdown::LinkHit {
                                                line: out_base + j,
                                                g_start: base
                                                    + seg.out_g_offset
                                                    + (is - seg.in_g_start),
                                                g_end: base
                                                    + seg.out_g_offset
                                                    + (ie - seg.in_g_start),
                                                url: url.clone(),
                                                // 结果行由 result_color + BOLD 着色，
                                                // 蓝化会盖掉它的配色。
                                                recolor: false,
                                            });
                                        }
                                    }
                                    let mut produced = Vec::with_capacity(wrapped.len());
                                    for (j, seg) in wrapped.into_iter().enumerate() {
                                        let prefix = if j == 0 {
                                            first_prefix
                                        } else {
                                            BranchPrefix::HANG
                                        };
                                        let mut spans =
                                            Vec::with_capacity(seg.line.spans.len() + 1);
                                        spans.push(Span::styled(
                                            prefix.to_string(),
                                            Style::default().fg(border_color),
                                        ));
                                        spans.extend(seg.line.spans);
                                        produced.push(Line::from(spans));
                                    }
                                    return produced;
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
                        BranchPrefix::span(false),
                        Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                    ]));
                }
            }
            if tool_name == "Bash" && *shell_backgrounded && started_at.is_some() {
                lines.push(Line::from(vec![
                    BranchPrefix::span(false),
                    Span::styled(
                        BACKGROUND_RUNNING_STATUS,
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if let Some(timeout) = shell_timeout_text(*timeout_ms) {
                    lines.push(Line::from(vec![
                        Span::raw(BranchPrefix::HANG),
                        Span::styled(timeout, Style::default().fg(theme::cur().inactive)),
                    ]));
                }
            } else if tool_name == "Bash"
                && matches!(
                    *phase,
                    crate::ui::message_view::ToolPhase::Queued
                        | crate::ui::message_view::ToolPhase::AwaitingApproval
                )
            {
                lines.push(Line::from(vec![
                    BranchPrefix::span(false),
                    Span::styled(
                        if *phase == crate::ui::message_view::ToolPhase::AwaitingApproval {
                            "Waiting for permission…"
                        } else {
                            "Waiting…"
                        },
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if source_agent_id.is_none() {
                    lines.push(Line::from(vec![
                        Span::raw(BranchPrefix::HANG),
                        Span::styled(
                            CONTROL_B_BACKGROUND_HINT,
                            Style::default().fg(theme::cur().inactive),
                        ),
                    ]));
                }
            } else if tool_name == "Bash"
                && *phase == crate::ui::message_view::ToolPhase::Running
                && started_at.is_some()
            {
                // 执行一开始就占住状态行，消除 Queued→Running 时「上一分支已退出、
                // 本分支尚未进入」的空档。但耗时秒数与 ctrl+b 提示仍等满 2 秒才出现：
                // 前 2 秒只显示裸 `Running…`，让用户知道命令确实在跑。
                let past_threshold =
                    started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2));
                lines.push(Line::from(vec![
                    BranchPrefix::span(false),
                    Span::styled(
                        if past_threshold {
                            shell_running_text(started_at.unwrap(), *timeout_ms)
                        } else {
                            SHELL_RUNNING_TEXT_PREFIX.to_string()
                        },
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
                if past_threshold && source_agent_id.is_none() {
                    lines.push(Line::from(vec![
                        Span::raw(BranchPrefix::HANG),
                        Span::styled(
                            CONTROL_B_BACKGROUND_HINT,
                            Style::default().fg(theme::cur().inactive),
                        ),
                    ]));
                }
            }
            if !is_running && empty_output && !*is_error {
                lines.push(Line::from(vec![
                    BranchPrefix::span(false),
                    Span::styled(
                        if detail_mode { "(No output)" } else { "Done" },
                        Style::default().fg(theme::cur().inactive),
                    ),
                ]));
            }
            if tool_name == "Bash" && !is_running && !*shell_backgrounded {
                if let Some(timeout) = shell_timeout_text(*timeout_ms) {
                    lines.push(Line::from(vec![
                        BranchPrefix::span(false),
                        Span::styled(timeout, Style::default().fg(theme::cur().inactive)),
                    ]));
                }
            }
            // 普通页面是否内联展示 diff 由 `diff_visible`（配置 `diff_enabled`）决定，
            // 详细模式（Ctrl+O）下恒展示——这是该标志**唯一**的消费点，
            // 不得把它并进 detail_mode，否则会顺带展开 shell 输出 / reasoning / 工具折叠。
            if detail_mode || diff_visible {
                if let Some(ref diff_input) = diff_input {
                    // diff 内容宽度需扣除一级分支前缀宽度
                    let diff_width = width.saturating_sub(BranchPrefix::WIDTH);
                    let widget_theme = theme::widgets_theme();
                    let prefix_style = Style::default().fg(if *is_error {
                        theme::cur().error
                    } else {
                        theme::cur().inactive
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
                            prefixed
                                .spans
                                .insert(0, Span::styled(BranchPrefix::HANG, prefix_style));
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
                            let prefix = if i == 0 {
                                BranchPrefix::PREFIX
                            } else {
                                BranchPrefix::HANG
                            };
                            prefixed.spans.insert(0, Span::styled(prefix, prefix_style));
                            lines.push(prefixed);
                        }
                    }
                }
            }
            // hook 行在工具输出/diff 之后，随工具块一起折叠与滚动。
            append_hook_rows(&mut lines, hook_rows, detail_mode, hook_rows_visible, width);
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
                        hook_rows_visible,
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
                                    BranchPrefix::PREFIX,
                                    Style::default().fg(theme::cur().inactive),
                                    Style::default().fg(theme::cur().inactive),
                                    BranchPrefix::content_width(width),
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
            tone,
            body_tone,
            detail_only,
            branch_prefix,
            ..
        } => {
            // 仅详细模式可见的提示（hook 成功汇总 / 异步完成 / hook 自身失败）：
            // 普通模式整行不渲染。
            if *detail_only && !detail_mode {
                return (Vec::new(), link_hits);
            }
            let active_content = if detail_mode && detail_content.is_some() {
                detail_content.as_deref().unwrap()
            } else {
                content.as_str()
            };
            // 会话级 hook 结果行：行首 `⎿` 与工具结果同列，整行按语义色，无
            // `●`/`·`/`⚠` 前缀（规格见 spec/global/domains/hooks-display.md）。
            // hook 展示关闭时整行不渲染——VM 里的行数据不动，开关切回即恢复。
            if *branch_prefix {
                if !hook_rows_visible {
                    return (Vec::new(), link_hits);
                }
                let color = match body_tone {
                    Some(NoteTone::Neutral) | None => theme::cur().inactive,
                    Some(tone) => note_tone_color(*tone),
                };
                let mut lines = Vec::new();
                for line in active_content
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                {
                    push_hook_line(&mut lines, line, color, width);
                }
                return (lines, link_hits);
            }
            // 后台任务通知：`●` + 状态色，对齐参考实现的
            // `UserAgentNotificationMessage`——`<Text color={color}>●</Text> {summary}`，
            // **只有圆点着色**，正文用默认前景色。普通系统提示（tone 为 None）
            // 仍走下方的缩进/灰色分支。
            if let Some(tone) = tone {
                let dot_color = note_tone_color(*tone);
                let mut lines = Vec::new();
                for line in active_content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    // 圆点已作为 span 放进行首，`first_prefix` 必须传空串：
                    // 传 `● ` 会与行内 span 叠加成 `● ● …`。
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        Line::from(vec![
                            Span::styled(theme::SYMBOL_DOT_SPACED, Style::default().fg(dot_color)),
                            Span::styled(
                                sanitize_display_text(line),
                                Style::default().fg(theme::cur().text),
                            ),
                        ]),
                        "",
                        "  ",
                        Style::default().fg(theme::cur().inactive),
                        width.saturating_sub(2).max(20),
                    );
                }
                return (lines, Vec::new());
            }
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
                } else if let Some(rest) = BranchPrefix::strip_prefix(line) {
                    let l = Line::from(Span::styled(
                        sanitize_display_text(rest),
                        Style::default().fg(theme::cur().inactive),
                    ));
                    push_wrapped_line_keep_lead(
                        &mut lines,
                        l,
                        BranchPrefix::PREFIX,
                        BranchPrefix::HANG,
                        Style::default().fg(theme::cur().inactive),
                        BranchPrefix::content_width(width),
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
                    // 显式语义色优先：调用方（如「hooks 未加载」提示）已经知道这是
                    // 警告，不应再靠正文关键词猜颜色。
                    let text_color = match body_tone {
                        Some(tone) => note_tone_color(*tone),
                        None => {
                            let is_error = line.contains("❌")
                                || line.contains("失败")
                                || line.contains("错误");
                            let is_warn = line.contains("⚠") || line.contains("已中断");
                            if is_error {
                                theme::cur().error
                            } else if is_warn {
                                theme::cur().warning
                            } else {
                                theme::cur().inactive
                            }
                        }
                    };
                    // 首行给语义符号，续行按符号宽度缩进；普通系统提示仍是 `· `。
                    let (prefix, prefix_style) = match *body_tone {
                        Some(_) if lines.is_empty() => (
                            note_tone_prefix(*body_tone),
                            Style::default().fg(text_color),
                        ),
                        Some(_) => ("  ", Style::default().fg(text_color)),
                        None => ("· ", Style::default().fg(theme::cur().subtle)),
                    };
                    push_prefixed_text(
                        &mut lines,
                        line,
                        prefix,
                        prefix_style,
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
            thinking,
            ..
        } => {
            let mut lines = Vec::new();

            // 一等公民：组自带思考行（活跃 → Thinking，冻结 → Thought），不再依赖 AssistantBubble。
            if let Some(t) = thinking {
                let active = t.active && !detail_mode;
                // 普通模式只留摘要行：进行中走秒，结束后落成 `Thought for Ns`，不吐正文。
                let live_ms = if active {
                    let base = t.duration_ms.unwrap_or(0);
                    let extra = t
                        .active_since
                        .map(|anchor| anchor.elapsed().as_millis() as u64)
                        .unwrap_or(0);
                    Some(base + extra)
                } else {
                    None
                };
                // 活跃态没有可展开的正文，不给展开提示
                let hint = if detail_mode || active {
                    ""
                } else {
                    " (ctrl+o to expand)"
                };
                let (prefix, timing) = if let Some(ms) = live_ms {
                    let secs = ms.max(1000) / 1000;
                    // 活跃态恒显 `∴ ` 前缀（不再做 500ms 显隐闪烁）
                    let dot = "∴ ".to_string();
                    (dot, format!("Thinking for {secs}s"))
                } else {
                    // 冻结后不再带符号，只保留缩进（与活跃行对齐）
                    (
                        "  ".to_string(),
                        match t.duration_ms {
                            Some(ms) => format!("Thought for {}s", ms.div_ceil(1000).max(1)),
                            None => format!("Thought for {} chars", t.text.chars().count()),
                        },
                    )
                };
                let actions = t
                    .action_summary
                    .as_deref()
                    .map(|summary| {
                        // 现在时只在真正活跃时用；留存窗口内已结束，计数回归过去时
                        if active {
                            format!(", {}", present_tense_summary(summary))
                        } else {
                            format!(", {summary}")
                        }
                    })
                    .unwrap_or_default();
                let ellipsis = if live_ms.is_some() { "…" } else { "" };
                let title_color = if live_ms.is_some() {
                    theme::cur().text_soft
                } else {
                    theme::cur().inactive
                };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{timing}{actions}{ellipsis}{hint}"),
                    Style::default().fg(title_color),
                )));
                // 详细模式：思考正文与 AssistantBubble 保持同一条渲染管线
                // （markdown 解析 + DIM 压暗 + 折行），避免组内/组外观感不一致。
                if detail_mode {
                    push_thinking_body(&mut lines, &t.text, width);
                }
            }

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
                            BranchPrefix::PREFIX,
                            BranchPrefix::HANG,
                            Style::default().fg(if entry.is_error {
                                theme::cur().error
                            } else {
                                theme::cur().inactive
                            }),
                            BranchPrefix::content_width(width),
                        );
                    }
                }
            } else if detail_mode {
                // 详细模式：显示每条工具的名称和结果
                // Read 工具只显示行数摘要，不展开文件内容
                for entry in tools {
                    let entry_is_error = tool_entry_is_error(entry);
                    let entry_color = if entry_is_error {
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
                            // 与单条 ToolBlock 的详细模式一致：Write/Edit/Read 显示完整绝对路径
                            let verbose = verbose_tool_args(&entry.tool_name, args);
                            let summary =
                                tool_args_header(&entry.tool_name, &verbose, max_args_width);
                            let header_line = lines.len().saturating_sub(1);
                            let args_start_g = indicator.graphemes(true).count()
                                + 1
                                + entry.display_name.graphemes(true).count()
                                + 1;
                            // 只读工具组里的 Read 路径同样可点：命中区只覆盖路径本身
                            if let (Some(url), Some(path_text)) = (
                                tool_file_url(&entry.tool_name, args),
                                tool_path_text(&entry.tool_name, args),
                            ) {
                                let path_display = verbose_display_path(path_text)
                                    .unwrap_or_else(|| path_text.to_string());
                                let path_g = path_display.graphemes(true).count();
                                let visible_g = summary.graphemes(true).count();
                                let truncated = verbose.graphemes(true).count() > visible_g;
                                let visible =
                                    path_g.min(visible_g.saturating_sub(usize::from(truncated)));
                                if visible > 0 {
                                    link_hits.push(cc_widgets::markdown::LinkHit {
                                        line: header_line,
                                        g_start: args_start_g,
                                        g_end: args_start_g + visible,
                                        url,
                                        recolor: false,
                                    });
                                }
                            }
                            if let Some(last_line) = lines.last_mut() {
                                if entry.tool_name == "Bash" {
                                    last_line.spans.extend([
                                        Span::raw("("),
                                        Span::styled(
                                            summary,
                                            Style::default().fg(theme::cur().text_soft),
                                        ),
                                        Span::raw(")"),
                                    ]);
                                } else {
                                    last_line.spans.push(Span::styled(
                                        format!("({})", summary),
                                        Style::default().fg(theme::cur().text_soft),
                                    ));
                                }
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
                                BranchPrefix::span(entry.is_error),
                                Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                            ]));
                        }
                    } else if entry.tool_name == "Glob"
                        && !entry.content.is_empty()
                        && !entry.is_error
                    {
                        if let Some(summary) = glob_summary(&entry.content) {
                            lines.push(Line::from(vec![
                                BranchPrefix::span(false),
                                Span::styled(summary, Style::default().fg(theme::cur().inactive)),
                            ]));
                        }
                        for line in entry.content.lines() {
                            push_prefixed_text(
                                &mut lines,
                                line,
                                BranchPrefix::HANG,
                                Style::default().fg(theme::cur().inactive),
                                Style::default().fg(theme::cur().inactive),
                                BranchPrefix::content_width(width),
                            );
                        }
                    } else if entry.tool_name == "Bash" {
                        let empty_output = entry.content.trim().is_empty()
                            || (!entry_is_error
                                && strip_exit_code_suffix(&entry.content).is_empty());
                        if empty_output && !entry_is_error {
                            lines.push(Line::from(vec![
                                BranchPrefix::span(false),
                                Span::styled(
                                    "(No output)",
                                    Style::default().fg(theme::cur().inactive),
                                ),
                            ]));
                            continue;
                        }
                        let mut error_header = String::new();
                        let bash_lines = transform_bash_output_lines(
                            &entry.content,
                            parse_exit_code(&entry.content),
                            &mut error_header,
                        );
                        let content_width = BranchPrefix::content_width(width);
                        let border_color = if entry_is_error {
                            theme::cur().error
                        } else {
                            theme::cur().inactive
                        };
                        for item in bash_lines {
                            let text_color = if item.is_error_header || entry_is_error {
                                theme::cur().error
                            } else {
                                theme::cur().text_soft
                            };
                            push_prefixed_text(
                                &mut lines,
                                item.text.as_ref(),
                                item.prefix,
                                Style::default().fg(border_color),
                                Style::default().fg(text_color),
                                content_width,
                            );
                        }
                    } else if !entry.content.is_empty() {
                        for (i, line) in entry.content.lines().enumerate() {
                            let first_prefix = if i == 0 {
                                BranchPrefix::PREFIX
                            } else {
                                BranchPrefix::HANG
                            };
                            push_prefixed_text(
                                &mut lines,
                                line,
                                first_prefix,
                                Style::default().fg(if entry.is_error {
                                    theme::cur().error
                                } else {
                                    theme::cur().inactive
                                }),
                                Style::default().fg(theme::cur().inactive),
                                BranchPrefix::content_width(width),
                            );
                        }
                    }
                }
            } else {
                // 折叠态：显示**纯动作行**（PRD §2.3，仅前面无 thinking bubble 的独立只读组）
                // 以及出错工具的错误摘要。
                // 组已自带思考行时，摘要已在标题里，折叠态不再重复一行
            if let Some(summary) = standalone_action.as_ref().filter(|_| thinking.is_none()) {
                    // 该行独立成行、行首无符号，首字母必须大写才与同族的
                    // `Thought for 3s` / `Brewed for 1m` 摘要行一致；
                    // 文案层因续写场景需要仍是小写 `read N file`，故在此处转换。
                    lines.push(Line::from(Span::styled(
                        format!("  {} (ctrl+o to expand)", uppercase_first_char(summary)),
                        Style::default().fg(theme::cur().inactive),
                    )));
                }
                for entry in tools {
                    if tool_entry_is_error(entry) && !entry.content.is_empty() {
                        let bash_summary;
                        let mut error_header = String::new();
                        let content = if entry.tool_name == "Bash" {
                            bash_summary = transform_bash_output_lines(
                                &entry.content,
                                parse_exit_code(&entry.content),
                                &mut error_header,
                            )
                            .iter()
                            .map(|line| line.text.as_ref())
                            .collect::<Vec<_>>()
                            .join("\n");
                            bash_summary.as_str()
                        } else {
                            &entry.content
                        };
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
#[path = "bash_status_test.rs"]
mod bash_status_tests;

#[cfg(test)]
#[path = "bash_file_inspect_test.rs"]
mod bash_file_inspect_tests;

#[cfg(test)]
mod tests {
    use super::*;
    include!("message_render_test.rs");
}
