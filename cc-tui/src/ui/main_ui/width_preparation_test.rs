use super::*;
use crate::ui::message_view::MessageViewModel;
use ratatui::layout::Size;
use tokio::sync::{mpsc, oneshot};

async fn make_pending_app() -> (App, mpsc::Receiver<RenderEvent>) {
    let (mut app, _) = App::new_headless(120, 40).await;
    app.global_ui.viewport.terminal_size = Size::new(120, 40);
    app.global_ui.viewport.fullscreen_latched = false;
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .width = 120;
    let (sender, receiver) = mpsc::channel(8);
    app.session_mgr.current_mut().messages.render_tx = sender;
    (app, receiver)
}

fn make_width_reply(receiver: &mut mpsc::Receiver<RenderEvent>) -> oneshot::Sender<PreparedWidth> {
    let event = receiver.try_recv().expect("应投递一次宽度准备");
    match event.into_payload() {
        RenderEvent::PrepareWidth { reply, .. } => reply,
        _ => panic!("预期宽度准备事件"),
    }
}

fn make_complete_width(app: &App, reply: oneshot::Sender<PreparedWidth>, width: u16) {
    let mut cache = app.session_mgr.current().messages.render_cache.write();
    cache.width = width;
    reply
        .send(PreparedWidth {
            width,
            full_width_rows: cache.total_lines,
            measured_rows: cache.total_lines,
        })
        .expect("当前宽度请求仍应有效");
}

#[tokio::test]
async fn test_width_preparation_cannot_overtake_pending_view_snapshot_after_queue_full() {
    let (mut app, _) = make_pending_app().await;
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(sender.try_send(RenderEvent::Resize(77)).is_ok());
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::user("主视口历史".into())];
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(100, false);
    app.render_rebuild();
    receiver
        .try_recv()
        .expect("旧子视口事件已处理，队列腾出一格");
    assert!(!try_prepare_width(&mut app, 120, false));
    assert!(
        matches!(
            receiver.try_recv().expect("主视图必须先入队"),
            RenderEvent::Rebuild(_)
        ),
        "宽度准备不能越过待发主视图，拿旧子视图高度钳位"
    );
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        100,
        "待发期间保留恢复的位置"
    );
    assert!(!try_prepare_width(&mut app, 120, false));
    let reply = make_width_reply(&mut receiver);
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .total_lines = 500;
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 100);
    assert!(!app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_width_preparation_returns_pending_without_waiting_for_worker() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let reply = make_width_reply(&mut receiver);
    for _ in 0..20 {
        assert!(
            !try_prepare_width(&mut app, 120, false),
            "后台未回复时立即返回待处理"
        );
        app.session_mgr.current_mut().ui.textarea.insert_str("x");
    }
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines()[0],
        "x".repeat(20)
    );
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "同一布局不重复入队"
    );
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
}

#[tokio::test]
async fn test_width_preparation_stable_layout_skips_barriers_after_viewport_publication() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
    for row in 0..32 {
        app.session_mgr.current_mut().ui.scroll_offset = row;
        app.session_mgr
            .current()
            .messages
            .render_cache
            .write()
            .version += 1;
        assert!(
            try_prepare_width(&mut app, 120, false),
            "视口发布不能重新等待宽度屏障"
        );
    }
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn test_width_preparation_retries_when_render_channel_is_full() {
    let (mut app, _) = make_pending_app().await;
    let (sender, mut receiver) = mpsc::channel(1);
    sender.try_send(RenderEvent::Resize(77)).unwrap();
    app.session_mgr.current_mut().messages.render_tx = sender;
    assert!(!try_prepare_width(&mut app, 120, false));
    assert!(app
        .session_mgr
        .current()
        .messages
        .width_preparation
        .pending
        .is_none());
    assert!(matches!(receiver.try_recv(), Ok(RenderEvent::Resize(77))));
    assert!(!try_prepare_width(&mut app, 120, false));
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 120);
    assert!(
        try_prepare_width(&mut app, 120, false),
        "通道腾出空间后应正常完成"
    );
}

#[tokio::test]
async fn test_width_preparation_discards_previous_width_reply() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let stale = make_width_reply(&mut receiver);
    assert!(!try_prepare_width(&mut app, 40, false));
    assert!(
        stale
            .send(PreparedWidth {
                width: 120,
                full_width_rows: 1,
                measured_rows: 1,
            })
            .is_err(),
        "旧宽度的回复已失效"
    );
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 40);
    assert!(try_prepare_width(&mut app, 40, false));
    assert_eq!(
        app.session_mgr.current().messages.last_resize_width,
        Some(40)
    );
}

#[tokio::test]
async fn test_width_preparation_discards_reply_after_same_width_history_replacement() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let stale = make_width_reply(&mut receiver);
    app.session_mgr.current().messages.invalidate_layout();
    assert!(!try_prepare_width(&mut app, 120, false));
    assert!(stale
        .send(PreparedWidth {
            width: 120,
            full_width_rows: 999,
            measured_rows: 999,
        })
        .is_err());
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
    assert_eq!(
        app.session_mgr.current().ui.full_width_message_rows,
        Some((120, 0, 0)),
        "历史替换不能接受旧行数"
    );
}

#[tokio::test]
async fn test_width_preparation_mode_change_requires_new_confirmation() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let stale = make_width_reply(&mut receiver);
    app.toggle_detail_mode();
    assert!(!try_prepare_width(&mut app, 120, false));
    assert!(stale
        .send(PreparedWidth {
            width: 120,
            full_width_rows: 1,
            measured_rows: 1,
        })
        .is_err());
    assert!(matches!(
        receiver.try_recv(),
        Ok(RenderEvent::ToggleDetail(true))
    ));
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
}

#[tokio::test]
async fn test_width_preparation_streaming_updates_do_not_starve_completed_frame() {
    let (mut app, mut receiver) = make_pending_app().await;
    assert!(!try_prepare_width(&mut app, 120, false));
    let reply = make_width_reply(&mut receiver);
    app.session_mgr
        .current()
        .messages
        .send_render_event(RenderEvent::Rebuild(Vec::new()))
        .unwrap();
    make_complete_width(&app, reply, 120);
    assert!(
        try_prepare_width(&mut app, 120, false),
        "同视图持续更新也应先呈现已完成布局"
    );
    assert!(
        !try_prepare_width(&mut app, 120, false),
        "随后再追踪较新的内容版本"
    );
    assert!(matches!(receiver.try_recv(), Ok(RenderEvent::Rebuild(_))));
    let reply = make_width_reply(&mut receiver);
    make_complete_width(&app, reply, 120);
    assert!(try_prepare_width(&mut app, 120, false));
}

#[tokio::test]
async fn test_width_preparation_reflows_virtual_and_cache_before_confirmation() {
    for strategy in ["cache", "virtual"] {
        let (mut app, _) = App::new_headless(80, 40).await;
        app.global_ui.viewport.terminal_size = Size::new(80, 40);
        app.global_ui.viewport.fullscreen_latched = false;
        let messages = vec![MessageViewModel::user("中文长消息 mixed text ".repeat(40))];
        let state = &mut app.session_mgr.current_mut().messages;
        state.view_messages = messages.clone();
        state
            .send_render_event(RenderEvent::SetDetailStrategy(strategy.into()))
            .unwrap();
        state
            .send_render_event(RenderEvent::Rebuild(messages))
            .unwrap();
        prepare_width(&mut app, 80).await;
        let wide = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .total_lines;
        prepare_width(&mut app, 40).await;
        let narrow = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .total_lines;
        assert!(
            narrow > wide,
            "{strategy} 必须实际重排，而不是仅设置 width 字段"
        );
        prepare_width(&mut app, 80).await;
        assert_eq!(
            app.session_mgr
                .current()
                .messages
                .render_cache
                .read()
                .total_lines,
            wide
        );
    }
}

#[tokio::test]
async fn test_viewport_preparation_retains_resize_cleanup_while_pending() {
    let (mut app, mut receiver) = make_pending_app().await;
    let size = Size::new(80, 40);
    assert!(prepare_viewport(&mut app, size).is_none());
    let reply = make_width_reply(&mut receiver);
    for _ in 0..3 {
        assert!(prepare_viewport(&mut app, size).is_none());
        assert!(
            app.global_ui.terminal_reflow_pending,
            "更新已记录的尺寸不能丢弃尚未输出的清理请求"
        );
    }
    make_complete_width(&app, reply, 80);
    assert!(prepare_viewport(&mut app, size).is_some());
    assert!(
        app.global_ui.terminal_reflow_pending,
        "清理请求应保留到终端实际输出成功"
    );
}

#[tokio::test]
async fn test_viewport_preparation_commits_fullscreen_only_after_ready_during_streaming() {
    let (mut app, mut receiver) = make_pending_app().await;
    let size = Size::new(120, 10);
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::user("history".into()));
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .total_lines = 100;
    assert!(prepare_viewport(&mut app, size).is_none());
    let full_width_reply = make_width_reply(&mut receiver);
    make_complete_width(&app, full_width_reply, 120);
    assert!(prepare_viewport(&mut app, size).is_none());
    assert!(
        !app.global_ui.viewport.fullscreen_latched,
        "窄宽布局未完成时不能提前切屏"
    );
    let narrow_reply = make_width_reply(&mut receiver);
    app.session_mgr
        .current()
        .messages
        .send_render_event(RenderEvent::Rebuild(Vec::new()))
        .expect("模拟确认期间继续流式更新");
    make_complete_width(&app, narrow_reply, 119);
    let plan = prepare_viewport(&mut app, size).expect("已完成布局不能被新一轮内容更新饿死");
    assert!(plan.enter_fullscreen);
    assert!(app.global_ui.viewport.fullscreen_latched);
    assert!(matches!(receiver.try_recv(), Ok(RenderEvent::Rebuild(_))));
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}
