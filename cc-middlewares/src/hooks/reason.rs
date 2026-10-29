//! hook 原因文案的收敛工具。
//!
//! reason 最终落在 TUI 的单行 hook 结果行里，而它的来源（stderr、HTTP 响应体、
//! 解析错误）可能带换行、制表符与 ANSI 转义：不收敛会把终端控制序列带进渲染层，
//! 或让「一条原因」撑成多行。执行失败（`executor`）与输出非法（`output_parser`）
//! 两条路径共用这里的口径，避免复制粘贴导致某一路漏掉 `strip_ansi`。

use crate::tools::output_filter::strip_ansi;

/// 单行原因正文的最大字符数（按字符而非字节截断，CJK 不会截在半个字上）。
const REASON_MAX_CHARS: usize = 200;

/// 把原始输出收敛成单行短文本：剥离 ANSI 转义 → 所有空白折叠成单个空格 → 截断。
pub(crate) fn single_line_reason(detail: &str) -> String {
    let cleaned = strip_ansi(detail);
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(REASON_MAX_CHARS)
        .collect()
}

/// 拼一段给用户看的 hook 失败原因：`<prefix>: <收敛后的 detail>`。
///
/// detail 收敛后为空时只保留 prefix，避免出现悬空的 `: `。
pub(crate) fn hook_error_reason(prefix: &str, detail: &str) -> String {
    let short = single_line_reason(detail);
    if short.is_empty() {
        return prefix.to_string();
    }
    format!("{prefix}: {short}")
}

#[cfg(test)]
#[path = "reason_test.rs"]
mod tests;
