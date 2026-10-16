use super::*;

#[test]
fn test_glob_summary_empty_returns_none() {
    assert_eq!(glob_summary(""), None);
}

#[test]
fn test_glob_summary_counts_non_empty_lines() {
    let content = "file1.rs\nfile2.rs\n\nfile3.rs\n";
    assert_eq!(glob_summary(content), Some("Found 3 files".to_string()));
}
