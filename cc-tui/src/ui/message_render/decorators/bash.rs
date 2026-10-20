use super::output_line::try_json_format_content;
use super::ToolDisplayLine;

pub const BRANCH_PREFIX: &str = "  ⎿ ";
pub const INDENT_PREFIX: &str = "    ";

/// 从 Bash 工具输出中解析 exit code。
///
/// 匹配格式：`[Exit code: N]` 或 `[Command completed with exit code N]`
/// 解析失败返回 None。
pub fn parse_exit_code(content: &str) -> Option<i32> {
    if let Some(pos) = content.rfind("[Exit code: ") {
        let rest = &content[pos + "[Exit code: ".len()..];
        if let Some(end) = rest.find(']') {
            if let Ok(code) = rest[..end].trim().parse::<i32>() {
                return Some(code);
            }
        }
    }
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

/// 剥离命令输出末尾由底层追加的退出码标记及紧邻的空行。
pub fn strip_exit_code_suffix(content: &str) -> &str {
    let trimmed = content.trim_end();
    if let Some(pos) = trimmed.rfind("[Exit code: ") {
        if trimmed[pos..].ends_with(']') {
            return trimmed[..pos].trim_end();
        }
    }
    if let Some(pos) = trimmed.rfind("[Command completed with exit code ") {
        if trimmed[pos..].ends_with(']') {
            return trimmed[..pos].trim_end();
        }
    }
    trimmed
}

/// 将 Bash 命令输出转换为结构化渲染行。
///
/// 1. 失败时（非零退出码）：
///    - 首行提升为 `Error: Exit code {code}` 并挂在树形折角 `  └ `
///    - 剥离末尾冗余的 `[Exit code: {code}]` 标记
///    - 正文（stdout/stderr）作为子内容统一以 `    ` 缩进
/// 2. 成功时（0 或无退出码）：
///    - 首行挂 `  └ `，后续行 `    ` 缩进
///
/// 正文先经 [`try_json_format_content`] 尝试 JSON 美化（对齐 CC
/// `OutputLine` → `tryJsonFormatContent`）：单行 JSON 会展开成 2 空格缩进的多行。
pub fn transform_bash_output_lines<'a>(
    content: &'a str,
    exit_code: Option<i32>,
    error_header_buf: &'a mut String,
) -> Vec<ToolDisplayLine<'a>> {
    let mut lines = Vec::new();

    if let Some(code) = exit_code.filter(|&c| c != 0) {
        *error_header_buf = format!("Error: Exit code {code}");
        lines.push(ToolDisplayLine {
            prefix: BRANCH_PREFIX,
            text: std::borrow::Cow::Borrowed(error_header_buf.as_str()),
            is_error_header: true,
        });

        let clean_body = strip_exit_code_suffix(content);
        if !clean_body.is_empty() {
            push_body_lines(&mut lines, clean_body, true);
        }
    } else if !content.is_empty() {
        push_body_lines(&mut lines, content, false);
    }

    lines
}

/// 把正文行追加成 [`ToolDisplayLine`]。
///
/// `reserve_first` 为真时把第一行留给已有的错误头（`Error: Exit code N`），
/// 正文首行改用缩进前缀。JSON 美化产生的行是 owned（原输出里并不存在），
/// 未美化时继续借用 `body`，零拷贝。
fn push_body_lines<'a>(lines: &mut Vec<ToolDisplayLine<'a>>, body: &'a str, reserve_first: bool) {
    let pretty = try_json_format_content(body);
    match pretty {
        Some(text) => {
            for (i, line) in text.lines().enumerate() {
                lines.push(ToolDisplayLine {
                    prefix: body_prefix(i, reserve_first),
                    text: std::borrow::Cow::Owned(line.to_string()),
                    is_error_header: false,
                });
            }
        }
        None => {
            for (i, line) in body.lines().enumerate() {
                lines.push(ToolDisplayLine {
                    prefix: body_prefix(i, reserve_first),
                    text: std::borrow::Cow::Borrowed(line),
                    is_error_header: false,
                });
            }
        }
    }
}

/// 正文行的树形前缀：只有「整段结果的第一行」用分支折角 `  ⎿ `，
/// 其余一律缩进。`reserve_first` 为真表示首行已被错误头占用，正文全用缩进。
fn body_prefix(index: usize, reserve_first: bool) -> &'static str {
    if index == 0 && !reserve_first {
        BRANCH_PREFIX
    } else {
        INDENT_PREFIX
    }
}

#[cfg(test)]
#[path = "bash_test.rs"]
mod bash_test;
