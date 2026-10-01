use super::*;

#[test]
fn test_start_drag_sets_coords() {
    let mut ts = TextSelection::new();
    ts.start_drag(5, 10);
    assert_eq!(ts.start, Some((5, 10)));
    assert_eq!(ts.end, Some((5, 10)));
    assert!(ts.dragging);
    assert!(ts.selected_text.is_none());
}

#[test]
fn test_update_drag_moves_end() {
    let mut ts = TextSelection::new();
    ts.start_drag(0, 0);
    ts.update_drag(3, 8);
    assert_eq!(ts.start, Some((0, 0)));
    assert_eq!(ts.end, Some((3, 8)));
}

#[test]
fn test_end_drag_stops_dragging() {
    let mut ts = TextSelection::new();
    ts.start_drag(1, 2);
    ts.end_drag();
    assert!(!ts.dragging);
    assert_eq!(ts.start, Some((1, 2)));
    assert_eq!(ts.end, Some((1, 2)));
}

#[test]
fn test_clear_resets_all() {
    let mut ts = TextSelection::new();
    ts.start_drag(5, 10);
    ts.update_drag(8, 20);
    ts.end_drag();
    ts.set_selected_text(Some("hello".into()));
    ts.clear();
    assert!(ts.start.is_none());
    assert!(ts.end.is_none());
    assert!(!ts.dragging);
    assert!(ts.selected_text.is_none());
}

#[test]
fn test_is_active() {
    let mut ts = TextSelection::new();
    assert!(!ts.is_active());
    ts.start_drag(0, 0);
    assert!(ts.is_active());
    ts.end_drag();
    assert!(!ts.is_active());
    ts.set_selected_text(Some("x".into()));
    assert!(ts.is_active());
}

// --- Task 3: 坐标映射和文本提取测试 ---

fn make_wrap_map_entry(
    line_idx: usize,
    start: usize,
    end: usize,
    text: &str,
) -> crate::ui::render_thread::WrappedLineInfo {
    let char_widths: Vec<u8> = text
        .chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u8)
        .collect();
    crate::ui::render_thread::WrappedLineInfo {
        line_idx,
        visual_row_start: start,
        visual_row_end: end,
        plain_text: text.to_string(),
        char_widths,
    }
}

#[test]
fn test_visual_to_logical_basic() {
    let wrap_map = vec![
        make_wrap_map_entry(0, 0, 1, "Hello"),
        make_wrap_map_entry(1, 1, 2, "World"),
    ];
    assert_eq!(visual_to_logical(0, 0, &wrap_map, 80), Some((0, 0)));
    assert_eq!(visual_to_logical(1, 0, &wrap_map, 80), Some((1, 0)));
}

#[test]
fn test_visual_to_logical_out_of_range() {
    let wrap_map = vec![make_wrap_map_entry(0, 0, 1, "Hello")];
    assert_eq!(visual_to_logical(99, 0, &wrap_map, 80), None);
}

#[test]
fn test_extract_selected_text_single_line() {
    let wrap_map = vec![make_wrap_map_entry(0, 0, 1, "Hello World")];
    let result = extract_selected_text((0, 2), (0, 8), &wrap_map, 80);
    // char 2..8 of "Hello World" = "llo Wo"
    assert_eq!(result, Some("llo Wo".to_string()));
}

#[test]
fn test_extract_selected_text_multi_line() {
    let wrap_map = vec![
        make_wrap_map_entry(0, 0, 1, "Line0"),
        make_wrap_map_entry(1, 1, 2, "Line1"),
        make_wrap_map_entry(2, 2, 3, "Line2"),
    ];
    let result = extract_selected_text((0, 0), (2, 5), &wrap_map, 80);
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_selected_text_swapped() {
    let wrap_map = vec![
        make_wrap_map_entry(0, 0, 1, "Line0"),
        make_wrap_map_entry(1, 1, 2, "Line1"),
        make_wrap_map_entry(2, 2, 3, "Line2"),
    ];
    let result = extract_selected_text((2, 5), (0, 0), &wrap_map, 80);
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_selected_text_partial_first_and_last() {
    let wrap_map = vec![
        make_wrap_map_entry(0, 0, 1, "Hello"),
        make_wrap_map_entry(1, 1, 2, "World"),
    ];
    let result = extract_selected_text((0, 2), (1, 3), &wrap_map, 80);
    assert_eq!(result, Some("llo\nWor".to_string()));
}

#[test]
fn test_char_col_to_offset_ascii() {
    let char_widths = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1]; // "ABCDEFGHIJ"
    let offset = char_col_to_offset(&char_widths, 5, 0, 80);
    assert_eq!(offset, 5);
}

#[test]
fn test_char_col_to_offset_cjk() {
    let char_widths = vec![2, 2, 2, 2]; // "你好世界"
    let offset = char_col_to_offset(&char_widths, 4, 0, 80);
    assert_eq!(offset, 2);
}

// --- PanelTextSelection tests ---

#[test]
fn test_panel_selection_lifecycle() {
    let mut ps = PanelTextSelection::new();
    assert!(!ps.is_active());
    ps.start_drag(2, 5);
    assert!(ps.is_active());
    assert_eq!(ps.start, Some((2, 5)));
    assert_eq!(ps.end, Some((2, 5)));
    ps.update_drag(4, 10);
    assert_eq!(ps.end, Some((4, 10)));
    ps.end_drag();
    assert!(!ps.dragging);
    ps.set_selected_text(Some("test".into()));
    assert!(ps.is_active());
    ps.clear();
    assert!(!ps.is_active());
}

// --- extract_panel_text tests ---

#[test]
fn test_extract_panel_text_single_line() {
    let lines = vec!["Hello World".to_string()];
    let result = extract_panel_text((0, 2), (0, 8), &lines);
    assert_eq!(result, Some("llo Wo".to_string()));
}

#[test]
fn test_extract_panel_text_multi_line() {
    let lines = vec![
        "Line0".to_string(),
        "Line1".to_string(),
        "Line2".to_string(),
    ];
    let result = extract_panel_text((0, 0), (2, 5), &lines);
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_panel_text_swapped() {
    let lines = vec![
        "Line0".to_string(),
        "Line1".to_string(),
        "Line2".to_string(),
    ];
    let result = extract_panel_text((2, 5), (0, 0), &lines);
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_panel_text_partial() {
    let lines = vec!["Hello".to_string(), "World".to_string()];
    let result = extract_panel_text((0, 2), (1, 3), &lines);
    assert_eq!(result, Some("llo\nWor".to_string()));
}

#[test]
fn test_extract_panel_text_out_of_range() {
    let lines = vec!["Hello".to_string()];
    let result = extract_panel_text((5, 0), (5, 3), &lines);
    assert_eq!(result, None);
}

// --- ScreenSelection / extract_snapshot_text tests ---

/// 按 unicode_width 构造 ScreenSnapshot：宽字符的后续占位 cell 用空串（模拟 ratatui Buffer）。
fn make_screen_snapshot(lines: &[&str]) -> ScreenSnapshot {
    let expanded: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            let mut cells = Vec::new();
            for c in line.chars() {
                let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                cells.push(c.to_string());
                for _ in 1..w {
                    cells.push(String::new()); // 宽字符占位 cell，symbol 为 ""
                }
            }
            cells
        })
        .collect();
    let width = expanded.iter().map(|r| r.len()).max().unwrap_or(0);
    let height = expanded.len();
    let mut symbols = Vec::with_capacity(width * height);
    for row in expanded {
        for col in 0..width {
            symbols.push(row.get(col).cloned().unwrap_or_else(|| " ".to_string()));
        }
    }
    ScreenSnapshot {
        symbols,
        width,
        height,
    }
}

#[test]
fn test_screen_selection_normalized_range_forward() {
    let ss = ScreenSelection {
        start: Some((1, 2)),
        end: Some((3, 5)),
        ..Default::default()
    };
    assert_eq!(ss.normalized_range(), Some((1, 2, 3, 5)));
}

#[test]
fn test_screen_selection_normalized_range_swapped() {
    let ss = ScreenSelection {
        start: Some((3, 5)),
        end: Some((1, 2)),
        ..Default::default()
    };
    assert_eq!(ss.normalized_range(), Some((1, 2, 3, 5)));
}

#[test]
fn test_screen_selection_normalized_range_empty() {
    let ss = ScreenSelection::default();
    assert_eq!(ss.normalized_range(), None);
}

#[test]
fn test_screen_selection_is_active_after_copy() {
    // 复制后 selected_text 被 take，但 start 仍在 → is_active 为真（高亮持续到点击他处）
    let mut ss = ScreenSelection::default();
    assert!(!ss.is_active());
    ss.start_drag(2, 3);
    assert!(ss.is_active());
    ss.end_drag();
    ss.set_selected_text(Some("text".into()));
    assert!(ss.is_active());
    ss.set_selected_text(None); // 模拟 copy 取走 selected_text
    assert!(ss.is_active()); // start 仍在 → 高亮持续
    ss.clear();
    assert!(!ss.is_active());
}

#[test]
fn test_extract_snapshot_text_single_line() {
    let snap = make_screen_snapshot(&["Hello World"]);
    // col 2..8（闭区间 [2,7]）= "llo Wo"
    let result = extract_snapshot_text(&snap, (0, 2), (0, 7));
    assert_eq!(result, Some("llo Wo".to_string()));
}

#[test]
fn test_extract_snapshot_text_multi_line() {
    let snap = make_screen_snapshot(&["Line0", "Line1", "Line2"]);
    let result = extract_snapshot_text(&snap, (0, 0), (2, 4));
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_snapshot_text_swapped() {
    let snap = make_screen_snapshot(&["Line0", "Line1", "Line2"]);
    let result = extract_snapshot_text(&snap, (2, 4), (0, 0));
    assert_eq!(result, Some("Line0\nLine1\nLine2".to_string()));
}

#[test]
fn test_extract_snapshot_text_partial_first_and_last() {
    let snap = make_screen_snapshot(&["Hello", "World"]);
    let result = extract_snapshot_text(&snap, (0, 2), (1, 2));
    assert_eq!(result, Some("llo\nWor".to_string()));
}

#[test]
fn test_extract_snapshot_text_cjk_placeholder() {
    // "你好AB" → 你(2列)+好(2列)+A+B = 6 cells，占位 cell symbol 为 ""
    let snap = make_screen_snapshot(&["你好AB"]);
    // 全行提取：占位 cell 贡献空串，结果应为 "你好AB"（不重复、不漏字）
    let result = extract_snapshot_text(&snap, (0, 0), (0, 5));
    assert_eq!(result, Some("你好AB".to_string()));
}

#[test]
fn test_extract_snapshot_text_cjk_multi_line() {
    let snap = make_screen_snapshot(&["你好", "World"]);
    let result = extract_snapshot_text(&snap, (0, 0), (1, 3));
    assert_eq!(result, Some("你好\nWorl".to_string()));
}

#[test]
fn test_extract_snapshot_text_trims_trailing_blank() {
    // 行尾空白 trim_end；首尾空行剔除（保留视觉干净）
    let snap = make_screen_snapshot(&["Hi  ", "    "]);
    let result = extract_snapshot_text(&snap, (0, 0), (1, 3));
    assert_eq!(result, Some("Hi".to_string()));
}

#[test]
fn test_extract_snapshot_text_out_of_range() {
    let snap = make_screen_snapshot(&["Hi"]);
    // 起始行超出高度 → None
    let result = extract_snapshot_text(&snap, (5, 0), (5, 3));
    assert_eq!(result, None);
}
