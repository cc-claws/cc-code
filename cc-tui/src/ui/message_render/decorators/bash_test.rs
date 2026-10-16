use super::*;

#[test]
fn test_parse_exit_code_standard_format() {
    let output = "error occurred\n[Exit code: 8]";
    assert_eq!(parse_exit_code(output), Some(8));
}

#[test]
fn test_parse_exit_code_completed_format() {
    let output = "[Command completed with exit code 127]";
    assert_eq!(parse_exit_code(output), Some(127));
}

#[test]
fn test_parse_exit_code_none_on_success() {
    let output = "cargo build finished successfully\n";
    assert_eq!(parse_exit_code(output), None);
}

#[test]
fn test_strip_exit_code_suffix_standard_bracket() {
    let output = "first line\nsecond line\n\n[Exit code: 1]";
    assert_eq!(strip_exit_code_suffix(output), "first line\nsecond line");
}

#[test]
fn test_strip_exit_code_suffix_completed_bracket() {
    let output = "[Command completed with exit code 8]";
    assert_eq!(strip_exit_code_suffix(output), "");
}

#[test]
fn test_transform_bash_output_lines_error_promoted_to_header() {
    // 覆盖图 1 到图 2 的真实样例
    let input = "CI Checks Summary:\n  [ok] Passed: 0\n  [FAIL] Failed: 0\n  [pending] Pending: 3\n\n[Exit code: 8]";
    let mut buf = String::new();
    let lines = transform_bash_output_lines(input, Some(8), &mut buf);

    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0].prefix, BRANCH_PREFIX);
    assert_eq!(lines[0].text, "Error: Exit code 8");
    assert!(lines[0].is_error_header);

    assert_eq!(lines[1].prefix, INDENT_PREFIX);
    assert_eq!(lines[1].text, "CI Checks Summary:");
    assert_eq!(lines[2].prefix, INDENT_PREFIX);
    assert_eq!(lines[2].text, "  [ok] Passed: 0");
    assert_eq!(lines[3].prefix, INDENT_PREFIX);
    assert_eq!(lines[3].text, "  [FAIL] Failed: 0");
    assert_eq!(lines[4].prefix, INDENT_PREFIX);
    assert_eq!(lines[4].text, "  [pending] Pending: 3");
}

#[test]
fn test_transform_bash_output_lines_empty_body_failure() {
    let input = "[Command completed with exit code 2]";
    let mut buf = String::new();
    let lines = transform_bash_output_lines(input, Some(2), &mut buf);

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].prefix, BRANCH_PREFIX);
    assert_eq!(lines[0].text, "Error: Exit code 2");
    assert!(lines[0].is_error_header);
}

#[test]
fn test_transform_bash_output_lines_success_retains_first_line_branch() {
    let input = "line 1\nline 2";
    let mut buf = String::new();
    let lines = transform_bash_output_lines(input, None, &mut buf);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].prefix, BRANCH_PREFIX);
    assert_eq!(lines[0].text, "line 1");
    assert!(!lines[0].is_error_header);
    assert_eq!(lines[1].prefix, INDENT_PREFIX);
    assert_eq!(lines[1].text, "line 2");
}
