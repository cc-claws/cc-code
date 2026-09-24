//! Issue #232 的独立渲染核验：调用 ratatui 实际绘制，不复制私有公式。
use super::*;
use crate::{
    app::MessageViewModel,
    ui::{main_ui::message_area::render_messages, render_thread::WrappedLineInfo},
};
use peri_widgets::unified_vertical_scrollbar;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{ScrollbarState, StatefulWidget},
};

fn make_render(
    max_scroll: usize,
    offset: usize,
    viewport: Option<usize>,
    total_content: bool,
) -> Buffer {
    let area = Rect::new(0, 0, 1, 30);
    let mut buffer = Buffer::empty(area);
    let mut state =
        ScrollbarState::new(max_scroll + if total_content { 30 } else { 1 }).position(offset);
    if let Some(viewport) = viewport {
        state = state.viewport_content_length(viewport);
    }
    unified_vertical_scrollbar().render(area, &mut buffer, &mut state);
    buffer
}

fn make_history(app: &mut App, count: usize) {
    let mut cache = app.session_mgr.current().messages.render_cache.write();
    cache.width = 79;
    cache.total_lines = count;
    cache.lines = (0..count)
        .map(|i| Line::from(format!("历史 {i}")))
        .collect();
    cache.wrap_map = (0..count)
        .map(|i| WrappedLineInfo {
            line_idx: i,
            visual_row_start: i,
            visual_row_end: i + 1,
            plain_text: format!("历史 {i}"),
            char_widths: vec![],
        })
        .collect();
}

#[tokio::test]
async fn test_scrollbar_audit_track_click_then_drag_uses_real_event_and_render_paths() {
    let (mut app, mut handle) = App::new_headless(80, 30).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    for max_scroll in [20, 60, 1000] {
        make_history(&mut app, max_scroll + 30);
        for row in 0..30 {
            let ui = &mut app.session_mgr.current_mut().ui;
            ui.scroll_offset = 0;
            ui.scroll_follow = false;
            ui.scrollbar_hover = true;
            ui.message_scrollbar_dragging = false;
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("绘制起点");
            let before = app
                .session_mgr
                .current()
                .ui
                .message_scrollbar_metrics
                .expect("滚动条存在");
            if row < before.thumb_area.bottom() {
                continue;
            }
            assert!(handle_message_scrollbar_down(&mut app, row, 79));
            let anchor = app.session_mgr.current().ui.scroll_offset;
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("绘制点击结果");
            assert_eq!(
                handle.terminal.backend().buffer()[(79, row)].symbol(),
                "█",
                "点击后鼠标必须落在真实滑块上"
            );
            assert!(handle_message_scrollbar_drag(&mut app, row));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                anchor,
                "点击后原地拖不能跳动"
            );
            assert!(handle_message_scrollbar_drag(
                &mut app,
                row.saturating_sub(1)
            ));
            assert!(
                app.session_mgr.current().ui.scroll_offset <= anchor,
                "向上拖方向正确"
            );
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("绘制拖动结果");
            assert_eq!(
                handle.terminal.backend().buffer()[(79, row.saturating_sub(1))].symbol(),
                "█",
                "点击后继续拖动，鼠标必须仍在滑块内：max={max_scroll}, row={row}"
            );
            assert!(
                app.session_mgr.current().ui.message_scrollbar_dragging,
                "正常重绘不能取消拖动"
            );
            assert!(handle_message_scrollbar_drag(&mut app, row));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                anchor,
                "回到点击位置不能累积漂移"
            );
        }
    }
}

#[tokio::test]
async fn test_scrollbar_audit_streaming_keeps_drag_until_scroll_range_disappears() {
    let (mut app, mut handle) = App::new_headless(80, 30).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    make_history(&mut app, 90);
    app.session_mgr.current_mut().ui.scrollbar_hover = true;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("绘制历史");
    let thumb = app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_metrics
        .expect("滚动条存在")
        .thumb_area;
    assert!(handle_message_scrollbar_down(&mut app, thumb.y, 79));
    for loading in [true, false, true, false] {
        app.session_mgr.current_mut().ui.loading = loading;
        handle
            .terminal
            .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
            .expect("切换 spinner");
        assert!(
            app.session_mgr.current().ui.message_scrollbar_dragging,
            "仍可滚动时 spinner 切换不能中断拖动"
        );
    }
    make_history(&mut app, 29);
    app.session_mgr.current_mut().ui.loading = true;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("仅 spinner 撑出滚动范围");
    assert!(app.session_mgr.current().ui.message_scrollbar_dragging);
    app.session_mgr.current_mut().ui.loading = false;
    handle
        .terminal
        .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
        .expect("内容完整容纳");
    assert_eq!(app.session_mgr.current().ui.scrollbar_max_offset, 0);
    assert!(app
        .session_mgr
        .current()
        .ui
        .message_scrollbar_metrics
        .is_none());
    assert!(
        !app.session_mgr.current().ui.message_scrollbar_dragging,
        "已无内容可滚时取消拖动是正确清理"
    );
}

#[test]
fn test_scrollbar_audit_track_click_lands_within_thumb_not_necessarily_center() {
    for max_scroll in [20usize, 60, 1000] {
        for offset in [0, max_scroll / 2, max_scroll] {
            let before = make_render(max_scroll, offset, None, false);
            let rows: Vec<_> = (0..30)
                .filter(|&row| before[(0, row)].symbol() == "█")
                .collect();
            let metrics = MessageScrollbarMetrics {
                bar_area: Rect::new(0, 0, 1, 30),
                thumb_area: Rect::new(0, rows[0], 1, rows.len() as u16),
                max_offset: max_scroll,
                up_btn_area: None,
                down_btn_area: None,
            };
            for row in 0..30 {
                if before[(0, row)].symbol() == "█" {
                    continue;
                }
                let clicked_offset = message_scrollbar_offset_for_row(metrics, row);
                let after = make_render(max_scroll, clicked_offset, None, false);
                assert_eq!(
                    after[(0, row)].symbol(),
                    "█",
                    "点击轨道后鼠标应落在滑块内：max={max_scroll}, offset={offset}, row={row}"
                );
            }
        }
    }
}

#[test]
fn test_scrollbar_audit_explicit_viewport_is_identical_and_total_content_breaks_bottom() {
    for max_scroll in [20usize, 60, 1000] {
        for offset in [0, max_scroll / 2, max_scroll] {
            assert_eq!(
                make_render(max_scroll, offset, None, false),
                make_render(max_scroll, offset, Some(30), false),
                "默认视口就是 area.height，显式设置不改变滑块尺寸"
            );
        }
        let correct = make_render(max_scroll, max_scroll, None, false);
        assert_eq!(correct[(0, 29)].symbol(), "█", "实际底部必须对应轨道底部");
    }
    let suggested = make_render(20, 20, Some(30), true);
    assert_ne!(
        suggested[(0, 29)].symbol(),
        "█",
        "Issue 建议的 total_content 参数会使内容到底但滑块不到底"
    );
}
