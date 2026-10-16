use super::*;

#[test]
fn test_format_display_path_for_memory_normalizes_and_strips() {
    assert_eq!(
        format_display_path_for_memory(r"C:\Users\admin\.peri\memory\user.md"),
        Some("memory/user.md".to_string())
    );
    assert_eq!(
        format_display_path_for_memory("/home/user/project/src/main.rs"),
        None
    );
}

#[test]
fn test_read_summary_memory_short_circuit() {
    assert_eq!(
        read_summary("content...", true),
        Some("Recalled memory".to_string())
    );
}

#[test]
fn test_read_summary_counts_lines() {
    let content = "line 1\nline 2\nline 3";
    assert_eq!(
        read_summary(content, false),
        Some("Read 3 lines".to_string())
    );
}
