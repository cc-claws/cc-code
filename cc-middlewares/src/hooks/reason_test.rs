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
fn test_single_line_reason_truncates_to_100_chars() {
    // Arrange：250 个 CJK 字符，按字符截断不能落在半个字上
    let raw = "错".repeat(250);
    // Act
    let reason = single_line_reason(&raw);
    // Assert
    assert_eq!(reason.chars().count(), 100);
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

/// 典型 Node 崩溃堆栈：取中间那行 `Error: Cannot find module …`，
/// 而不是整段折叠，也不是末尾的版本尾巴。
#[test]
fn test_hook_error_reason_picks_last_error_line_from_stack() {
    // Arrange
    let stderr = concat!(
        "node:internal/modules/cjs/loader:1080\n",
        "  throw err;\n",
        "  ^\n",
        "\n",
        "Error: Cannot find module 'C:\\Users\\me\\.claude\\clawd-hook.js'\n",
        "Require stack:\n",
        "- C:\\Users\\me\\.claude\\[stdin]\n",
        "    at Module._resolveFilename (node:internal/modules/cjs/loader:1077:15)\n",
        "Node.js v20.11.1\n",
    );
    // Act
    let reason = hook_error_reason("hook exited with code 1", stderr);
    // Assert
    assert_eq!(
        reason,
        "hook exited with code 1: Error: Cannot find module 'C:\\Users\\me\\.claude\\clawd-hook.js'"
    );
}

/// 没有 `Error:` 字样时退化成最后一条非空行，仍只保留一行。
#[test]
fn test_hook_error_reason_falls_back_to_last_non_empty_line() {
    // Arrange
    let stderr = "first line\n\nsecond line   with   spaces\n\n";
    // Act
    let reason = hook_error_reason("hook failed to run", stderr);
    // Assert
    assert_eq!(reason, "hook failed to run: second line with spaces");
}

/// 关键行本身也可能很长（单行 JSON、长路径）：按字符截断到 100。
#[test]
fn test_hook_error_reason_truncates_long_line_to_100_chars() {
    // Arrange
    let stderr = format!("noise\nError: {}\n", "错".repeat(250));
    // Act
    let reason = hook_error_reason("hook exited with code 1", &stderr);
    // Assert
    let detail = reason
        .strip_prefix("hook exited with code 1: ")
        .expect("reason 应保留前缀");
    assert_eq!(detail.chars().count(), 100);
    assert!(
        detail.starts_with("Error: 错错"),
        "截断后仍保留关键行开头: {detail:?}"
    );
}

/// 剥 ANSI 必须先于挑行：否则 `\x1b[31mError:` 匹配不到 `Error:`。
#[test]
fn test_hook_error_reason_matches_error_line_with_ansi_prefix() {
    // Arrange
    let stderr = "\u{1b}[31mError: boom\u{1b}[0m\nlast line\n";
    // Act
    let reason = hook_error_reason("hook exited with code 1", stderr);
    // Assert
    assert_eq!(reason, "hook exited with code 1: Error: boom");
}
