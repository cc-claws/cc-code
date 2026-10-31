use super::*;
use crate::app::MessageViewModel;
use ratatui::crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Rect;

async fn make_scroll_app() -> App {
    let (mut app, _) = App::new_headless(80, 30).await;
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::user("历史内容".into())];
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = 100;
    ui.scroll_follow = false;
    ui.scrollbar_min_offset = 0;
    ui.scrollbar_max_offset = 300;
    ui.messages_area = Some(Rect::new(0, 1, 79, 20));
    app
}

#[tokio::test]
async fn test_scroll_anchor_keyboard_and_wheel_actions_advance_user_revision() {
    let events = vec![
        Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    ];
    for event in events {
        let mut app = make_scroll_app().await;
        let revision = app.session_mgr.current().ui.scroll_revision;
        let label = format!("{event:?}");
        handle_event(&mut app, event).await.expect("输入处理应成功");
        assert!(
            app.session_mgr.current().ui.scroll_revision > revision,
            "用户定位必须作废旧锚点：{label}"
        );
    }
}

#[tokio::test]
async fn test_scroll_anchor_track_end_click_follows_but_thumb_grab_pauses() {
    let mut app = make_scroll_app().await;
    let metrics = MessageScrollbarMetrics {
        bar_area: Rect::new(79, 1, 1, 20),
        thumb_area: Rect::new(79, 6, 1, 3),
        max_offset: 300,
        up_btn_area: None,
        down_btn_area: None,
    };
    app.session_mgr.current_mut().ui.message_scrollbar_metrics = Some(metrics);
    let revision = app.session_mgr.current().ui.scroll_revision;
    assert!(handle_message_scrollbar_down(&mut app, 20, 79));
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        300,
        "点击轨道底端应定位末尾"
    );
    assert!(
        app.session_mgr.current().ui.scroll_follow,
        "主动定位末尾必须恢复跟随"
    );
    assert!(app.session_mgr.current().ui.scroll_revision > revision);
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.message_scrollbar_dragging = false;
    ui.scroll_offset = usize::MAX;
    ui.scroll_follow = true;
    ui.message_scrollbar_metrics = Some(MessageScrollbarMetrics {
        thumb_area: Rect::new(79, 18, 1, 3),
        ..metrics
    });
    let revision = ui.scroll_revision;
    assert!(handle_message_scrollbar_down(&mut app, 19, 79));
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        300,
        "抓住底部滑块应冻结精确位置"
    );
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "抓取期间不能继续追尾"
    );
    assert!(app.session_mgr.current().ui.scroll_revision > revision);
}

#[tokio::test]
async fn test_scroll_anchor_selection_auto_scroll_clamps_tail_sentinel_and_advances_revision() {
    let mut app = make_scroll_app().await;
    let ui = &mut app.session_mgr.current_mut().ui;
    ui.scroll_offset = usize::MAX;
    ui.scroll_follow = true;
    ui.text_selection.start = Some((300, 0));
    ui.text_selection.end = Some((300, 0));
    ui.text_selection.dragging = true;
    let revision = ui.scroll_revision;
    handle_event(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 1,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }),
    )
    .await
    .expect("拖选处理应成功");
    let ui = &app.session_mgr.current().ui;
    assert!(ui.scroll_offset < 300, "从跟底哨兵向上拖选应先钳位再滚动");
    assert!(!ui.scroll_follow);
    assert!(ui.scroll_revision > revision, "拖选滚动也须作废在途锚点");
}
