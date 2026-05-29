use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// 最大读取行数
const MAX_LINES: usize = 2000;
/// 目录列表最大条目数
const MAX_DIR_ENTRIES: usize = 100;

/// 文件读取结果
#[derive(Debug, Clone)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub truncated: bool,
    pub is_dir: bool,
}

/// 读取文件内容（支持行范围）
///
/// - 路径校验：canonicalize + 路径穿越防护
/// - 超过 MAX_LINES 截断，附加 "... (truncated)"
/// - 目录：列出条目（最多 100），目录后缀 "/"
/// - 文件不存在：返回 None
pub fn read_file_content(
    base_dir: &Path,
    path: &str,
    line_start: Option<usize>,
    line_end: Option<usize>,
) -> Option<FileContent> {
    let resolved = resolve_path(base_dir, path)?;

    if resolved.is_dir() {
        return Some(read_directory(&resolved, path));
    }

    if !resolved.is_file() {
        return None;
    }

    let raw = fs::read_to_string(&resolved).ok()?;

    let (content, truncated) = extract_lines(&raw, line_start, line_end);

    Some(FileContent {
        path: path.to_string(),
        content,
        line_start,
        line_end,
        truncated,
        is_dir: false,
    })
}

/// 路径解析 + 穿越防护
fn resolve_path(base_dir: &Path, path: &str) -> Option<PathBuf> {
    let rel = Path::new(path);

    // 拒绝绝对路径
    if rel.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }

    // 逐组件检查 —— 拒绝任何导致逃逸的 .. 组件
    let mut depth: i32 = 0;
    for component in rel.components() {
        match component {
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            Component::Normal(_) => depth += 1,
            _ => {}
        }
    }

    let resolved = base_dir.join(rel);

    // canonicalize 验证路径存在且在 base_dir 内
    let canonical = resolved.canonicalize().ok()?;
    let canonical_base = base_dir.canonicalize().ok()?;

    if !canonical.starts_with(&canonical_base) {
        return None;
    }

    Some(canonical)
}

/// 按行范围提取内容
fn extract_lines(raw: &str, line_start: Option<usize>, line_end: Option<usize>) -> (String, bool) {
    let all_lines: Vec<&str> = raw.lines().collect();
    let total = all_lines.len();

    let start = line_start.unwrap_or(1).saturating_sub(1); // 转为 0-based
    let end = line_end.map(|e| e.min(total)).unwrap_or(total);

    if start >= total {
        return (String::new(), false);
    }

    let selected: Vec<&str> = all_lines[start..end].to_vec();
    let truncated = selected.len() > MAX_LINES;
    let result: Vec<&str> = if truncated {
        selected[..MAX_LINES].to_vec()
    } else {
        selected
    };

    let content = result.join("\n");
    let content = if truncated {
        format!("{content}\n... (truncated)")
    } else {
        content
    };

    (content, truncated)
}

/// 读取目录列表
fn read_directory(dir: &Path, display_path: &str) -> FileContent {
    let mut entries: Vec<String> = Vec::new();

    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            if entries.len() >= MAX_DIR_ENTRIES {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let suffix = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            entries.push(format!("{name}{suffix}"));
        }
    }

    entries.sort();

    let content = entries.join("\n");

    FileContent {
        path: display_path.to_string(),
        content,
        line_start: None,
        line_end: None,
        truncated: false,
        is_dir: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_read_file_full_content() {
        // 读取完整文件内容
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test.rs"), "fn main() {}\n").unwrap();
        let result = read_file_content(dir.path(), "test.rs", None, None).unwrap();
        assert_eq!(result.content, "fn main() {}");
        assert!(!result.truncated);
        assert!(!result.is_dir);
    }

    #[test]
    fn test_read_file_line_range() {
        // 读取指定行范围
        let dir = tempdir().unwrap();
        let content = "line1\nline2\nline3\nline4\nline5\n";
        fs::write(dir.path().join("test.txt"), content).unwrap();
        let result = read_file_content(dir.path(), "test.txt", Some(2), Some(4)).unwrap();
        assert_eq!(result.content, "line2\nline3\nline4");
        assert_eq!(result.line_start, Some(2));
        assert_eq!(result.line_end, Some(4));
    }

    #[test]
    fn test_read_directory() {
        // 读取目录列表
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        let result = read_file_content(dir.path(), ".", None, None).unwrap();
        assert!(result.is_dir);
        assert!(result.content.contains("a.txt"));
        assert!(result.content.contains("subdir/"));
    }

    #[test]
    fn test_read_nonexistent_file() {
        // 不存在的文件返回 None
        let dir = tempdir().unwrap();
        let result = read_file_content(dir.path(), "nope.rs", None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_path_traversal_blocked() {
        // 路径穿越被拒绝
        let dir = tempdir().unwrap();
        let result = read_file_content(dir.path(), "../../../etc/passwd", None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_truncation() {
        // 超过 MAX_LINES 截断
        let dir = tempdir().unwrap();
        let lines: Vec<String> = (0..2500).map(|i| format!("line{i}")).collect();
        fs::write(dir.path().join("big.txt"), lines.join("\n")).unwrap();
        let result = read_file_content(dir.path(), "big.txt", None, None).unwrap();
        assert!(result.truncated);
        assert!(result.content.ends_with("... (truncated)"));
    }
}
