use async_trait::async_trait;
use peri_agent::{agent::state::State, middleware::r#trait::Middleware, tools::BaseTool};
use serde_json::Value;
use std::process::Stdio;
use tokio::time::{timeout, Duration};

use crate::tools::output_persist::persist_truncated_output;

/// Windows `cmd /C` 会吞掉引号，导致 `git commit -m "msg with spaces"` 中的
/// message 被空格拆成多个 pathspec。检测到此模式时，将 message 写入临时文件，
/// 改写为 `git commit -F tempfile`，彻底绕开 cmd.exe 引号解析。
///
/// 支持多个 `-m` 标志，按 git 语义以 `\n\n` 拼接。
/// 支持 `&&`、`||`、`|` 链式命令——逐段扫描，仅改写含 `git commit -m` 的段。
/// 返回 `(rewritten_command, Vec<(temp_file_path, message_content)>)`，
/// 调用方负责写入文件并执行后清理。
#[cfg(windows)]
fn rewrite_git_commit_for_windows(command: &str) -> (String, Vec<(String, String)>) {
    // 按 shell 连接符拆分为多段，分别处理
    let segments = split_shell_segments(command);
    let mut rewritten_parts: Vec<String> = Vec::new();
    let mut all_infos: Vec<(String, String)> = Vec::new();

    for (seg, sep) in &segments {
        let seg_trimmed = seg.trim();
        if let Some((new_seg, info)) = rewrite_single_git_commit(seg_trimmed) {
            rewritten_parts.push(new_seg);
            all_infos.push(info);
        } else {
            rewritten_parts.push(seg.to_string());
        }
        if let Some(s) = sep {
            rewritten_parts.push(s.to_string());
        }
    }

    (rewritten_parts.join(""), all_infos)
}

/// 按 shell 连接符（`&&`、`||`、`|`）拆分命令字符串，跳过引号内的内容。
/// 返回 `Vec<(segment, Option<separator>)>`。
#[cfg(windows)]
fn split_shell_segments(command: &str) -> Vec<(String, Option<String>)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(c) = chars.next() {
        // 处理引号内字符（转义引号跳过）
        if let Some(q) = in_quote {
            current.push(c);
            if c == '\\' {
                // 转义：吃掉下一个字符
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else if c == q {
                in_quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_quote = Some(c);
            current.push(c);
            continue;
        }

        // 检测连接符（需要前后有空格，避免误拆 "foo&&bar"）
        if c == '&' && chars.peek() == Some(&'&') {
            chars.next(); // 消费第二个 &
            // 检查前后是否合理（简单判断：当前段非空）
            if !current.trim().is_empty() {
                let sep = "&&".to_string();
                segments.push((current.clone(), Some(sep)));
                current.clear();
                // 跳过分隔符后的空格已由下轮处理
                continue;
            }
        }
        if c == '|' && chars.peek() == Some(&'|') {
            chars.next(); // 消费第二个 |
            if !current.trim().is_empty() {
                let sep = "||".to_string();
                segments.push((current.clone(), Some(sep)));
                current.clear();
                continue;
            }
        }
        if c == '|' {
            // 单独的 |（管道），排除 || 的情况（已在上面处理）
            if !current.trim().is_empty() {
                let sep = "|".to_string();
                segments.push((current.clone(), Some(sep)));
                current.clear();
                continue;
            }
        }

        current.push(c);
    }
    if !current.is_empty() || segments.is_empty() {
        segments.push((current, None));
    }
    segments
}

/// 尝试将单个命令段中的 `git commit -m "msg"` 改写为 `git commit -F tempfile`。
/// 返回 `Some((rewritten_cmd, (temp_path, msg)))` 或 `None`。
#[cfg(windows)]
fn rewrite_single_git_commit(trimmed: &str) -> Option<(String, (String, String))> {
    let commit_pos = trimmed.find("commit").or_else(|| trimmed.find("COMMIT"))?;
    let pos = commit_pos;
    let prefix = &trimmed[..pos];
    if !prefix.contains("git") && !prefix.ends_with(' ') {
        return None;
    }

    let commit_prefix = &trimmed[..pos + 6]; // 到 "commit" 为止
    let after_commit = trimmed[pos + 6..].trim_start();

    let mut remaining = after_commit;
    let mut messages: Vec<String> = Vec::new();
    let mut other_args: Vec<&str> = Vec::new();

    while !remaining.is_empty() {
        if remaining.starts_with("--message ") {
            let rest = remaining[10..].trim_start();
            if let (Some(msg), after) = extract_quoted_message(rest) {
                messages.push(msg);
                remaining = after.trim_start();
                continue;
            }
        } else if remaining.starts_with("-m ") {
            let rest = remaining[3..].trim_start();
            if let (Some(msg), after) = extract_quoted_message(rest) {
                messages.push(msg);
                remaining = after.trim_start();
                continue;
            }
        }

        let end = remaining.find(' ').unwrap_or(remaining.len());
        other_args.push(&remaining[..end]);
        remaining = remaining[end..].trim_start();
    }

    if messages.is_empty() {
        return None;
    }

    let combined_msg = messages.join("\n\n");

    let temp_dir = std::env::temp_dir();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_path = temp_dir.join(format!("peri-commit-msg-{timestamp}.txt"));
    let temp_path_str = temp_path.to_string_lossy().replace('\\', "/");

    let mut new_cmd = format!("{commit_prefix} -F {temp_path_str}");
    for arg in &other_args {
        new_cmd.push(' ');
        new_cmd.push_str(arg);
    }

    Some((new_cmd, (temp_path.to_string_lossy().to_string(), combined_msg)))
}

/// 从命令字符串中提取引号包裹的 message 内容。
/// 返回 `(Some(message), remaining_after_quote)` 或 `(None, _)`。
/// 仅在 Windows 上由 `rewrite_git_commit_for_windows` 调用，
/// 非 Windows 编译保留以供单元测试覆盖。
#[cfg_attr(not(windows), allow(dead_code))]
fn extract_quoted_message(s: &str) -> (Option<String>, &str) {
    let mut chars = s.chars();
    let quote_char = match chars.next() {
        Some(c @ '"') | Some(c @ '\'') => c,
        _ => return (None, s),
    };
    let q_len = quote_char.len_utf8();
    let rest = &s[q_len..];
    let mut msg = String::new();
    let mut char_indices = rest.char_indices().peekable();
    while let Some((i, c)) = char_indices.next() {
        if c == '\\' {
            // 转义引号
            if let Some(&(_, next_c)) = char_indices.peek() {
                if next_c == quote_char {
                    msg.push(quote_char);
                    char_indices.next(); // consume escaped char
                    continue;
                }
            }
            msg.push(c);
        } else if c == quote_char {
            // 结束引号
            return (Some(msg), &rest[i + c.len_utf8()..]);
        } else {
            msg.push(c);
        }
    }
    // 未找到结束引号
    (None, s)
}

/// BashTool - 终端命令执行工具，与 TypeScript TerminalMiddleware 对齐
const BASH_DESCRIPTION: &str = r#"Executes a given shell command and returns its output.

Usage:
- The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh)
- IMPORTANT: Avoid using this tool to run find, grep, cat, head, tail, sed, awk, or echo commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task
- Instead, use the appropriate dedicated tool which will provide a much better experience for the user:
  - File search: Use Glob (NOT find or ls)
  - Content search: Use Grep (NOT grep or rg)
  - Read files: Use Read (NOT cat/head/tail)
  - Edit files: Use Edit (NOT sed/awk)
  - Write files: Use Write (NOT echo/cat with redirect)
- You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). Default is 120000ms (2 minutes)
- When issuing multiple commands, use && to chain them together rather than using separate tool calls if the commands depend on each other
- For long running commands, consider using a timeout to avoid waiting indefinitely

Platform behavior:
- Windows: uses cmd /C to execute commands
- Unix/macOS: uses bash -c to execute commands
- On Unix, child processes run in their own process group; timeout kills the entire process tree

Output handling:
- Output exceeding 2000 lines is truncated (head + tail preserved)
- Output exceeding 100000 bytes is truncated
- Non-zero exit codes are reported
- Both stdout and stderr are captured"#;
pub struct BashTool {
    pub cwd: String,
}

impl BashTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 输出最大字节数
const MAX_OUTPUT_CHARS: usize = 100_000;
/// 输出最大行数（在第 N 行截断后，若还有行数超过上限再截字节）
const MAX_OUTPUT_LINES: usize = 2_000;

/// 按字节截断字符串，确保不拆分 UTF-8 字符
fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn truncate_output(output: &str) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    if lines.len() > MAX_OUTPUT_LINES {
        let total_lines = lines.len();
        // Persist full content before truncating
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
        // Check byte limit after adding hint
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

#[async_trait::async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        BASH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command (and optional arguments) to execute. This can be complex commands that use pipes, &&, or other shell features. For multiple dependent commands, chain them with && rather than making separate calls"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (default 120000, max 600000). If the command takes longer than this, it will be killed and a timeout error returned"
                },
                "description": {
                    "type": "string",
                    "description": "A clear, concise description of what this command does in active voice. Never use words like 'complex' or 'risk' in the description — just describe what it does"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later"
                }
            },
            "required": ["command"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing command parameter")?;

        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(120_000)
            .clamp(1, 600_000);
        let _description = input["description"].as_str();
        let _run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        // Windows: 重写 git commit -m 为 git commit -F，绕开 cmd.exe 引号问题
        #[cfg(windows)]
        let (command, temp_msg_files) = {
            let (cmd, infos) = rewrite_git_commit_for_windows(command);
            let mut files = Vec::new();
            for (ref path, ref content) in &infos {
                let _ = std::fs::write(path, content);
                files.push(path.clone());
            }
            (cmd, files)
        };
        #[cfg(not(windows))]
        let temp_msg_files: Vec<String> = Vec::new();

        let result = timeout(Duration::from_millis(timeout_ms), {
            // Windows 分支中 `command` 被 shadow 为 String（L278），需要取引用；
            // 非 Windows 上 `command` 本身就是 &str，直接传递。
            #[cfg(windows)]
            let command_arg: &str = &command;
            #[cfg(not(windows))]
            let command_arg: &str = command;
            let mut cmd = crate::process::shell_command(command_arg, &[]);
            cmd.current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            cmd.process_group(0);
            cmd.output()
        })
        .await;

        // 清理临时文件
        for path in &temp_msg_files {
            let _ = tokio::fs::remove_file(path).await;
        }

        match result {
            Err(_) => Err(format!(
                "Error: Command timed out after {} seconds.\nCommand: {command}",
                timeout_ms as f64 / 1000.0
            )
            .into()),
            Ok(Err(e)) => Err(format!("Error executing command: {e}").into()),
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let exit_code = out.status.code().unwrap_or(-1);

                let mut output = String::new();

                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }
                if exit_code != 0 {
                    output.push_str(&format!("\n[Exit code: {exit_code}]"));
                }

                if output.is_empty() {
                    output = format!("[Command completed with exit code {exit_code}]");
                }

                // 截断过长输出，防止撑爆 LLM context window
                Ok(truncate_output(&output))
            }
        }
    }
}

/// TerminalMiddleware - 与 TypeScript TerminalMiddleware 对齐
pub struct TerminalMiddleware;

impl TerminalMiddleware {
    pub fn new() -> Self {
        Self
    }

    pub fn build_tools(cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(BashTool::new(cwd))]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["Bash"]
    }
}

impl Default for TerminalMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for TerminalMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }

    fn name(&self) -> &str {
        "TerminalMiddleware"
    }
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
