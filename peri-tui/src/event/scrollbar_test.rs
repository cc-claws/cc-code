use super::*;
use crate::app::MessageViewModel;
use crate::ui::{main_ui::message_area::render_messages, render_thread::WrappedLineInfo};
use ratatui::{layout::Rect, text::Line};

#[tokio::test]
async fn test_message_scrollbar_real_thumb_press_and_drag_do_not_jump() {
    let (mut app, mut handle) = App::new_headless(80, 20).await;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("历史".into()));
    for max_scroll in [50usize, 100, 200, 500, 1000, 2000, 10_000] {
        {
            let mut cache = app.session_mgr.current().messages.render_cache.write();
            cache.width = 79;
            cache.total_lines = max_scroll + 20;
            cache.lines = (0..cache.total_lines)
                .map(|i| Line::from(format!("历史 {i}")))
                .collect();
            cache.wrap_map = (0..cache.total_lines)
                .map(|i| WrappedLineInfo {
                    line_idx: i,
                    visual_row_start: i,
                    visual_row_end: i + 1,
                    plain_text: format!("历史 {i}"),
                    char_widths: vec![],
                })
                .collect();
        }
        for pct in [10, 30, 50, 70, 90] {
            let offset = max_scroll * pct / 100;
            let ui = &mut app.session_mgr.current_mut().ui;
            ui.scroll_offset = offset;
            ui.scroll_follow = false;
            ui.scrollbar_hover = true;
            ui.message_scrollbar_dragging = false;
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("绘制实际滑块");
            let metrics = app
                .session_mgr
                .current()
                .ui
                .message_scrollbar_metrics
                .expect("长内容必须有滚动条");
            let thumb = metrics.thumb_area;
            assert!(thumb.height > 0, "滑块命中区必须来自实际绘制结果");
            let row = thumb.y + thumb.height / 2;
            assert_eq!(
                handle.terminal.backend().buffer()[(thumb.x, row)].symbol(),
                "█"
            );
            assert!(handle_message_scrollbar_down(&mut app, row, thumb.x));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                offset,
                "点击滑块不能跳转：max={max_scroll}, pct={pct}"
            );
            assert!(handle_message_scrollbar_drag(&mut app, row));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                offset,
                "原地拖动不能改变内容"
            );
            assert!(handle_message_scrollbar_drag(
                &mut app,
                row.saturating_add(1)
            ));
            assert!(
                app.session_mgr.current().ui.scroll_offset >= offset,
                "向下拖动方向必须一致"
            );
            handle
                .terminal
                .draw(|f| render_messages(f, &mut app, Rect::default(), f.area()))
                .expect("绘制拖动结果");
            assert!(handle_message_scrollbar_drag(&mut app, row));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                offset,
                "回到锚点必须恢复精确偏移，不能累积舍入漂移"
            );
            assert!(handle_message_scrollbar_drag(
                &mut app,
                metrics.bar_area.bottom()
            ));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                max_scroll,
                "拖出轨道底部应到底"
            );
            assert!(handle_message_scrollbar_drag(&mut app, metrics.bar_area.y));
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                0,
                "拖到轨道顶部应到顶"
            );
        }
    }
}
