//! 文件名生成 — 自动文件名、sanitize、格式推断。

use chrono::Local;
use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

use super::renderer::ExportFormat;

/// 根据消息和格式生成默认文件名。
///
/// 格式：`<timestamp>-<first-prompt-kebab>.<ext>`
/// 无消息时：`conversation-<timestamp>.<ext>`
pub fn generate_default_filename(messages: &[BaseMessage], format: ExportFormat) -> String {
    let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
    let prompt_hint = extract_first_prompt_hint(messages);
    let ext = format.extension();

    if prompt_hint.is_empty() {
        format!("conversation-{timestamp}.{ext}")
    } else {
        let sanitized = sanitize_filename(&prompt_hint);
        format!("{timestamp}-{sanitized}.{ext}")
    }
}

/// 提取首条用户消息的第一行，截断 50 字符。
fn extract_first_prompt_hint(messages: &[BaseMessage]) -> String {
    messages
        .iter()
        .find_map(|m| match m {
            BaseMessage::Human { content, .. } => Some(content),
            _ => None,
        })
        .map(|c| match c {
            MessageContent::Text(text) => text.as_ref(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_ref()),
                    _ => None,
                })
                .unwrap_or(""),
            _ => "",
        })
        .map(|text| {
            let first_line = text.lines().next().unwrap_or("");
            first_line.chars().take(50).collect::<String>()
        })
        .unwrap_or_default()
}

/// 清理文件名：小写、去特殊字符、空格转连字符。
pub fn sanitize_filename(text: &str) -> String {
    let mapped: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();
    mapped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// 从文件名推断导出格式。
pub fn infer_format_from_filename(filename: &str) -> ExportFormat {
    if filename.ends_with(".md") || filename.ends_with(".markdown") {
        ExportFormat::Markdown
    } else if filename.ends_with(".json") {
        ExportFormat::Json
    } else {
        ExportFormat::PlainText
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_filename_with_prompt() {
        let messages = vec![BaseMessage::human("How to fix the bug?")];
        let name = generate_default_filename(&messages, ExportFormat::Markdown);
        assert!(name.ends_with(".md"), "应有正确扩展名: {name}");
        assert!(
            name.contains("how-to-fix-the-bug"),
            "应包含 sanitized prompt: {name}"
        );
    }

    #[test]
    fn test_generate_default_filename_empty_prompt() {
        let messages: Vec<BaseMessage> = vec![];
        let name = generate_default_filename(&messages, ExportFormat::PlainText);
        assert!(
            name.starts_with("conversation-"),
            "无消息时用 conversation- 前缀: {name}"
        );
        assert!(name.ends_with(".txt"), "应有 .txt 扩展名: {name}");
    }

    #[test]
    fn test_sanitize_filename_removes_special_chars() {
        assert_eq!(sanitize_filename("Hello World!"), "hello-world");
        assert_eq!(sanitize_filename("fix: bug #123"), "fix-bug-123");
        assert_eq!(sanitize_filename("  spaces  "), "spaces");
    }

    #[test]
    fn test_sanitize_filename_chinese() {
        let result = sanitize_filename("你好世界");
        assert_eq!(result, "你好世界", "中文字符应保留");
    }

    #[test]
    fn test_infer_format_from_filename() {
        assert!(matches!(
            infer_format_from_filename("out.md"),
            ExportFormat::Markdown
        ));
        assert!(matches!(
            infer_format_from_filename("out.json"),
            ExportFormat::Json
        ));
        assert!(matches!(
            infer_format_from_filename("out.txt"),
            ExportFormat::PlainText
        ));
        assert!(matches!(
            infer_format_from_filename("out"),
            ExportFormat::PlainText
        ));
    }

    #[test]
    fn test_extract_first_prompt_hint_multi_line() {
        let messages = vec![BaseMessage::human("first line\nsecond line\nthird line")];
        let hint = extract_first_prompt_hint(&messages);
        assert_eq!(hint, "first line", "应只取第一行");
    }

    #[test]
    fn test_extract_first_prompt_hint_truncation() {
        let long = "a".repeat(100);
        let messages = vec![BaseMessage::human(long)];
        let hint = extract_first_prompt_hint(&messages);
        assert!(hint.chars().count() <= 50, "应截断到 50 字符");
    }
}
