use super::*;

fn make_panel(entries: &[&str]) -> InputHistoryPanel {
    InputHistoryPanel::new(entries.iter().map(|entry| entry.to_string()).collect())
}

#[test]
fn test_total_reports_entry_count() {
    let panel = make_panel(&["a", "b", "c"]);
    assert_eq!(panel.total(), 3);
}

#[test]
fn test_move_cursor_wraps_from_first_to_last() {
    let mut panel = make_panel(&["a", "b", "c"]);
    panel.move_cursor(-1);
    assert_eq!(panel.cursor, 2);
}

#[test]
fn test_move_cursor_wraps_from_last_to_first() {
    let mut panel = make_panel(&["a", "b", "c"]);
    panel.cursor = 2;
    panel.move_cursor(1);
    assert_eq!(panel.cursor, 0);
}

#[test]
fn test_move_cursor_on_empty_history_stays_at_zero() {
    let mut panel = make_panel(&[]);
    panel.move_cursor(1);
    panel.move_cursor(-5);
    assert_eq!(panel.cursor, 0);
}

#[test]
fn test_selected_returns_entry_at_cursor() {
    let mut panel = make_panel(&["first", "second"]);
    assert_eq!(panel.selected(), Some("first"));
    panel.move_cursor(1);
    assert_eq!(panel.selected(), Some("second"));
}

#[test]
fn test_selected_is_none_for_empty_history() {
    let panel = make_panel(&[]);
    assert_eq!(panel.selected(), None);
}

#[test]
fn test_desired_height_clamps_between_min_and_max_rows() {
    let few = make_panel(&["a"]);
    assert_eq!(few.desired_height(40, 80), MIN_ROWS as u16);
    let many = make_panel(&["a"; MAX_ROWS + 10]);
    assert_eq!(many.desired_height(40, 80), MAX_ROWS as u16);
}

#[test]
fn test_desired_height_never_exceeds_screen() {
    let panel = make_panel(&["a"; MAX_ROWS + 10]);
    assert_eq!(panel.desired_height(10, 80), 6);
}
