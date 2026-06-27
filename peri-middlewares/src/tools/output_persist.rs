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
        Ok(_) => format!(
            "\n\n[Full output saved to {} — use Read tool to view complete content]",
            file_path.display()
        ),
        Err(e) => format!(
            "\n\n[Failed to save full output to {}: {e}]",
            file_path.display()
        ),
    }
}

/// 工具输出公共截断阈值：与 Bash 工具保持一致。
pub const MAX_OUTPUT_CHARS: usize = 100_000;
pub const MAX_OUTPUT_LINES: usize = 2_000;

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
        // 从提示中提取文件路径并验证内容
        let prefix = "saved to ";
        let suffix = " — use Read";
        let path_start = hint.find(prefix).unwrap() + prefix.len();
        let path_end = hint[path_start..]
            .find(suffix)
            .map(|i| path_start + i)
            .unwrap_or(hint.len());
        let path = &hint[path_start..path_end];
        let saved = fs::read_to_string(path).unwrap();
        assert_eq!(saved, content);
        fs::remove_file(path).ok();
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
        let prefix = "saved to ";
        let suffix = " — use Read";
        let path_start = hint.find(prefix).unwrap() + prefix.len();
        let path_end = hint[path_start..]
            .find(suffix)
            .map(|i| path_start + i)
            .unwrap_or(hint.len());
        let path = &hint[path_start..path_end];
        let saved = fs::read_to_string(path).unwrap();
        assert_eq!(saved, "");
        fs::remove_file(path).ok();
    }
}
