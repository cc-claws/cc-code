//! 消息渲染器 — 将 BaseMessage 列表渲染为可读文本。
//!
//! 支持三种格式：PlainText、Markdown、Json。

use peri_agent::messages::{BaseMessage, ContentBlock};

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
                    let input_str = input.to_string();
                    let truncated: String = input_str.chars().take(200).collect();
                    out.push_str(&format!("[Tool: {}] {}\n", name, truncated));
                }
                ContentBlock::ToolResult { content, .. } => {
                    let line_count = content
                        .iter()
                        .filter_map(|b| b.as_text())
                        .map(|t| t.lines().count())
                        .sum::<usize>();
                    out.push_str(&format!("[Tool Result] ({} lines)\n", line_count));
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
                    let input_str = input.to_string();
                    let truncated: String = input_str.chars().take(200).collect();
                    out.push_str(&format!(
                        "<details><summary>Tool: {}</summary>\n\n{}\n\n</details>\n\n",
                        name, truncated
                    ));
                }
                ContentBlock::ToolResult { content, .. } => {
                    let line_count = content
                        .iter()
                        .filter_map(|b| b.as_text())
                        .map(|t| t.lines().count())
                        .sum::<usize>();
                    out.push_str(&format!("_(tool output, {} lines)_\n\n", line_count));
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

    #[test]
    fn test_render_plain_text_skips_system_messages() {
        let messages = vec![
            BaseMessage::system("You are helpful"),
            BaseMessage::human("hello"),
            BaseMessage::ai("hi there"),
        ];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(!text.contains("You are helpful"), "应跳过 System 消息");
        assert!(text.contains("hello"), "应包含 Human 消息");
        assert!(text.contains("hi there"), "应包含 Ai 消息");
    }

    #[test]
    fn test_render_plain_text_user_assistant_labels() {
        let messages = vec![BaseMessage::human("question"), BaseMessage::ai("answer")];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains("=== User ==="), "应有 User 标签");
        assert!(text.contains("=== Assistant ==="), "应有 Assistant 标签");
    }

    #[test]
    fn test_render_markdown_contains_frontmatter() {
        let messages = vec![BaseMessage::human("test")];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.starts_with("---"), "Markdown 应以 frontmatter 开头");
        assert!(md.contains("# Conversation Export"), "应有标题");
        assert!(md.contains("## User"), "应有 User heading");
    }

    #[test]
    fn test_render_json_is_valid_json() {
        let messages = vec![BaseMessage::human("test")];
        let json = render_messages(&messages, ExportFormat::Json);
        assert!(
            serde_json::from_str::<serde_json::Value>(&json).is_ok(),
            "应为合法 JSON"
        );
    }

    #[test]
    fn test_render_plain_text_contains_content() {
        let messages = vec![BaseMessage::human("read file")];
        let text = render_messages(&messages, ExportFormat::PlainText);
        assert!(text.contains("read file"), "应包含消息内容");
    }

    #[test]
    fn test_render_markdown_separates_turns() {
        let messages = vec![
            BaseMessage::human("q1"),
            BaseMessage::ai("a1"),
            BaseMessage::human("q2"),
        ];
        let md = render_messages(&messages, ExportFormat::Markdown);
        assert!(md.contains("---"), "应有 turn 分隔线");
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::PlainText.extension(), "txt");
        assert_eq!(ExportFormat::Markdown.extension(), "md");
        assert_eq!(ExportFormat::Json.extension(), "json");
    }
}
