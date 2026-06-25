use super::*;

#[test]
fn test_compute_diff_simple_edit() {
    // 基本编辑：替换一行
    let input = DiffInput {
        file_path: "test.txt".to_string(),
        old_content: "line1\nline2\nline3\n".to_string(),
        new_content: "line1\nmodified\nline3\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    // 至少有一个 hunk
    assert!(!result.hunks.is_empty(), "应有至少一个 hunk");
    assert!(!result.is_truncated);
    assert!(!result.is_binary);
    // 检查 hunk 中有 Add 和 Remove 行
    let hunk = &result.hunks[0];
    let has_remove = hunk
        .lines
        .iter()
        .any(|l| matches!(l, DiffLine::Remove { .. }));
    let has_add = hunk.lines.iter().any(|l| matches!(l, DiffLine::Add { .. }));
    assert!(has_remove, "应有 Remove 行");
    assert!(has_add, "应有 Add 行");
}

#[test]
fn test_compute_diff_new_file() {
    // 新文件：所有行应为 Add
    let input = DiffInput {
        file_path: "new.txt".to_string(),
        old_content: String::new(),
        new_content: "hello\nworld\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(result.is_new_file);
    assert!(!result.hunks.is_empty(), "新文件应有 hunk");
    let hunk = &result.hunks[0];
    // 所有非 HunkHeader 行都应该是 Add
    for line in &hunk.lines {
        match line {
            DiffLine::Add { .. } | DiffLine::HunkHeader { .. } => {}
            other => panic!("新文件不应有 {:?} 行", other),
        }
    }
}

#[test]
fn test_compute_diff_identical_content() {
    // 完全相同的内容，不应产生 hunk
    let input = DiffInput {
        file_path: "same.txt".to_string(),
        old_content: "same\ncontent\n".to_string(),
        new_content: "same\ncontent\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(result.hunks.is_empty(), "相同内容不应产生 hunk");
    assert!(!result.is_truncated);
}

#[test]
fn test_compute_diff_large_file_truncation() {
    // 超过 1MB 限制，应截断
    let big_old = "x".repeat(600_000);
    let big_new = "y".repeat(600_000);
    let input = DiffInput {
        file_path: "big.txt".to_string(),
        old_content: big_old,
        new_content: big_new,
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(result.is_truncated, "超大文件应截断");
    assert!(result.hunks.is_empty(), "截断后不应有 hunk");
}

#[test]
fn test_word_diff_basic() {
    // "hello world" → "hello earth"：hello 未变，world→earth 变更
    let wd = compute_word_diff("hello world", "hello earth");
    // 应有 Unchanged 段（hello 和空格）
    let has_unchanged = wd
        .segments
        .iter()
        .any(|(s, t)| matches!(t, DiffWordType::Unchanged) && s.contains("hello"));
    assert!(has_unchanged, "应有包含 'hello' 的 Unchanged 段");
    // 应有 Removed 和 Added 段
    let has_removed = wd
        .segments
        .iter()
        .any(|(_, t)| matches!(t, DiffWordType::Removed));
    let has_added = wd
        .segments
        .iter()
        .any(|(_, t)| matches!(t, DiffWordType::Added));
    assert!(has_removed, "应有 Removed 段");
    assert!(has_added, "应有 Added 段");
}

#[test]
fn test_diff_input_equality() {
    // clone 后 assert_eq
    let input = DiffInput {
        file_path: "test.txt".to_string(),
        old_content: "old".to_string(),
        new_content: "new".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let cloned = input.clone();
    assert_eq!(input, cloned);
}
