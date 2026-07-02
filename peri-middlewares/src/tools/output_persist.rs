use std::{env, fs, path::PathBuf};

/// 当输出被截断时，将完整内容写入临时文件。
/// 返回追加到截断信息后的提示字符串。
/// 文件路径：`{temp_dir}/peri-tool-output-{uuid}.txt`
pub fn persist_truncated_output(full_content: &str) -> String {
    let id = uuid::Uuid::new_v4();
    let dir = env::temp_dir();
    let file_name = format!("peri-tool-output-{id}.txt");
    let file_path: PathBuf = dir.join(&file_name);

    match fs::write(&file_path, full_content) {
        Ok(_) => {
            let read_path = file_path.to_string_lossy().replace('\\', "/");
            format!(
                "\n\n[Full output saved to {}]\n[If your answer depends on omitted content, you must call the Read tool with file_path=\"{}\" before answering. Use offset/limit for large files.]",
                file_path.display(),
                read_path
            )
        }
        Err(e) => format!(
            "\n\n[Failed to save full output to {}: {e}]",
            file_path.display()
        ),
    }
}

/// 工具输出公共截断阈值：与 Bash 工具保持一致。
pub const MAX_OUTPUT_CHARS: usize = 100_000;
pub const MAX_OUTPUT_LINES: usize = 2_000;

/// Bash 输出给模型的预览上限。完整输出仍会落盘供 Read 按需查看。
pub const MAX_SHELL_OUTPUT_CHARS: usize = 20_000;
pub const MAX_SHELL_OUTPUT_LINES: usize = 50;

/// 字节级截断，钳位到字符边界，避免切割 UTF-8 多字节字符。
pub fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn tail_from_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut start = s.len().saturating_sub(max_bytes);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

fn truncate_bytes_head_tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let omitted = s.len().saturating_sub(max_bytes);
    let marker = format!("\n\n... [{omitted} bytes omitted, showing head and tail] ...\n\n");
    if max_bytes <= marker.len() + 2 {
        return truncate_bytes(s, max_bytes);
    }
    let content_budget = max_bytes - marker.len();
    let head_budget = content_budget / 2;
    let tail_budget = content_budget - head_budget;
    let head = truncate_bytes(s, head_budget);
    let tail = tail_from_char_boundary(s, tail_budget);
    format!("{head}{marker}{tail}")
}

/// Bash 输出截断：只给模型小预览，完整日志写入临时文件供 Read 按需读取。
pub fn truncate_shell_output(output: &str) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    if lines.len() > MAX_SHELL_OUTPUT_LINES {
        let total_lines = lines.len();
        let persist_hint = persist_truncated_output(output);
        let head_count = MAX_SHELL_OUTPUT_LINES / 2;
        let tail_count = MAX_SHELL_OUTPUT_LINES - head_count;
        let head: Vec<&str> = lines.iter().take(head_count).copied().collect();
        let tail: Vec<&str> = lines
            .iter()
            .skip(total_lines - tail_count)
            .copied()
            .collect();
        let mut result = head.join("\n");
        result.push_str(&format!(
            "\n\n... [{} lines truncated from preview, showing first {} and last {} of {} total lines] ...\n\n",
            total_lines - MAX_SHELL_OUTPUT_LINES,
            head_count,
            tail_count,
            total_lines
        ));
        result.push_str(&tail.join("\n"));
        if result.len() > MAX_SHELL_OUTPUT_CHARS {
            result = truncate_bytes_head_tail(&result, MAX_SHELL_OUTPUT_CHARS);
            result.push_str(&format!(
                "\n\n[Output truncated: preview exceeded {} byte limit]",
                MAX_SHELL_OUTPUT_CHARS
            ));
        }
        result.push_str(&persist_hint);
        return result;
    }
    if output.len() > MAX_SHELL_OUTPUT_CHARS {
        let persist_hint = persist_truncated_output(output);
        let truncated = truncate_bytes_head_tail(output, MAX_SHELL_OUTPUT_CHARS);
        return format!(
            "{}\n\n[Output truncated: exceeds {} byte preview limit]{}",
            truncated, MAX_SHELL_OUTPUT_CHARS, persist_hint
        );
    }
    output.to_string()
}

/// 通用工具输出截断：行数或字节数任一超阈触发 head/tail + 字节兜底。
/// 任何工具（Grep/Glob/FolderOperations 等）在返回前都应调用此函数做兜底，
/// 防止单条 tool_result 撑爆 LLM context window（issue #47）。
pub fn truncate_tool_output(output: &str) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    if lines.len() > MAX_OUTPUT_LINES {
        let total_lines = lines.len();
        let persist_hint = persist_truncated_output(output);
        let head_count = MAX_OUTPUT_LINES / 2;
        let tail_count = MAX_OUTPUT_LINES - head_count;
        let head: Vec<&str> = lines.iter().take(head_count).copied().collect();
        let tail: Vec<&str> = lines
            .iter()
            .skip(total_lines - tail_count)
            .copied()
            .collect();
        let mut result = head.join("\n");
        result.push_str(&format!(
            "\n\n... [{} lines truncated, showing head {} and tail {} of {} total lines] ...\n\n",
            total_lines - MAX_OUTPUT_LINES,
            head_count,
            tail_count,
            total_lines
        ));
        result.push_str(&tail.join("\n"));
        result.push_str(&persist_hint);
        if result.len() > MAX_OUTPUT_CHARS {
            let truncated = truncate_bytes(&result, MAX_OUTPUT_CHARS);
            return format!(
                "{}\n\n[Output truncated: exceeds {} byte limit]{}",
                truncated, MAX_OUTPUT_CHARS, persist_hint
            );
        }
        return result;
    }
    if output.len() > MAX_OUTPUT_CHARS {
        let persist_hint = persist_truncated_output(output);
        let truncated = truncate_bytes(output, MAX_OUTPUT_CHARS);
        return format!(
            "{}\n\n[Output truncated: exceeds {} byte limit]{}",
            truncated, MAX_OUTPUT_CHARS, persist_hint
        );
    }
    output.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_saved_path(hint: &str) -> String {
        let prefix = "saved to ";
        let path_start = hint.find(prefix).unwrap() + prefix.len();
        let path_end = hint[path_start..]
            .find(']')
            .map(|i| path_start + i)
            .unwrap_or(hint.len());
        hint[path_start..path_end].to_string()
    }

    #[test]
    fn test_persist_writes_file_and_returns_hint() {
        let content = "line1\nline2\nline3";
        let hint = persist_truncated_output(content);
        // 提示应包含文件名
        assert!(
            hint.contains("peri-tool-output-"),
            "hint should contain filename: {hint}"
        );
        // 提示应引导用户使用 Read 工具
        assert!(
            hint.contains("Read"),
            "hint should guide to use Read tool: {hint}"
        );
        assert!(
            hint.contains("must call the Read tool with file_path="),
            "hint should be an actionable next step: {hint}"
        );
        // 从提示中提取文件路径并验证内容
        let path = extract_saved_path(&hint);
        let saved = fs::read_to_string(path).unwrap();
        assert_eq!(saved, content);
        fs::remove_file(extract_saved_path(&hint)).ok();
    }

    #[test]
    fn test_persist_empty_string() {
        let hint = persist_truncated_output("");
        // 空内容也应生成包含路径的提示
        assert!(
            hint.contains("Read"),
            "empty content should also produce hint: {hint}"
        );
        // 验证空文件确实被写入，并清理
        let path = extract_saved_path(&hint);
        let saved = fs::read_to_string(path).unwrap();
        assert_eq!(saved, "");
        fs::remove_file(extract_saved_path(&hint)).ok();
    }
}
