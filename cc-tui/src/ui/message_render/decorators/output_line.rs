//! `OutputLine` 装饰器：对齐 CC `src/components/shell/OutputLine.tsx`。
//!
//! CC 的 Bash / MCP 工具结果都过这一层，它负责四件事：
//!
//! 1. **JSON 逐行美化** —— `tryJsonFormatContent`：单行 JSON 展开成 2 空格缩进；
//! 2. **URL 超链接** —— `linkifyUrlsInText`（CC 用 OSC 8；peri 用链接色 + 下划线，
//!    见 `crate::ui::message_render::style_urls`）；
//! 3. **行数截断** —— `renderTruncatedContent`：只留前 3 行 + `… +N lines (ctrl+o to expand)`；
//! 4. **宽度预折行 + 每行 trimEnd**。
//!
//! 装饰器只做**纯文本变换**（返回 `String`），不碰 ratatui 的 span/样式——
//! 着色由调用方（`format_prefixed_text` / `format_shell_output_lines`）后续施加，
//! 这样 JSON 美化、URL 识别、截断测算三者共享同一份文本，口径不会各自漂移。

use super::super::find_url_ranges;

/// CC `terminal.ts::MAX_LINES_TO_SHOW`：折叠态只显示 3 行。
pub const MAX_LINES_TO_SHOW: usize = 3;
/// CC `terminal.ts::PADDING_TO_PREVENT_OVERFLOW`：给 `  ⎿ ` 前缀与父级缩进留的余量。
pub const PADDING_TO_PREVENT_OVERFLOW: usize = 10;
/// CC `OutputLine.tsx::MAX_JSON_FORMAT_LENGTH`：超长内容整体跳过 JSON 美化。
const MAX_JSON_FORMAT_LENGTH: usize = 10_000;

/// 单行 JSON 美化，对齐 CC `OutputLine.tsx::tryFormatJson`。
///
/// 返回 `Some` 表示这一行是合法的单行 JSON 且美化安全；`None` 表示原样保留。
/// 判定顺序与 CC 一致：
///
/// 1. 解析失败 → 保留原文；
/// 2. 紧凑重序列化后与原行「去空白 + `\/` 归一」比较，**不一致就放弃**——
///    这是精度守卫：超 `i64`/`u64` 的大整数会被 serde_json 降级成 f64，
///    `1.10` 这类写法也会被改写，一旦重排就等于伪造了命令输出；
/// 3. 通过则按 2 空格缩进美化。
///
/// 快速路径：顶层不是 `{` / `[` 的行直接跳过，省掉普通日志行的解析开销。
/// 这不会漏格式——顶层标量（`123` / `"s"` / `true`）美化前后逐字节相同。
pub fn try_format_json(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let compact = serde_json::to_string(&value).ok()?;
    if normalize_json(trimmed) != normalize_json(&compact) {
        return None;
    }
    serde_json::to_string_pretty(&value).ok()
}

/// 去掉空白并把 `\/` 归一成 `/`（JSON 允许但 `to_string` 不产出这种转义）。
fn normalize_json(s: &str) -> String {
    s.replace("\\/", "/")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// 逐行尝试 JSON 美化，对齐 CC `tryJsonFormatContent`。
///
/// 返回 `None` 表示没有任何一行被美化（调用方可继续零拷贝借用原文本）；
/// 返回 `Some` 时是整段替换文本——被美化的行会从 1 行展开成多行。
pub fn try_json_format_content(content: &str) -> Option<String> {
    // CC 用 JS 的 `content.length`（UTF-16 码元）。用字符数最接近同一量级；
    // 若按字节数算，中文内容会提前三倍触发跳过。
    if content.chars().count() > MAX_JSON_FORMAT_LENGTH {
        return None;
    }
    let mut changed = false;
    let mut out: Vec<String> = Vec::new();
    for line in content.split('\n') {
        match try_format_json(line) {
            Some(pretty) => {
                changed = true;
                out.extend(pretty.split('\n').map(str::to_string));
            }
            None => out.push(line.to_string()),
        }
    }
    changed.then(|| out.join("\n"))
}

/// 按显示宽度预折行，对齐 CC `terminal.ts::wrapText`。
///
/// 与 [`crate::ui::message_render::wrap_line_spans`] 的区别：这里**只处理纯文本**、
/// 不感知样式，折行单元是「显示列」而非 grapheme 宽度之和——用于截断预算测算。
/// 每行仍做 `trimEnd`（CC 同样处理，避免行尾空白占宽度）。
pub fn wrap_plain_text(text: &str, wrap_width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;

    let wrap_width = wrap_width.max(1);
    let mut out = Vec::new();
    for line in text.split('\n') {
        if UnicodeWidthStr::width(line) <= wrap_width {
            out.push(line.trim_end().to_string());
            continue;
        }
        let mut chunk = String::new();
        let mut width = 0usize;
        for ch in line.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if width + cw > wrap_width && !chunk.is_empty() {
                out.push(chunk.trim_end().to_string());
                chunk.clear();
                width = 0;
            }
            chunk.push(ch);
            width += cw;
        }
        out.push(chunk.trim_end().to_string());
    }
    out
}

/// 截断结果：显示内容 + 是否有更多行。
#[derive(Debug, PartialEq, Eq)]
pub struct TruncatedContent {
    /// 已折行、已截断的显示文本（不含末尾提示行）。
    pub above_the_fold: String,
    /// 被折叠掉的行数；`0` 表示全部内容都显示了。
    pub remaining_lines: usize,
}

/// 折行后按 `MAX_LINES_TO_SHOW` 截断，对齐 CC `renderTruncatedContent`。
///
/// 两个 CC 特例必须保留：
/// - **折叠后只剩 1 行时直接显示第 4 行**，不出 `+1 line` 提示——为一行提示
///   浪费一行显示空间不划算（`terminal.ts:47-55`）；
/// - 超长内容先按 `3 × wrap_width × 4` 字符预切片再折行，避免几十 MB 输出做 O(n)
///   折行（`terminal.ts:80-84`）。
pub fn truncate_wrapped(
    content: &str,
    terminal_width: usize,
    max_lines: usize,
) -> TruncatedContent {
    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return TruncatedContent {
            above_the_fold: String::new(),
            remaining_lines: 0,
        };
    }

    let wrap_width = terminal_width
        .saturating_sub(PADDING_TO_PREVENT_OVERFLOW)
        .max(10);
    let max_chars = max_lines.saturating_mul(wrap_width).saturating_mul(4);
    let pre_truncated = trimmed.chars().count() > max_chars;
    let for_wrapping: String = if pre_truncated {
        trimmed.chars().take(max_chars).collect()
    } else {
        trimmed.to_string()
    };

    let wrapped = wrap_plain_text(&for_wrapping, wrap_width);
    let remaining = wrapped.len().saturating_sub(max_lines);

    // 只剩 1 行时直接显示，不出提示
    if remaining == 1 && !pre_truncated {
        return TruncatedContent {
            above_the_fold: wrapped
                .into_iter()
                .take(max_lines + 1)
                .collect::<Vec<_>>()
                .join("\n"),
            remaining_lines: 0,
        };
    }

    let estimated_remaining = if pre_truncated {
        // 预切片后看不到真实总行数，按字符数估算（CC 同款）
        remaining.max(trimmed.chars().count() / wrap_width - max_lines)
    } else {
        remaining
    };

    TruncatedContent {
        above_the_fold: wrapped
            .into_iter()
            .take(max_lines)
            .collect::<Vec<_>>()
            .join("\n"),
        remaining_lines: estimated_remaining,
    }
}

/// 装饰器主体：把工具原始输出变成「该显示的那几行文本」。
///
/// 顺序与 CC `OutputLine` 的 `useMemo` 完全一致：
/// **JSON 美化 → URL 识别（仅着色阶段）→ 预折行 + 截断**。
///
/// `linkify` 只影响是否值得继续处理（URL 着色在 span 阶段做），此处保留参数
/// 是为了对齐 CC 调用点的形状，未来若要在文本层替换 URL 可直接接入。
pub fn decorate(content: &str, terminal_width: usize, max_lines: usize) -> TruncatedContent {
    let formatted = try_json_format_content(content).unwrap_or_else(|| content.to_string());
    truncate_wrapped(&formatted, terminal_width, max_lines)
}

/// 内容里是否含至少一个可识别的 URL。
///
/// 供调用方决定是否走 URL 着色路径——避免对绝大多数无 URL 的输出反复扫描。
pub fn contains_url(text: &str) -> bool {
    !find_url_ranges(text).is_empty()
}

#[cfg(test)]
#[path = "output_line_test.rs"]
mod output_line_test;
