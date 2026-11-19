//! `ScreenSelection::shift_rows`：弹窗撑开底部布局时，屏幕选区必须跟着消息区一起搬。
//!
//! 回归背景：用户拖选一段内容后不动鼠标，agent 再弹出审批/提问弹窗，消息区被往上压，
//! 而屏幕选区仍是绝对坐标，于是高亮盖进弹窗、复制出来的也还是旧行号上的内容。

use super::*;

fn selection(start: (u16, u16), end: (u16, u16)) -> ScreenSelection {
    let mut sel = ScreenSelection::start_drag(start.0, start.1);
    sel.update_drag(end.0, end.1);
    sel.end_drag();
    sel.set_selected_text(Some("picked".into()));
    sel
}

#[test]
fn shift_rows_moves_selection_up_when_popup_opens() {
    // 消息区顶边从第 3 行被压到第 1 行：选区整体上移 2 行。
    let mut sel = selection((10, 4), (12, 8));
    sel.shift_rows(-2, Some);
    assert_eq!(sel.start, Some((8, 4)));
    assert_eq!(sel.end, Some((10, 8)));
    assert!(sel.is_active(), "平移后高亮应继续存在");
}

#[test]
fn shift_rows_moves_selection_down_when_popup_closes() {
    let mut sel = selection((8, 4), (10, 8));
    sel.shift_rows(2, Some);
    assert_eq!(sel.start, Some((10, 4)));
    assert_eq!(sel.end, Some((12, 8)));
}

#[test]
fn shift_rows_keeps_columns() {
    let mut sel = selection((5, 0), (5, 30));
    sel.shift_rows(-1, Some);
    assert_eq!(sel.start, Some((4, 0)));
    assert_eq!(sel.end, Some((4, 30)));
}

#[test]
fn shift_rows_clears_selection_when_any_endpoint_is_covered() {
    // 选区的结尾被压到消息区可见范围之外：不能只留半截高亮继续复制屏幕外内容。
    let mut sel = selection((10, 4), (12, 8));
    let visible = 8..=11;
    sel.shift_rows(-2, |row| visible.contains(&row).then_some(row));
    assert!(!sel.is_active(), "端点被遮挡时必须整段作废");
    assert_eq!(sel.start, None);
    assert_eq!(sel.end, None);
    assert_eq!(sel.selected_text, None);
}

#[test]
fn shift_rows_clears_selection_when_start_is_covered() {
    let mut sel = selection((10, 4), (12, 8));
    let visible = 9..=20;
    sel.shift_rows(-2, |row| visible.contains(&row).then_some(row));
    assert!(!sel.is_active());
}

#[test]
fn shift_rows_on_inactive_selection_is_noop() {
    let mut sel = ScreenSelection::default();
    let mut called = false;
    sel.shift_rows(-3, |row| {
        called = true;
        Some(row)
    });
    assert!(!called, "没有选区时不应触发 remap");
    assert!(!sel.is_active());
}

#[test]
fn shift_rows_keeps_dragging_state_when_remap_accepts() {
    // 拖拽进行中用户同时触发弹窗：位移后仍应保持 dragging，鼠标继续拖能接上。
    let mut sel = ScreenSelection::start_drag(6, 2);
    sel.update_drag(9, 5);
    sel.shift_rows(-1, Some);
    assert!(sel.dragging);
    assert_eq!(sel.start, Some((5, 2)));
    assert_eq!(sel.end, Some((8, 5)));
}
