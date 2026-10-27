//! MEMORY.md 入口索引截断与读取逻辑，对齐 CC 规范。

pub const ENTRYPOINT_NAME: &str = "MEMORY.md";
pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrypointTruncation {
    pub content: String,
    pub line_count: usize,
    pub byte_count: usize,
    pub was_line_truncated: bool,
    pub was_byte_truncated: bool,
}

/// 格式化字节大小（对齐 formatFileSize）
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

/// 对 MEMORY.md 的原始内容按行数（≤200）与字节数（≤25,000）进行截断。
///
/// 若发生截断，在内容末尾追加标准的 WARNING 说明。
pub fn truncate_entrypoint_content(raw: &str) -> EntrypointTruncation {
    let trimmed = raw.trim();
    let content_lines: Vec<&str> = trimmed.split('\n').collect();
    let line_count = content_lines.len();
    let byte_count = trimmed.len();

    let was_line_truncated = line_count > MAX_ENTRYPOINT_LINES;
    let was_byte_truncated = byte_count > MAX_ENTRYPOINT_BYTES;

    if !was_line_truncated && !was_byte_truncated {
        return EntrypointTruncation {
            content: trimmed.to_string(),
            line_count,
            byte_count,
            was_line_truncated: false,
            was_byte_truncated: false,
        };
    }

    let mut truncated = if was_line_truncated {
        content_lines[..MAX_ENTRYPOINT_LINES].join("\n")
    } else {
        trimmed.to_string()
    };

    if truncated.len() > MAX_ENTRYPOINT_BYTES {
        let mut end = MAX_ENTRYPOINT_BYTES;
        while !truncated.is_char_boundary(end) {
            end -= 1;
        }
        if let Some(cut_at) = truncated[..end].rfind('\n') {
            truncated.truncate(cut_at);
        } else {
            truncated.truncate(end);
        }
    }

    let reason = if was_byte_truncated && !was_line_truncated {
        format!(
            "{} (limit: {}) — index entries are too long",
            format_size(byte_count),
            format_size(MAX_ENTRYPOINT_BYTES)
        )
    } else if was_line_truncated && !was_byte_truncated {
        format!("{} lines (limit: {})", line_count, MAX_ENTRYPOINT_LINES)
    } else {
        format!("{} lines and {}", line_count, format_size(byte_count))
    };

    let warning = format!(
        "\n\n> WARNING: {} is {}. Only part of it was loaded. Keep index entries to one line under ~200 chars; move detail into topic files.",
        ENTRYPOINT_NAME, reason
    );

    EntrypointTruncation {
        content: format!("{}{}", truncated, warning),
        line_count,
        byte_count,
        was_line_truncated,
        was_byte_truncated,
    }
}

/// 读取并截断入口索引文件。若文件不存在、不可读或为空则返回 None
pub fn read_and_truncate_entrypoint(
    entrypoint_path: &std::path::Path,
) -> Option<EntrypointTruncation> {
    if !entrypoint_path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(entrypoint_path).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(truncate_entrypoint_content(&raw))
}
