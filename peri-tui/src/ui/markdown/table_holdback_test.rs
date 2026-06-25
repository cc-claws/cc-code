use super::*;

#[test]
fn test_count_pipe_columns_basic() {
    assert_eq!(count_pipe_columns("| a | b | c |"), 3);
    assert_eq!(count_pipe_columns("| a | b | c"), 3);
    assert_eq!(count_pipe_columns("| single |"), 1);
    assert_eq!(count_pipe_columns("no pipes"), 0);
    assert_eq!(count_pipe_columns(""), 0);
}

#[test]
fn test_count_pipe_columns_with_spaces() {
    assert_eq!(count_pipe_columns("|  Name  |  Value  |"), 2);
}

#[test]
fn test_is_separator_row() {
    assert!(is_separator_row("|---|---|"));
    assert!(is_separator_row("|------|-------|"));
    assert!(is_separator_row("|:---:|:---:|"));
    assert!(is_separator_row("|---|:---:|---:|"));
    assert!(!is_separator_row("| a | b |"));
    assert!(!is_separator_row("not a separator"));
    assert!(!is_separator_row(""));
}

#[test]
fn test_is_table_line() {
    assert!(is_table_line("| a | b |"));
    assert!(is_table_line("|---|---|"));
    assert!(is_table_line("  | a | b |")); // 前导空格
    assert!(!is_table_line("not a table"));
    assert!(!is_table_line(""));
}

#[test]
fn test_holdback_decision_no_table() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let decision = scanner.scan("just some text\nwith no tables\n");
    assert_eq!(decision, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_decision_complete_table() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let table = "| A | B |\n|---|---|\n| 1 | 2 |\n";
    let decision = scanner.scan(table);
    assert_eq!(decision, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_decision_incomplete_row() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 第三行不完整：只有 1 列，表头有 2 列
    let table = "| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(table);
    // 应该 holdback 未完成的行
    match decision {
        HoldbackDecision::Hold { .. } => {} // pass
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_holdback_decision_incomplete_last_line_no_newline() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 表头行未完成（不以 \n 结尾）
    let table = "| A | B";
    let decision = scanner.scan(table);
    match decision {
        HoldbackDecision::Hold { .. } => {} // pass
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_holdback_flush_on_non_streaming() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(false);
    let table = "| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(table);
    assert_eq!(decision, HoldbackDecision::FlushAll);
}

#[test]
fn test_holdback_after_row_completed() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 第一轮：不完整的行
    let table1 = "| A | B |\n|---|---|\n| 1";
    let decision1 = scanner.scan(table1);
    assert!(matches!(decision1, HoldbackDecision::Hold { .. }));

    // 第二轮：行已完成
    let table2 = "| A | B |\n|---|---|\n| 1 | 2 |\n";
    scanner.last_scan_len = 0; // 重置后重新扫描完整文本
    let decision2 = scanner.scan(table2);
    assert_eq!(decision2, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_table_after_paragraph() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let text = "Some paragraph\n\n| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(text);
    match decision {
        HoldbackDecision::Hold { holdback_offset } => {
            // holdback_offset 应指向表格开始位置之后的不完整行
            assert!(holdback_offset > 0, "holdback_offset 应大于 0");
        }
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_reset_clears_state() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    scanner.scan("| A | B |\n|---|---|\n| 1");
    scanner.reset();
    assert_eq!(scanner.last_scan_len, 0);
}
