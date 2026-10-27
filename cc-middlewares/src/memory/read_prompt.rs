//! 面向宿主的精简记忆提示词，仅附带有界索引，不触发记忆生成。

/// 首次注入的索引正文预算，包含截断提示，按 UTF-8 字节计数。
pub const MAX_MEMORY_READ_INDEX_BYTES: usize = 20_000;

const TRUNCATED_INDEX_SUFFIX: &str =
    "\n[Partial index; Grep the full MEMORY.md for relevant entries, then Read their topic files.]";

/// 构建精简的记忆读取提示词，索引由调用方在会话创建时捕获。
pub fn build_memory_read_prompt(memory_dir: &str, entrypoint_content: Option<&str>) -> String {
    let formatted_dir = if memory_dir.ends_with('/') || memory_dir.ends_with('\\') {
        memory_dir.to_string()
    } else {
        format!("{memory_dir}/")
    };
    let mut prompt = format!(
        "# Memory\nDirectory: `{formatted_dir}`.\nFor relevant prior context or explicit recall, use the index and Read relevant topic files. If the preview is empty, partial, or has no relevant entry, Grep the full MEMORY.md with task keywords (output_mode: content); if missing or no match, Grep this directory's *.md files. Read matching topics or sections; use offset/limit for long files. An empty preview does not mean no memory exists; current source takes precedence over memory.\nSave, update, or delete memory only when the user explicitly requests it; use Write/Edit to maintain topic files and a short MEMORY.md index (frontmatter type: user/feedback/project/reference). Before editing the index, Read its current contents; never overwrite it from this preview."
    );
    if let Some(content) = entrypoint_content.filter(|content| !content.trim().is_empty()) {
        prompt.push_str("\n\nMEMORY.md:\n");
        prompt.push_str(&truncate_memory_read_index(content.trim()));
    }
    prompt
}

fn truncate_memory_read_index(content: &str) -> String {
    if content.len() <= MAX_MEMORY_READ_INDEX_BYTES {
        return content.to_string();
    }
    let mut end = MAX_MEMORY_READ_INDEX_BYTES - TRUNCATED_INDEX_SUFFIX.len();
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    // 优先保留完整行；首行已超长时，在预算内的字符边界裁切。
    let prefix = &content[..end];
    let prefix = match prefix.rfind('\n') {
        Some(line_end) => &prefix[..line_end],
        None => prefix,
    };
    format!("{}{TRUNCATED_INDEX_SUFFIX}", prefix.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("read_prompt_test.rs");
}
