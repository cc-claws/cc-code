/// 精简持久化记忆文件的路径显示（如 `/.../memory/user.md` → `memory/user.md`）。
pub fn format_display_path_for_memory(path_str: &str) -> Option<String> {
    let normalized = path_str.replace('\\', "/");
    if let Some(pos) = normalized.find("/memory/") {
        let filename = &normalized[pos + "/memory/".len()..];
        if !filename.is_empty() {
            return Some(format!("memory/{filename}"));
        }
    }
    None
}

/// 生成 Read 工具的折叠态行数/记忆召回摘要。
pub fn read_summary(content: &str, is_memory: bool) -> Option<String> {
    if is_memory {
        return Some("Recalled memory".to_string());
    }
    if content.is_empty() {
        return None;
    }
    if let Some(first_line) = content.lines().next() {
        if first_line.starts_with("Read ") && first_line.ends_with(" lines") {
            return Some(first_line.to_string());
        }
    }
    Some(format!("Read {} lines", content.lines().count()))
}

#[cfg(test)]
#[path = "read_test.rs"]
mod read_test;
