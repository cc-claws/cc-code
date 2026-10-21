use super::*;
use crate::{
    app::{text_selection::extract_selected_text, MessageViewModel},
    ui::{main_ui::message_area::render_messages, render_thread::WrappedLineInfo},
};
use cc_widgets::markdown::LinkHit;
use ratatui::{crossterm::event::MouseEvent, layout::Rect, text::Line};

fn make_mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[tokio::test]
async fn test_welcome_prefix_preserves_message_selection_links_and_drag_coordinates() {
    let (mut app, mut handle) = App::new_headless(90, 30).await;
    app.global_ui.viewport.startup_welcome = true;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("message".into()));
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.width = 80;
        cache.total_lines = 3;
        cache.lines = ["alpha", "bravo", "charlie"]
            .into_iter()
            .map(Line::from)
            .collect();
        cache.wrap_map = ["alpha", "bravo", "charlie"]
            .into_iter()
            .enumerate()
            .map(|(row, text)| WrappedLineInfo {
                line_idx: row,
                visual_row_start: row,
                visual_row_end: row + 1,
                plain_text: text.into(),
                char_widths: vec![1; text.len()],
            })
            .collect();
        cache.links = vec![LinkHit {
            line: 1,
            g_start: 1,
            g_end: 4,
            url: "https://example.com/message".into(),
        }];
    }
    let area = Rect::new(4, 3, 80, 20);
    handle
        .terminal
        .draw(|frame| render_messages(frame, &mut app, Rect::default(), area))
        .expect("绘制非零起点欢迎与消息");
    let prefix = app.session_mgr.current().ui.message_prefix_rows as u16;
    assert!(prefix > 0);
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 1,
            area.y + 1,
        ),
    )
    .await
    .expect("点击欢迎卡");
    assert!(
        app.session_mgr.current().ui.screen_selection.dragging,
        "欢迎卡使用屏幕选区"
    );
    assert!(
        app.session_mgr.current().ui.text_selection.start.is_none(),
        "欢迎不能命中消息链接"
    );
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 1,
            area.y + prefix + 1,
        ),
    )
    .await
    .expect("点击缓存消息");
    let start = app
        .session_mgr
        .current()
        .ui
        .text_selection
        .start
        .expect("消息选区起点");
    assert_eq!(start, (1, 1), "消息坐标应扣除欢迎前缀和区域起点");
    {
        let cache = app.session_mgr.current().messages.render_cache.read();
        assert_eq!(
            find_clicked_link(start, &cache.wrap_map, &cache.links, area.width).as_deref(),
            Some("https://example.com/message"),
            "前缀不能改变消息链接命中"
        );
    }
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Drag(MouseButton::Left),
            area.x + 4,
            area.y + prefix + 2,
        ),
    )
    .await
    .expect("拖拽消息选区");
    let end = app
        .session_mgr
        .current()
        .ui
        .text_selection
        .end
        .expect("消息选区终点");
    assert_eq!(end, (2, 4));
    {
        let cache = app.session_mgr.current().messages.render_cache.read();
        assert_eq!(
            extract_selected_text(start, end, &cache.wrap_map, area.width).as_deref(),
            Some("ravo\nchar"),
            "复制应保持纯消息文本"
        );
    }
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 1,
            area.y + prefix + 1,
        ),
    )
    .await
    .expect("第一次点击消息行");
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 1,
            area.y + prefix + 1,
        ),
    )
    .await
    .expect("双击消息行");
    assert_eq!(
        app.session_mgr.current().ui.text_selection.start,
        Some((1, 0))
    );
    assert_eq!(
        app.session_mgr.current().ui.text_selection.end,
        Some((1, area.width)),
        "双击整行也必须使用缓存坐标"
    );
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 2,
            area.y + prefix,
        ),
    )
    .await
    .expect("从消息首行开始拖拽");
    handle_event(
        &mut app,
        make_mouse(
            MouseEventKind::Drag(MouseButton::Left),
            area.x + 2,
            area.y + 1,
        ),
    )
    .await
    .expect("拖到欢迎前缀");
    assert_eq!(
        app.session_mgr.current().ui.text_selection.end,
        Some((0, 0)),
        "拖入欢迎区应钳到消息起点，不能被当作原地链接点击"
    );
}
