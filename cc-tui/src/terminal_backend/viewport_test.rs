use super::*;

#[test]
fn test_growing_viewport_does_not_shrink_or_grow_while_idle() {
    let mut state = ViewportState::growing(3);
    assert_eq!(
        state.prepare(11, Size::new(80, 24)).area,
        Rect::new(0, 3, 80, 11)
    );
    assert_eq!(state.prepare(11, Size::new(80, 24)).area.height, 11);
    assert_eq!(state.prepare(16, Size::new(80, 24)).area.height, 16);
    assert_eq!(state.prepare(8, Size::new(80, 24)).area.height, 16);
    assert!(!state.fullscreen_latched, "折叠和空闲刷新不能触发全屏");
}

#[test]
fn test_growing_viewport_latches_only_at_full_terminal_height() {
    let mut state = ViewportState::growing(3);
    state.prepare(11, Size::new(80, 24));
    assert!(!state.prepare(20, Size::new(80, 24)).enter_fullscreen);
    let plan = state.prepare(21, Size::new(80, 24));
    assert!(!plan.enter_fullscreen, "起点非零时触底不能提前全屏");
    assert_eq!(plan.area, Rect::new(0, 3, 80, 21));
    let plan = state.prepare(23, Size::new(80, 24));
    assert_eq!(plan.scroll_rows, 2);
    assert_eq!(plan.area, Rect::new(0, 1, 80, 23));
    assert!(!plan.enter_fullscreen, "比宿主少一行时仍应保持增长");
    let plan = state.prepare(24, Size::new(80, 24));
    assert!(plan.enter_fullscreen, "长满宿主高度时必须在绘制前切换");
    assert_eq!(plan.area, Rect::new(0, 0, 80, 24));
    let plan = state.prepare(4, Size::new(120, 40));
    assert!(!plan.enter_fullscreen, "全屏只能切换一次");
    assert!(state.fullscreen_latched, "折叠和放大终端不能解除锁定");
    assert_eq!(plan.area, Rect::new(0, 0, 120, 40));
}

#[test]
fn test_growing_viewport_near_bottom_scrolls_only_newly_required_rows() {
    let mut state = ViewportState::growing(22);
    let plan = state.prepare(11, Size::new(80, 24));
    assert_eq!(plan.scroll_rows, 9);
    assert_eq!(plan.area, Rect::new(0, 13, 80, 11));
    assert!(!state.fullscreen_latched, "初始预留空间本身不能触发全屏");
    let idle = state.prepare(11, Size::new(80, 24));
    assert_eq!(idle.scroll_rows, 0, "空闲刷新不能滚动");
    let grown = state.prepare(12, Size::new(80, 24));
    assert_eq!(grown.scroll_rows, 1, "从十一行长到十二行只滚动一行");
    assert_eq!(grown.area, Rect::new(0, 12, 80, 12));
    assert!(!grown.enter_fullscreen, "短内容不能因底部启动提前全屏");
    let shorter = state.prepare(8, Size::new(80, 24));
    assert_eq!(shorter.scroll_rows, 0);
    assert_eq!(shorter.area, grown.area, "折叠后起点与占用高度均保持");
    for rows in 13..24 {
        let plan = state.prepare(rows, Size::new(80, 24));
        assert_eq!(plan.scroll_rows, 1);
        assert_eq!(plan.area.bottom(), 24);
        assert!(!plan.enter_fullscreen, "未长满宿主时只滚动必要行");
    }
    let full = state.prepare(24, Size::new(80, 24));
    assert!(full.enter_fullscreen);
    assert_eq!(full.scroll_rows, 0, "切全屏不再滚动正常屏");
    assert_eq!(full.area, Rect::new(0, 0, 80, 24));
}

#[test]
fn test_growing_viewport_resize_preserves_height_then_latches_when_needed() {
    let mut state = ViewportState::growing(4);
    state.prepare(11, Size::new(80, 24));
    assert_eq!(state.prepare(8, Size::new(120, 40)).area.height, 11);
    assert!(!state.fullscreen_latched);
    assert!(!state.prepare(11, Size::new(40, 15)).enter_fullscreen);
    let smaller = state.prepare(8, Size::new(40, 14));
    assert_eq!(smaller.scroll_rows, 1);
    assert_eq!(smaller.area, Rect::new(0, 3, 40, 11));
    assert!(!smaller.enter_fullscreen, "缩窗后内容仍可容纳时只上移起点");
    assert!(state.prepare(8, Size::new(40, 11)).enter_fullscreen);
    assert_eq!(state.available_rows, 11);
}

#[test]
fn test_growing_viewport_large_coordinates_do_not_overflow() {
    let mut state = ViewportState::growing(u16::MAX);
    let plan = state.prepare(11, Size::new(80, u16::MAX));
    assert_eq!(plan.scroll_rows, 10);
    assert_eq!(plan.area.y, u16::MAX - 11);
    assert_eq!(plan.area.bottom(), u16::MAX);
    assert!(!plan.enter_fullscreen);
}

#[test]
fn test_growing_viewport_tiny_terminal_never_underflows() {
    for rows in [0, 1, 5, 11] {
        let mut state = ViewportState::growing(24);
        let plan = state.prepare(11, Size::new(1, rows));
        assert_eq!(plan.area, Rect::new(0, 0, 1, rows));
        assert!(state.fullscreen_latched);
    }
}

#[test]
fn test_growing_viewport_keeps_welcome_after_first_message() {
    let mut state = ViewportState::growing(0);
    let session = uuid::Uuid::nil();
    state.sync_welcome(session, false);
    state.sync_welcome(session, true);
    assert!(state.startup_welcome, "首条消息发送后欢迎卡仍保留");
    state.sync_welcome(uuid::Uuid::from_u128(1), true);
    assert!(!state.startup_welcome, "恢复已有消息的会话不补欢迎卡");
    state.sync_welcome(uuid::Uuid::from_u128(2), false);
    assert!(state.startup_welcome, "新建空会话重新显示欢迎卡");
}

#[test]
fn test_headless_viewport_defaults_to_existing_fullscreen_layout() {
    let state = ViewportState::default();
    assert!(state.fullscreen_latched);
    assert!(!state.startup_welcome);
}

#[test]
fn test_clear_owned_area_erases_only_owned_rows_and_restores_origin() {
    let mut output = Vec::new();
    clear_owned_area(&mut output, Rect::new(0, 4, 80, 3)).expect("清理正常屏应用区域");
    assert_eq!(
        output, b"\x1b[5;1H\x1b[2K\x1b[6;1H\x1b[2K\x1b[7;1H\x1b[2K\x1b[5;1H",
        "只擦除第 5 至 7 行，回到第 5 行，不碰上方 shell 历史、不追加换行"
    );
}

#[test]
fn test_clear_owned_area_empty_region_writes_nothing() {
    for area in [
        Rect::default(),
        Rect::new(0, 5, 80, 0),
        Rect::new(0, 5, 0, 3),
    ] {
        let mut output = Vec::new();
        clear_owned_area(&mut output, area).expect("空区域应安全返回");
        assert!(output.is_empty(), "空区域不能移动光标或清屏");
    }
}

#[test]
fn test_clear_owned_area_at_terminal_bottom_does_not_scroll() {
    let mut output = Vec::new();
    clear_owned_area(&mut output, Rect::new(0, 23, 80, 1)).expect("清理终端末行");
    assert_eq!(output, b"\x1b[24;1H\x1b[2K\x1b[24;1H");
    assert!(!output.contains(&b'\n'), "末行清理不能引起原生滚动");
}
