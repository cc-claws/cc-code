//! 消息渲染器 — 将 BaseMessage 列表渲染为可读文本。
//!
//! 支持三种格式：PlainText、Markdown、Json。

use cc_agent::messages::{BaseMessage, ContentBlock};

/// 导出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// .txt — 纯文本，适合分享
    PlainText,
    /// .md — 结构化 Markdown，适合文档
    Markdown,
    /// .json — 原始 JSON，适合程序处理
    Json,
}

impl ExportFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::PlainText => "txt",
            Self::Markdown => "md",
            Self::Json => "json",
        }
    }
}

/// 将消息列表渲染为指定格式的字符串。
pub fn render_messages(messages: &[BaseMessage], format: ExportFormat) -> String {
    match format {
        ExportFormat::PlainText => render_plain_text(messages),
        ExportFormat::Markdown => render_markdown(messages),
        ExportFormat::Json => render_json(messages),
    }
}

/// 拼接 ToolResult 的所有文本 block 为完整正文（不截断）。
fn tool_result_body(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| b.as_text())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 根据正文中连续反引号的最大长度生成安全的 Markdown fenced code block 围栏。
///
/// 至少 3 个反引号；正文含 ``` 时递增（如 4 个），保证围栏不被内容破坏
/// （例如写脚本的 Bash 命令里内嵌 heredoc 反引号）。
fn code_fence(body: &str) -> String {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for ch in body.chars() {
        if ch == '`' {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat((max_run + 1).max(3))
}

// ── PlainText ────────────────────────────────────────────────────────────────

fn render_plain_text(messages: &[BaseMessage]) -> String {
    let mut out = String::new();
    for msg in messages {
        if msg.is_system() {
            continue;
        }
        let role = match msg {
            BaseMessage::Human { .. } => "User",
            BaseMessage::Ai { .. } => "Assistant",
            BaseMessage::Tool { .. } => "Tool",
            BaseMessage::System { .. } => continue,
        };
        out.push_str(&format!("=== {} ===\n", role));

        // 渲染 tool_use blocks
        for block in msg.content_blocks() {
            match block {
                ContentBlock::ToolUse { name, input, .. } => {
                    // 完整保留参数 JSON，不做截断（导出用于调试/审计）
                    out.push_str(&format!("[Tool: {}] {}\n", name, input));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let marker = if is_error { " (error)" } else { "" };
                    out.push_str(&format!(
                        "[Tool Result{}]\n{}\n",
                        marker,
                        tool_result_body(&content)
                    ));
                }
                ContentBlock::Text { text } => {
                    out.push_str(&text);
                    out.push('\n');
                }
                _ => {} // Image/Document/Reasoning/Unknown 跳过
            }
        }
        out.push('\n');
    }
    out
}

// ── Markdown ─────────────────────────────────────────────────────────────────

fn render_markdown(messages: &[BaseMessage]) -> String {
    let non_system: Vec<&BaseMessage> = messages.iter().filter(|m| !m.is_system()).collect();
    let msg_count = non_system.len();

    let mut out = String::new();

    // Frontmatter
    out.push_str("---\n");
    out.push_str(&format!("messages: {}\n", msg_count));
    out.push_str(&format!(
        "exported: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    out.push_str("---\n\n");
    out.push_str("# Conversation Export\n\n");

    for (i, msg) in non_system.iter().enumerate() {
        if i > 0 {
            out.push_str("---\n\n");
        }
        let role = match msg {
            BaseMessage::Human { .. } => "User",
            BaseMessage::Ai { .. } => "Assistant",
            BaseMessage::Tool { .. } => "Tool",
            _ => continue,
        };
        out.push_str(&format!("## {}\n\n", role));

        for block in msg.content_blocks() {
            match block {
                ContentBlock::ToolUse { name, input, .. } => {
                    // 完整保留参数 JSON（不截断）；围栏长度自适应，防止内嵌
                    // 反引号（如 heredoc 脚本）破坏 Markdown 结构
                    let input_str = input.to_string();
                    let fence = code_fence(&input_str);
                    out.push_str(&format!(
                        "<details><summary>Tool: {}</summary>\n\n{fence}json\n{}\n{fence}\n\n</details>\n\n",
                        name, input_str
                    ));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let marker = if is_error { " (error)" } else { "" };
                    let body = tool_result_body(&content);
                    let fence = code_fence(&body);
                    out.push_str(&format!(
                        "<details><summary>Tool Result{}</summary>\n\n{fence}\n{}\n{fence}\n\n</details>\n\n",
                        marker, body
                    ));
                }
                ContentBlock::Text { text } => {
                    out.push_str(&text);
                    out.push_str("\n\n");
                }
                _ => {}
            }
        }
    }
    out
}

// ── JSON ─────────────────────────────────────────────────────────────────────

fn render_json(messages: &[BaseMessage]) -> String {
    serde_json::to_string_pretty(messages).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("renderer_test.rs");
}
