use super::*;
use ratatui::{backend::TestBackend, text::Line, Terminal};

#[test]
fn scroll_state_ensure_visible_above() {
    let mut state = ScrollState { offset: 5 };
    state.ensure_visible(2, 10);
    assert_eq!(state.offset(), 2);
}

#[test]
fn scroll_state_ensure_visible_below() {
    let mut state = ScrollState { offset: 0 };
    state.ensure_visible(15, 10);
    assert_eq!(state.offset(), 6); // 15 - (10-1) = 6
}

#[test]
fn scroll_state_ensure_visible_within() {
    let mut state = ScrollState { offset: 3 };
    state.ensure_visible(5, 10);
    assert_eq!(state.offset(), 3);
}

#[test]
fn scroll_state_scroll_up_down() {
    let mut state = ScrollState::new();
    state.scroll_down(3, 20, 10);
    assert_eq!(state.offset(), 3);
    state.scroll_up(1);
    assert_eq!(state.offset(), 2);
}

#[test]
fn scrollable_area_renders_content() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'_>> = (0..20).map(|i| Line::from(format!("Line {}", i))).collect();
    let content = Text::from(lines);
    let mut scroll_state = ScrollState::new();
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            ScrollableArea::new(content).render(f, area, &mut scroll_state);
        })
        .unwrap();
}

#[test]
fn scrollable_area_clamps_offset() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let lines: Vec<Line<'_>> = (0..20).map(|i| Line::from(format!("Line {}", i))).collect();
    let content = Text::from(lines);
    let mut scroll_state = ScrollState { offset: 100 };
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            ScrollableArea::new(content).render(f, area, &mut scroll_state);
        })
        .unwrap();
    // 20 lines, 5 visible -> max_scroll = 15
    assert_eq!(scroll_state.offset(), 15);
}

#[test]
fn scroll_state_reset() {
    let mut state = ScrollState { offset: 10 };
    state.reset();
    assert_eq!(state.offset(), 0);
}

#[test]
fn scroll_state_with_offset() {
    let state = ScrollState::with_offset(5);
    assert_eq!(state.offset(), 5);
}

#[test]
fn scroll_state_ensure_visible_zero_height() {
    let mut state = ScrollState { offset: 5 };
    state.ensure_visible(10, 0);
    assert_eq!(state.offset(), 0, "visible_height=0 应重置 offset");
}

#[test]
fn scroll_state_scroll_down_clamps_to_max() {
    let mut state = ScrollState::new();
    state.scroll_down(100, 10, 5);
    assert_eq!(
        state.offset(),
        5,
        "offset 应不超过 content_height - visible_height"
    );
}
