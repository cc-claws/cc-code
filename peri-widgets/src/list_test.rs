use super::*;
use ratatui::{backend::TestBackend, style::Color, Terminal};

#[test]
fn list_state_move_cursor_clamp() {
    let mut state = ListState::new(vec!["a", "b", "c"]);
    state.move_cursor(1);
    assert_eq!(state.cursor(), 1);
    state.move_cursor(5);
    assert_eq!(state.cursor(), 2); // clamped to max
    state.move_cursor(-10);
    assert_eq!(state.cursor(), 0); // clamped to 0
}

#[test]
fn list_state_empty_items() {
    let mut state: ListState<&str> = ListState::new(vec![]);
    state.move_cursor(1); // should not panic
    assert!(state.selected().is_none());
}

#[test]
fn list_state_set_items_clamp_cursor() {
    let mut state = ListState::new(vec!["a", "b", "c", "d"]);
    // move cursor to last item
    state.move_cursor(3);
    assert_eq!(state.cursor(), 3);
    // replace with shorter list
    state.set_items(vec!["x"]);
    assert_eq!(state.cursor(), 0); // clamped
}

#[test]
fn list_state_ensure_visible() {
    let mut state = ListState::new((0..20i32).collect());
    state.move_cursor(15);
    state.ensure_visible(10);
    assert_eq!(state.scroll.offset(), 6); // 15 - (10-1)
}

#[test]
fn test_list_state_hovered_within_viewport() {
    let state = ListState::new(vec!["a", "b", "c"]);
    let mut state = state;
    state.update_mouse(Some((1, 0)));
    assert_eq!(state.hovered(), Some(1));
}

#[test]
fn test_list_state_hovered_with_scroll_offset() {
    let mut state = ListState::new((0..20i32).collect());
    state.move_cursor(15);
    state.ensure_visible(10);
    assert_eq!(state.scroll.offset(), 6);
    state.update_mouse(Some((0, 0)));
    assert_eq!(state.hovered(), Some(6)); // row 0 + offset 6
}

#[test]
fn test_list_state_hovered_out_of_bounds() {
    let state = ListState::new(vec!["a", "b", "c"]);
    let mut state = state;
    state.update_mouse(Some((100, 0)));
    assert_eq!(state.hovered(), None);
}

#[test]
fn test_list_state_hovered_none_when_no_mouse() {
    let state = ListState::new(vec!["a", "b", "c"]);
    assert_eq!(state.hovered(), None);
}

#[test]
fn test_list_state_set_cursor_by_mouse() {
    let mut state = ListState::new((0..10i32).collect());
    state.move_cursor(5);
    state.ensure_visible(5);
    let offset = state.scroll.offset();
    state.set_cursor_by_mouse(2);
    assert_eq!(state.cursor(), 2 + offset as usize);
}

#[test]
fn selectable_list_renders_cursor_marker() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ListState::new(vec!["a", "b", "c"]);
    state.move_cursor(1); // cursor on "b"
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                Line::from(*item)
            });
            f.render_stateful_widget(list, area, &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    // Row 1 (cursor) should contain the marker prefix
    let row1: String = (area.x..area.x + area.width)
        .map(|x| buf.cell((x, area.y + 1)).unwrap().symbol().to_string())
        .collect();
    assert!(
        row1.contains("▶"),
        "Expected cursor marker on row 1, got: {:?}",
        row1
    );
    // Row 0 should contain spaces (non-cursor marker)
    let row0: String = (area.x..area.x + area.width)
        .map(|x| buf.cell((x, area.y)).unwrap().symbol().to_string())
        .collect();
    assert!(!row0.contains("▶"), "Expected no cursor marker on row 0");
}

#[test]
fn selectable_list_custom_render_item() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ListState::new(vec!["a", "b"]);
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                Line::from(format!("[{}]", item))
            });
            f.render_stateful_widget(list, area, &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    let row0: String = (area.x..area.x + area.width)
        .map(|x| buf.cell((x, area.y)).unwrap().symbol().to_string())
        .collect();
    assert!(
        row0.contains("[a]"),
        "Expected [a] on row 0, got: {:?}",
        row0
    );
}

#[test]
fn test_selectable_list_hover_style_applied() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ListState::new(vec!["a", "b", "c"]);
    state.update_mouse(Some((1, 0))); // hover on item 1
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                Line::from(*item)
            })
            .hover_style(Style::default().bg(Color::Blue));
            f.render_stateful_widget(list, area, &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    // Row 1 (hovered) should have blue background
    let cell = buf.cell((area.x + 2, area.y + 1)).unwrap();
    assert_eq!(cell.bg, Color::Blue, "Hovered 行应有蓝色背景");
    // Row 0 (normal) 不应有蓝色背景
    let cell0 = buf.cell((area.x + 2, area.y)).unwrap();
    assert_ne!(cell0.bg, Color::Blue);
}

#[test]
fn test_selectable_list_cursor_overrides_hover() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ListState::new(vec!["a", "b", "c"]);
    state.move_cursor(1); // cursor on item 1
    state.update_mouse(Some((1, 0))); // hover on item 1 (same as cursor)
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                Line::from(*item)
            })
            .cursor_style(Style::default().bg(Color::Red))
            .hover_style(Style::default().bg(Color::Blue));
            f.render_stateful_widget(list, area, &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    // Row 1: cursor 和 hover 重合，应使用 cursor_style (Red)
    let cell = buf.cell((area.x + 2, area.y + 1)).unwrap();
    assert_eq!(cell.bg, Color::Red, "cursor 应覆盖 hover 样式");
}

#[test]
fn test_selectable_list_no_mouse_unchanged() {
    // 回归测试：不调用 update_mouse 时渲染结果应与原有行为一致
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ListState::new(vec!["a", "b"]);
    state.move_cursor(1);
    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 5);
            let list = SelectableList::new(|item: &&str, _is_cursor: bool, _is_hovered: bool| {
                Line::from(*item)
            });
            f.render_stateful_widget(list, area, &mut state);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    // Row 0 (non-cursor, no hover) 应为默认样式
    let cell0 = buf.cell((area.x + 2, area.y)).unwrap();
    assert_eq!(cell0.bg, Color::Reset);
    // Row 1 (cursor) 应为默认 cursor 样式
    let cell1 = buf.cell((area.x + 2, area.y + 1)).unwrap();
    assert_eq!(cell1.bg, Color::Reset);
}
