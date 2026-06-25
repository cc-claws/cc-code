use super::*;

fn make_items(n: usize) -> Vec<i32> {
    (0..n as i32).collect()
}

#[test]
fn test_panel_list_new_empty() {
    let list: PanelList<i32> = PanelList::new();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.cursor(), 0);
    assert_eq!(list.scroll_offset(), 0);
}

#[test]
fn test_panel_list_move_cursor_clamp() {
    let mut list = PanelList::new();
    list.set_items(make_items(5));
    list.move_cursor(1);
    assert_eq!(list.cursor(), 1);
    list.move_cursor(100);
    assert_eq!(list.cursor(), 4);
    list.move_cursor(-100);
    assert_eq!(list.cursor(), 0);
}

#[test]
fn test_panel_list_move_cursor_no_wrap() {
    let mut list = PanelList::new();
    list.set_items(make_items(3));
    list.move_cursor(-1); // 在顶部向上
    assert_eq!(list.cursor(), 0, "不应循环到末尾");
    list.move_cursor(1);
    list.move_cursor(1);
    list.move_cursor(1); // 在底部向下
    assert_eq!(list.cursor(), 2, "不应循环到开头");
}

#[test]
fn test_panel_list_set_items_clamp_cursor() {
    let mut list = PanelList::new();
    list.set_items(make_items(10));
    list.move_cursor(9);
    assert_eq!(list.cursor(), 9);
    list.set_items(make_items(3));
    assert_eq!(list.cursor(), 2, "缩短列表后 cursor 应被 clamp");
}

#[test]
fn test_panel_list_handle_scroll_clamp() {
    let mut list = PanelList::new();
    list.set_items(make_items(10));
    list.handle_scroll(-5, 5);
    assert_eq!(list.scroll_offset(), 0, "不应滚动到负值");
}

#[test]
fn test_panel_list_handle_scroll_down_clamp() {
    let mut list = PanelList::new();
    list.set_items(make_items(10));
    list.handle_scroll(100, 5);
    // max_scroll = 10 - 5 = 5
    assert_eq!(
        list.scroll_offset(),
        5,
        "不应超过 items.len - visible_height"
    );
}

#[test]
fn test_panel_list_ensure_visible_up() {
    let mut list = PanelList::new();
    list.set_items(make_items(20));
    list.scroll_offset = 10;
    list.cursor = 5; // cursor 在视口上方
    list.ensure_visible(10);
    assert_eq!(list.scroll_offset(), 5, "scroll_offset 应跟随 cursor 上移");
}

#[test]
fn test_panel_list_ensure_visible_down() {
    let mut list = PanelList::new();
    list.set_items(make_items(20));
    list.scroll_offset = 0;
    list.cursor = 15; // cursor 在视口下方
    list.ensure_visible(10);
    assert_eq!(list.scroll_offset(), 6, "scroll_offset 应跟随 cursor 下移");
}

#[test]
fn test_panel_list_handle_mouse_click_valid() {
    let mut list = PanelList::new();
    list.set_items(make_items(10));
    let area = Rect::new(10, 5, 30, 15);
    // border_top=1, mouse 在 area.y + 1 + 3 = row 9 → item index 3
    let hit = list.handle_mouse_click(9, 20, area, 1);
    assert!(hit);
    assert_eq!(list.cursor(), 3);
}

#[test]
fn test_panel_list_handle_mouse_click_outside() {
    let mut list = PanelList::new();
    list.set_items(make_items(10));
    let area = Rect::new(10, 5, 30, 15);
    // mouse 在 area.y - 1 = row 4 → 不在面板内
    let hit = list.handle_mouse_click(4, 20, area, 1);
    assert!(!hit);
}

#[test]
fn test_panel_list_handle_mouse_click_below_items() {
    let mut list = PanelList::new();
    list.set_items(make_items(3));
    let area = Rect::new(10, 5, 30, 15);
    // border_top=1, mouse 在 area.y + 1 + 10 = row 16 → item index 10 > len(3)
    let hit = list.handle_mouse_click(16, 20, area, 1);
    assert!(!hit);
}

#[test]
fn test_panel_list_visible_range() {
    let mut list = PanelList::new();
    list.set_items(make_items(20));
    list.scroll_offset = 5;
    let range = list.visible_range(10);
    assert_eq!(range, 5..15);
}
