use super::{hook_error_reason, single_line_reason};

#[test]
fn test_single_line_reason_strips_ansi_and_collapses_whitespace() {
    // Arrange：ANSI 转义 + 换行 + 制表符 + 连续空格
    let raw = "\u{1b}[31merror\u{1b}[0m:\n\tfailed   twice\r\n";
    // Act
    let reason = single_line_reason(raw);
    // Assert
    assert_eq!(reason, "error: failed twice");
}

#[test]
fn test_single_line_reason_truncates_to_200_chars() {
    // Arrange：250 个 CJK 字符，按字符截断不能落在半个字上
    let raw = "错".repeat(250);
    // Act
    let reason = single_line_reason(&raw);
    // Assert
    assert_eq!(reason.chars().count(), 200);
    assert!(reason.chars().all(|c| c == '错'));
}

#[test]
fn test_hook_error_reason_joins_prefix_and_detail() {
    assert_eq!(
        hook_error_reason("hook exited with code 1", "boom\n"),
        "hook exited with code 1: boom"
    );
}

#[test]
fn test_hook_error_reason_keeps_prefix_when_detail_empty() {
    // 空 stderr（如 hook 被信号杀掉）不能留下悬空的 ": "
    assert_eq!(
        hook_error_reason("hook terminated by signal", "  \n\t"),
        "hook terminated by signal"
    );
}
