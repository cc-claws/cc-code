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
            text: error_header_buf.as_str(),
            is_error_header: true,
        });

        let clean_body = strip_exit_code_suffix(content);
        if !clean_body.is_empty() {
            for line in clean_body.lines() {
                lines.push(ToolDisplayLine {
                    prefix: INDENT_PREFIX,
                    text: line,
                    is_error_header: false,
                });
            }
        }
    } else {
        let body_lines: Vec<&str> = if content.is_empty() {
            Vec::new()
        } else {
            content.lines().collect()
        };
        for (i, line) in body_lines.iter().enumerate() {
            lines.push(ToolDisplayLine {
                prefix: if i == 0 { BRANCH_PREFIX } else { INDENT_PREFIX },
                text: line,
                is_error_header: false,
            });
        }
    }

    lines
}

#[cfg(test)]
#[path = "bash_test.rs"]
mod bash_test;
