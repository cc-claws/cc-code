use super::*;
use unicode_width::UnicodeWidthStr;

#[test]
fn test_branch_prefix_constants_width() {
    assert_eq!(UnicodeWidthStr::width(BranchPrefix::PREFIX), 5);
    assert_eq!(UnicodeWidthStr::width(BranchPrefix::HANG), 5);
    assert_eq!(UnicodeWidthStr::width(BranchPrefix::NESTED_PREFIX), 8);
    assert_eq!(UnicodeWidthStr::width(BranchPrefix::NESTED_HANG), 8);
    assert_eq!(BranchPrefix::WIDTH, 5);
    assert_eq!(BranchPrefix::NESTED_WIDTH, 8);
}

#[test]
fn test_branch_prefix_span_styling() {
    let success_span = BranchPrefix::span(false);
    assert_eq!(success_span.content, BranchPrefix::PREFIX);
    assert_eq!(
        success_span.style.fg,
        Some(crate::ui::theme::cur().inactive)
    );

    let error_span = BranchPrefix::span(true);
    assert_eq!(error_span.content, BranchPrefix::PREFIX);
    assert_eq!(error_span.style.fg, Some(crate::ui::theme::cur().error));
}

#[test]
fn test_branch_prefix_nested_span_styling() {
    let success_span = BranchPrefix::nested_span(false);
    assert_eq!(success_span.content, BranchPrefix::NESTED_PREFIX);
    assert_eq!(
        success_span.style.fg,
        Some(crate::ui::theme::cur().inactive)
    );

    let error_span = BranchPrefix::nested_span(true);
    assert_eq!(error_span.content, BranchPrefix::NESTED_PREFIX);
    assert_eq!(error_span.style.fg, Some(crate::ui::theme::cur().error));
}

#[test]
fn test_branch_prefix_content_width_calculation() {
    assert_eq!(BranchPrefix::content_width(80), 75);
    assert_eq!(BranchPrefix::content_width(24), 20);
    assert_eq!(BranchPrefix::content_width(10), 20);

    assert_eq!(BranchPrefix::nested_content_width(80), 72);
    assert_eq!(BranchPrefix::nested_content_width(25), 20);
    assert_eq!(BranchPrefix::nested_content_width(10), 20);
}

#[test]
fn test_branch_prefix_is_branch_line() {
    let matching_line = Line::from(vec![
        Span::raw(BranchPrefix::PREFIX),
        Span::raw("Running… (1s)"),
    ]);
    assert!(BranchPrefix::is_branch_line(&matching_line));

    let old_4_col_line = Line::from(vec![Span::raw("  ⎿ "), Span::raw("Running… (1s)")]);
    assert!(!BranchPrefix::is_branch_line(&old_4_col_line));

    let empty_line = Line::default();
    assert!(!BranchPrefix::is_branch_line(&empty_line));
}

#[test]
fn test_branch_prefix_strip_prefix() {
    assert_eq!(
        BranchPrefix::strip_prefix("  ⎿  Read foo"),
        Some("Read foo")
    );
    assert_eq!(BranchPrefix::strip_prefix("⎿ bar"), Some("bar"));
    assert_eq!(BranchPrefix::strip_prefix("  ⎿ bar"), Some("bar"));
    assert_eq!(BranchPrefix::strip_prefix("normal text"), None);
}
