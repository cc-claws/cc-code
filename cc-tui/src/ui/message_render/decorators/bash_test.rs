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
    let lines = transform_bash_output_lines(input, Some(8), &mut buf, BodyWindow::All).lines;

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
    let lines = transform_bash_output_lines(input, Some(2), &mut buf, BodyWindow::All).lines;

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].prefix, BRANCH_PREFIX);
    assert_eq!(lines[0].text, "Error: Exit code 2");
    assert!(lines[0].is_error_header);
}

#[test]
fn test_transform_bash_output_lines_success_retains_first_line_branch() {
    let input = "line 1\nline 2";
    let mut buf = String::new();
    let lines = transform_bash_output_lines(input, None, &mut buf, BodyWindow::All).lines;

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].prefix, BRANCH_PREFIX);
    assert_eq!(lines[0].text, "line 1");
    assert!(!lines[0].is_error_header);
    assert_eq!(lines[1].prefix, INDENT_PREFIX);
    assert_eq!(lines[1].text, "line 2");
}

#[test]
fn test_transform_head_lines_window_matches_full_prefix() {
    // 折叠态先截断再美化（issue #645）：可见前缀必须与全量美化逐字节一致
    let input = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n{\"d\":4}\n{\"e\":5}";
    let mut buf_full = String::new();
    let full = transform_bash_output_lines(input, None, &mut buf_full, BodyWindow::All);
    let mut buf_head = String::new();
    let head = transform_bash_output_lines(input, None, &mut buf_head, BodyWindow::HeadLines(3));
    let full_texts: Vec<&str> = full.lines.iter().map(|l| l.text.as_ref()).collect();
    let head_texts: Vec<&str> = head.lines.iter().map(|l| l.text.as_ref()).collect();
    assert!(
        full_texts.starts_with(&head_texts[..]),
        "窗口美化结果应是全量美化结果的前缀，head: {:?} full: {:?}",
        head_texts,
        full_texts
    );
    assert!(
        head.lines.len() < full.lines.len(),
        "窗口版行数应少于全量美化行数"
    );
    assert_eq!(head.source_total, 5, "源行总数应为 5");
    assert_eq!(head.source_taken, 3, "进入美化的源行应为 3");
}

#[test]
fn test_transform_head_window_skips_prettify_when_over_limit() {
    // 全文超 MAX_JSON_FORMAT_LENGTH 时整段跳过美化的行为必须保持
    let line = format!("{{\"pad\":\"{}\"}}", "x".repeat(2488));
    let input = [line.as_str(); 5].join("\n");
    let mut buf_full = String::new();
    let full = transform_bash_output_lines(&input, None, &mut buf_full, BodyWindow::All);
    let mut buf_head = String::new();
    let head = transform_bash_output_lines(&input, None, &mut buf_head, BodyWindow::HeadLines(3));
    let full_texts: Vec<&str> = full.lines.iter().map(|l| l.text.as_ref()).collect();
    let head_texts: Vec<&str> = head.lines.iter().map(|l| l.text.as_ref()).collect();
    assert!(full_texts.starts_with(&head_texts[..]), "超长场景前缀也应一致");
    assert_eq!(head_texts.len(), 3, "未美化时窗口应恰好取 3 个源行");
    assert_eq!(
        head_texts[0],
        input.lines().next().unwrap(),
        "超长内容不应被美化，应显示原文行"
    );
}

#[test]
fn test_transform_tail_lines_window_matches_full_suffix() {
    // 流式实时输出尾部窗口（issue #645）：可见后缀必须与全量美化一致
    let input = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n{\"d\":4}\n{\"e\":5}";
    let mut buf_full = String::new();
    let full = transform_bash_output_lines(input, None, &mut buf_full, BodyWindow::All);
    let mut buf_tail = String::new();
    let tail = transform_bash_output_lines(input, None, &mut buf_tail, BodyWindow::TailLines(2));
    let full_texts: Vec<&str> = full.lines.iter().map(|l| l.text.as_ref()).collect();
    let tail_texts: Vec<&str> = tail.lines.iter().map(|l| l.text.as_ref()).collect();
    assert!(
        full_texts.ends_with(&tail_texts[..]),
        "尾部窗口美化结果应是全量美化结果的后缀"
    );
    assert_eq!(tail.source_taken, 2, "尾部窗口应取 2 个源行");
}

#[test]
fn test_transform_head_window_with_error_header() {
    // 失败场景：错误头占首行，正文窗口化美化，源行统计基于剥离后的正文
    let input = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}\n{\"d\":4}\n\n[Exit code: 8]";
    let mut buf = String::new();
    let out = transform_bash_output_lines(input, Some(8), &mut buf, BodyWindow::HeadLines(2));
    assert_eq!(out.lines[0].text.as_ref(), "Error: Exit code 8");
    assert!(out.lines[0].is_error_header, "首行应为错误头");
    assert_eq!(out.source_total, 4, "源行总数应剥离 exit code 标记后计数");
    assert_eq!(out.source_taken, 2, "进入美化的源行应为 2");
}

#[test]
fn test_transform_head_chars_window_covers_char_prefix() {
    // 错误摘要按 400 字符截断：窗口须取到累计字符数满足为止
    let input = "{\"a\":1}\n{\"b\":2}\n{\"c\":3}";
    let mut buf = String::new();
    let out = transform_bash_output_lines(input, None, &mut buf, BodyWindow::HeadChars(400));
    assert_eq!(out.source_taken, 3, "字符数不足 400 时应取全部源行");
    let long_line = format!("{{\"pad\":\"{}\"}}", "x".repeat(450));
    let input2 = format!("{long_line}\n{{\"b\":2}}");
    let mut buf2 = String::new();
    let out2 = transform_bash_output_lines(&input2, None, &mut buf2, BodyWindow::HeadChars(400));
    assert_eq!(out2.source_taken, 1, "单行即超 400 字符时只取该行");
}
