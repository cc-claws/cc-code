//! hook 原因文案的收敛工具。
//!
//! reason 最终落在 TUI 的单行 hook 结果行里，而它的来源（stderr、HTTP 响应体、
//! 解析错误）可能带换行、制表符与 ANSI 转义：不收敛会把终端控制序列带进渲染层，
//! 或让「一条原因」撑成多行。执行失败（`executor`）与输出非法（`output_parser`）
//! 两条路径共用这里的口径，避免复制粘贴导致某一路漏掉 `strip_ansi`。

use crate::tools::output_filter::strip_ansi;

/// 单行原因正文的最大字符数（按字符而非字节截断，CJK 不会截在半个字上）。
///
/// hook 的 stderr 常是整段 Node / Python 堆栈，单行里塞满功能名与路径毫无价值；
/// 100 字符足够放下「哪个模块没找到 / 哪个命令超时」，又不至于把一行撑满视口。
const REASON_MAX_CHARS: usize = 100;

/// 把原始输出收敛成单行短文本：剥离 ANSI 转义 → 所有空白折叠成单个空格 → 截断。
pub(crate) fn single_line_reason(detail: &str) -> String {
    let cleaned = strip_ansi(detail);
    collapse_and_truncate(&cleaned)
}

/// 从多行原始输出里挑出「最后一条有意义的信息」。
///
/// 崩溃堆栈的信息密度是反着来的：最具体的失败原因在中间
/// （`Error: Cannot find module …`），后面只剩一串 `at …` 栈帧，末尾往往还是
/// `Node.js v20.x` 这类版本尾巴。整段折叠成一行只会剩下噪音，所以：
///
/// 1. 先剥 ANSI——否则 `\x1b[31mError:` 匹配不上 `Error:`；
/// 2. 优先取**最后一行**包含 `Error:` 的文本；
/// 3. 没有 `Error:` 就退化成最后一条非空行；
/// 4. 折叠空白并截断到 [`REASON_MAX_CHARS`]。
fn last_meaningful_line(detail: &str) -> String {
    let cleaned = strip_ansi(detail);
    let mut last_non_empty = "";
    let mut last_error_line = "";
    for line in cleaned.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        last_non_empty = trimmed;
        if trimmed.contains("Error:") {
            last_error_line = trimmed;
        }
    }
    let picked = if last_error_line.is_empty() {
        last_non_empty
    } else {
        last_error_line
    };
    collapse_and_truncate(picked)
}

/// 空白（换行 / 制表符 / 连续空格）折叠成单个空格，再按字符截断。
fn collapse_and_truncate(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(REASON_MAX_CHARS)
        .collect()
}

/// 拼一段给用户看的 hook 失败原因：`<prefix>: <最后一条有意义的信息>`。
///
/// detail 收敛后为空时只保留 prefix，避免出现悬空的 `: `。
pub(crate) fn hook_error_reason(prefix: &str, detail: &str) -> String {
    let short = last_meaningful_line(detail);
    if short.is_empty() {
        return prefix.to_string();
    }
    format!("{prefix}: {short}")
}

#[cfg(test)]
#[path = "reason_test.rs"]
mod tests;
