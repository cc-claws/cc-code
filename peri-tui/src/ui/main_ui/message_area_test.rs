use super::*;
use crate::ui::render_thread::WrappedLineInfo;
use crate::ui::theme;

fn wrapped_line(line_idx: usize, start: usize, end: usize) -> WrappedLineInfo {
    WrappedLineInfo {
        line_idx,
        visual_row_start: start,
        visual_row_end: end,
        plain_text: String::new(),
        char_widths: Vec::new(),
    }
}

/// 检查 span 是否有选区背景色
fn has_selection_bg(style: Style) -> bool {
    matches!(style.bg, Some(theme::SELECTION_BG))
}

#[test]
fn test_committed_visual_start_allows_large_visual_rows() {
    let wrap_map = vec![
        wrapped_line(0, 0, u16::MAX as usize + 10),
        wrapped_line(1, u16::MAX as usize + 10, u16::MAX as usize + 20),
    ];
    let result = committed_visual_start(1, 2, u16::MAX as usize + 20, &wrap_map);
    assert_eq!(result, u16::MAX as usize + 10);
}

#[test]
fn test_highlight_line_spans_full_span() {
    let spans = vec![Span::from("Hello"), Span::from("World")];
    let result = highlight_line_spans(spans, 0, 10);
    assert_eq!(result.len(), 2);
    assert!(has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
}

#[test]
fn test_highlight_line_spans_partial_start() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 3, 10);
    // 前 3 字符原样，后 2 字符选区背景
    assert_eq!(result.len(), 2);
    assert!(!has_selection_bg(result[0].style));
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[0].content, "Hel");
    assert_eq!(result[1].content, "lo");
}

#[test]
fn test_highlight_line_spans_partial_both() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 1, 4);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].content, "H");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "ell");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "o");
    assert!(!has_selection_bg(result[2].style));
}

#[test]
fn test_highlight_line_spans_multi_span() {
    let spans = vec![Span::from("Hel"), Span::from("lo Wo"), Span::from("rld")];
    let result = highlight_line_spans(spans, 2, 8);
    // 选中范围 char 2..8 = "llo Wo"
    // span0 "Hel": 前 2 原样 + 后 1 选区背景
    // span1 "lo Wo": 全部选区背景
    // span2 "rld": 不在选区（span2 starts at char 8）
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].content, "He");
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[1].content, "l");
    assert!(has_selection_bg(result[1].style));
    assert_eq!(result[2].content, "lo Wo");
    assert!(has_selection_bg(result[2].style));
    assert_eq!(result[3].content, "rld");
    assert!(!has_selection_bg(result[3].style));
}

#[test]
fn test_highlight_line_spans_outside() {
    let spans = vec![Span::from("Hello")];
    let result = highlight_line_spans(spans, 10, 15);
    assert_eq!(result.len(), 1);
    assert!(!has_selection_bg(result[0].style));
    assert_eq!(result[0].content, "Hello");
}

#[test]
fn test_committed_visual_start_uses_next_uncommitted_line() {
    let wrap_map = vec![
        wrapped_line(0, 0, 1),
        wrapped_line(1, 1, 4),
        wrapped_line(2, 4, 5),
    ];
    let result = committed_visual_start(2, 3, 5, &wrap_map);
    assert_eq!(result, 4);
}

#[test]
fn test_committed_visual_start_all_committed_uses_total_visual_rows() {
    let wrap_map = vec![wrapped_line(0, 0, 2), wrapped_line(1, 2, 5)];
    let result = committed_visual_start(2, 2, 5, &wrap_map);
    assert_eq!(result, 5);
}
