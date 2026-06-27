use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::resolve_path;

const EDIT_FILE_DESCRIPTION: &str = r#"Performs exact string replacements in files.

Usage:
- You must use your Read tool at least once in the conversation before editing. This tool will fail if you attempt an edit without reading the file
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix
- ALWAYS prefer editing existing files in the codebase. DO NOT create new files unless explicitly required
- The file_path parameter must be an absolute path, not a relative path
- The old_string parameter must match exactly, including all whitespace and indentation
- The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance of old_string
- Use replace_all for replacing and renaming strings across the file

Error handling:
- old_string not found: returns an error indicating the string does not exist in the file
- old_string not unique: returns an error with the count of occurrences, suggesting more context or replace_all
- old_string is empty: returns an error rejecting the operation
- File not found: returns an error indicating the path does not exist"#;

/// Edit tool (replace) - 与 TypeScript replace_tool 对齐
pub struct EditFileTool {
    pub cwd: String,
}

impl EditFileTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 为 old_string not found 错误构建模糊匹配提示。
///
/// 策略 1：取 old_string 前 5 行做前缀匹配，报告匹配到的行号范围。
/// 策略 2：前缀匹配失败时，用滑动窗口找最接近的区域，报告差异行数。
/// old_string > 5000 字符时跳过，仅返回建议 Read 提示。
fn build_not_found_hint(content: &str, old_string: &str) -> String {
    const MAX_FUZZY_LEN: usize = 5000;
    if old_string.len() > MAX_FUZZY_LEN {
        return "建议先 Read 此文件获取最新内容再重试。".to_string();
    }

    // 策略 1：前缀匹配
    let prefix_lines: Vec<&str> = old_string.lines().take(5).collect();
    let prefix: String = prefix_lines.join("\n");
    if !prefix.is_empty() {
        if let Some(byte_offset) = content.find(&prefix) {
            let line_start = content[..byte_offset].lines().count() + 1;
            let line_end = line_start + prefix_lines.len() - 1;
            return format!(
                "old_string 前 {} 行匹配到文件第 {}-{} 行，但整体不匹配。\
                 文件可能已被修改。建议先 Read 此文件获取最新内容再重试。",
                prefix_lines.len(),
                line_start,
                line_end
            );
        }
    }

    // 策略 2：行数近似匹配（回退）
    let old_lines: Vec<&str> = old_string.lines().collect();
    let file_lines: Vec<&str> = content.lines().collect();
    let window_len = old_lines.len();

    if window_len > 0 && window_len <= file_lines.len() {
        let mut best_pos = 0;
        let mut best_common = 0;

        for start in 0..=file_lines.len().saturating_sub(window_len) {
            let window = &file_lines[start..start + window_len];
            let common = window
                .iter()
                .zip(old_lines.iter())
                .filter(|(a, b)| a.trim() == b.trim())
                .count();
            if common > best_common {
                best_common = common;
                best_pos = start;
            }
        }

        if best_common > 0 {
            let line_start = best_pos + 1;
            let line_end = best_pos + window_len;
            let diff_count = window_len - best_common;
            return format!(
                "最接近的匹配在文件第 {}-{} 行（{} 行中有 {} 行不同）。\
                 建议先 Read 此文件获取最新内容再重试。",
                line_start, line_end, window_len, diff_count
            );
        }
    }

    "建议先 Read 此文件获取最新内容再重试。".to_string()
}

/// 把字符串中每行行首的连续 4-空格组转换为 1 个 tab。
/// 用于 tab 缩进文件场景：LLM 通常把 Read 出的 tab 错读为 4 空格，
/// 这里把 old_string/new_string 行首空格转回 tab 以匹配原文风格。
/// 非行首字符不动，行首不足 4 的余数空格保留。
fn convert_leading_spaces_to_tabs(s: &str) -> String {
    s.lines()
        .map(|line| {
            let stripped = line.trim_start_matches(' ');
            let leading = line.len() - stripped.len();
            let tabs = leading / 4;
            let rem = leading % 4;
            format!("{}{}{}", "\t".repeat(tabs), " ".repeat(rem), stripped)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 精确匹配失败时的 tab fallback：仅当文件本身用 tab 缩进，且把 old_string
/// 行首空格转 tab 后能在文件中至少匹配到一处时启用。返回转换后的 (old, new)
/// 用于后续替换；new 同步转换以保持文件的 tab 风格。否则返回 None，让上层
/// 走原本的 not found 错误路径。
/// 注：调用方负责按单次/replace_all 语义对结果做 unique 校验。
fn try_tab_fallback(content: &str, old_string: &str, new_string: &str) -> Option<(String, String)> {
    if !content.lines().any(|l| l.starts_with('\t')) {
        return None;
    }
    let old_tabs = convert_leading_spaces_to_tabs(old_string);
    if old_tabs == old_string {
        return None;
    }
    if !content.contains(&old_tabs) {
        return None;
    }
    Some((old_tabs, convert_leading_spaces_to_tabs(new_string)))
}

#[async_trait::async_trait]
impl BaseTool for EditFileTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        EDIT_FILE_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace. Must match EXACTLY including all whitespace, indentation, and newlines. The edit will fail if old_string is not unique in the file unless replace_all is true"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences of old_string. If false (default), replace only the first occurrence. Use this to rename variables or update repeated patterns across the file"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = input["file_path"]
            .as_str()
            .ok_or("The 'file_path' parameter is required for the Edit tool.")?;
        let old_string = input["old_string"]
            .as_str()
            .ok_or("The 'old_string' parameter is required for the Edit tool.")?;
        let new_string = input["new_string"]
            .as_str()
            .ok_or("The 'new_string' parameter is required for the Edit tool.")?;
        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

        if old_string.is_empty() {
            return Err("Error: old_string cannot be empty".into());
        }

        let resolved = resolve_path(&self.cwd, file_path);

        let raw_content = match std::fs::read_to_string(&resolved) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!("Error: File not found at {file_path}").into());
            }
            Err(e) => return Err(e.into()),
        };

        // CRLF 兼容：Read 工具已剥离 \r，LLM 提取的 old_string 为 LF 格式。
        // 归一化到 LF 进行匹配和替换，写回时恢复原始行尾。
        let is_crlf = raw_content.contains("\r\n");
        let content = if is_crlf {
            raw_content.replace("\r\n", "\n")
        } else {
            raw_content.clone()
        };

        let old_lines = old_string.lines().count();
        let new_lines = new_string.lines().count();
        let line_diff = new_lines as i64 - old_lines as i64;
        let rel = resolved
            .strip_prefix(&self.cwd)
            .unwrap_or(&resolved)
            .display()
            .to_string();

        // 构建行数变化描述
        let diff_desc = match line_diff.cmp(&0) {
            std::cmp::Ordering::Greater => format!(
                "Added {} line{}",
                line_diff,
                if line_diff == 1 { "" } else { "s" }
            ),
            std::cmp::Ordering::Less => format!(
                "Removed {} line{}",
                -line_diff,
                if -line_diff == 1 { "" } else { "s" }
            ),
            std::cmp::Ordering::Equal => "Replaced text (same line count)".to_string(),
        };

        if replace_all {
            let (new_content, occurrences) = if !content.contains(old_string) {
                match try_tab_fallback(&content, old_string, new_string) {
                    Some((old_tabs, new_tabs)) => {
                        let n = content.matches(&old_tabs).count();
                        (content.replace(&old_tabs, &new_tabs), n)
                    }
                    None => {
                        let hint = build_not_found_hint(&content, old_string);
                        return Err(format!(
                            "Error: old_string not found in {}\n{hint}",
                            resolved.display()
                        )
                        .into());
                    }
                }
            } else {
                let n = content.matches(old_string).count();
                (content.replace(old_string, new_string), n)
            };
            // 恢复原始行尾格式
            let final_content = if is_crlf {
                new_content.replace('\n', "\r\n")
            } else {
                new_content
            };
            // 原子写入：先写临时文件再 rename
            let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
            let tmp_path = resolved.with_extension(tmp_ext);
            std::fs::write(&tmp_path, &final_content)?;
            match std::fs::rename(&tmp_path, &resolved) {
                Ok(_) => Ok(format!(
                    "{} to {} (replaced {} occurrence{})",
                    diff_desc,
                    rel,
                    occurrences,
                    if occurrences == 1 { "" } else { "s" }
                )),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    Err(format!("Error renaming temp file: {e}").into())
                }
            }
        } else {
            // tab fallback：精确匹配 0 次时尝试把 old/new 行首空格转 tab。
            // 命中后用转换后的字符串重新计数，复用下面的 unique 校验路径。
            let (old_eff, new_eff): (String, String) = if !content.contains(old_string) {
                match try_tab_fallback(&content, old_string, new_string) {
                    Some(pair) => pair,
                    None => {
                        let hint = build_not_found_hint(&content, old_string);
                        return Err(format!(
                            "Error: old_string not found in {}\n{hint}",
                            resolved.display()
                        )
                        .into());
                    }
                }
            } else {
                (old_string.to_string(), new_string.to_string())
            };
            let occurrences = content.matches(old_eff.as_str()).count();
            if occurrences == 0 {
                let hint = build_not_found_hint(&content, old_string);
                return Err(format!(
                    "Error: old_string not found in {}\n{hint}",
                    resolved.display()
                )
                .into());
            }
            if occurrences > 1 {
                let locations: Vec<String> = content
                    .match_indices(old_eff.as_str())
                    .take(10)
                    .map(|(offset, _)| {
                        let line = content[..offset].lines().count() + 1;
                        let end_line = line + old_eff.lines().count().saturating_sub(1);
                        if end_line > line {
                            format!("第 {}-{} 行", line, end_line)
                        } else {
                            format!("第 {} 行", line)
                        }
                    })
                    .collect();
                let location_text = if occurrences > 10 {
                    format!(
                        "{}（共 {} 处，仅显示前 10 处）",
                        locations.join("、"),
                        occurrences
                    )
                } else {
                    locations.join("、")
                };
                return Err(format!(
                    "Error: old_string is not unique in {} (found {} occurrences).\n\
                     匹配位置：{location_text}。\n\
                     请提供更多上下文使其唯一，或设置 replace_all=true。",
                    resolved.display(),
                    occurrences
                )
                .into());
            }
            let new_content = content.replacen(old_eff.as_str(), new_eff.as_str(), 1);
            // 恢复原始行尾格式
            let final_content = if is_crlf {
                new_content.replace('\n', "\r\n")
            } else {
                new_content
            };
            // 原子写入：先写临时文件再 rename
            let tmp_ext = format!("tmp.{}", uuid::Uuid::now_v7());
            let tmp_path = resolved.with_extension(tmp_ext);
            std::fs::write(&tmp_path, &final_content)?;
            match std::fs::rename(&tmp_path, &resolved) {
                Ok(_) => Ok(format!("{} to {}", diff_desc, rel)),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    Err(format!("Error renaming temp file: {e}").into())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("edit_test.rs");
}
