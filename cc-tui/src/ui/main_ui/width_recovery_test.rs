use super::*;
use crate::ui::{
    message_view::{MessageViewModel, ToolCategory, ToolEntry},
    render_thread::ViewportAnchor,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

async fn make_worker_barrier(app: &App, width: u16) {
    let (reply, ready) = oneshot::channel();
    app.session_mgr
        .current()
        .messages
        .render_tx
        .send(RenderEvent::ResizeAndWait(width, reply))
        .await
        .expect("布局屏障应入队");
    tokio::time::timeout(Duration::from_secs(5), ready)
        .await
        .expect("真实渲染线程应及时处理布局屏障")
        .expect("布局屏障不能丢失回复");
}

async fn make_fullscreen_app() -> App {
    let (mut app, _) = App::new_headless(80, 40).await;
    app.global_ui.viewport.terminal_size = Size::new(80, 40);
    app.global_ui.viewport.fullscreen_latched = true;
    app.global_ui.viewport.startup_welcome = false;
    app
}

async fn make_mock_app() -> (App, mpsc::Receiver<RenderEvent>) {
    let mut app = make_fullscreen_app().await;
    make_worker_barrier(&app, 80).await;
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::user("测试正文".into())];
    let (sender, receiver) = mpsc::channel(8);
    app.session_mgr.current_mut().messages.render_tx = sender;
    (app, receiver)
}

fn make_width_request(
    receiver: &mut mpsc::Receiver<RenderEvent>,
) -> (WidthPreparationRequest, oneshot::Sender<PreparedWidth>) {
    match receiver.try_recv().expect("高度跨越边界后应投递宽度准备") {
        RenderEvent::PrepareWidth { request, reply } => (request, reply),
        _ => panic!("高度跨越边界只能请求宽度准备"),
    }
}

fn make_publish_rows(app: &App, width: u16, rows: usize) {
    let mut cache = app.session_mgr.current().messages.render_cache.write();
    cache.width = width;
    cache.total_lines = rows;
    cache.version = cache.version.wrapping_add(1);
}

fn make_complete_width(
    app: &App,
    reply: oneshot::Sender<PreparedWidth>,
    width: u16,
    full_width_rows: usize,
    measured_rows: usize,
) {
    make_publish_rows(app, width, measured_rows);
    reply
        .send(PreparedWidth {
            width,
            full_width_rows,
            measured_rows,
        })
        .expect("当前布局的宽度回复仍应有效");
}

fn make_hidden_group(index: usize) -> MessageViewModel {
    let mut message = MessageViewModel::ToolCallGroup {
        thinking: None,
        category: ToolCategory::Read,
        tools: vec![ToolEntry {
            tool_name: "Read".into(),
            display_name: "Read".into(),
            args_display: Some(format!("history_{index}.txt")),
            content: "success".into(),
            is_error: false,
            file_inspect: false,
        }],
        collapsed: true,
        content_hash: 0,
        standalone_action: None,
    };
    message.recompute_hash();
    message
}

async fn make_wait_for_width(app: &mut App, width: u16) {
    tokio::time::timeout(Duration::from_secs(5), prepare_width(app, width))
        .await
        .expect("宽度准备必须收敛，不能在全宽和窄宽之间反复切换");
}

#[tokio::test]
async fn test_width_recovery_rechecks_narrow_measurement_only_after_content_fits() {
    let (mut app, mut receiver) = make_mock_app().await;
    assert!(!try_prepare_width(&mut app, 80, true));
    let (request, reply) = make_width_request(&mut receiver);
    assert_eq!(request.extra_rows, 0, "夹具只计算消息正文高度");
    let overflow_rows = request.available_rows + 10;
    let fitting_rows = request.available_rows / 2;
    make_complete_width(&app, reply, 79, overflow_rows, overflow_rows);
    assert!(try_prepare_width(&mut app, 80, true));
    let revision = app.session_mgr.current().messages.layout_revision();
    make_publish_rows(&app, 79, overflow_rows + 4);
    assert!(
        try_prepare_width(&mut app, 80, true),
        "窄宽实测变化但仍溢出时不能重新排全宽"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    make_publish_rows(&app, 79, fitting_rows);
    assert!(
        !try_prepare_width(&mut app, 80, true),
        "窄宽从溢出变可容纳后必须重测全宽"
    );
    let (recovery, reply) = make_width_request(&mut receiver);
    assert_eq!(recovery, request, "只实测高度变化也必须重测同一布局");
    assert_eq!(
        app.session_mgr.current().messages.layout_revision(),
        revision,
        "视口实测不能伪装成消息布局变化"
    );
    make_complete_width(&app, reply, 80, fitting_rows, fitting_rows);
    assert!(try_prepare_width(&mut app, 80, true));
    make_publish_rows(&app, 80, fitting_rows);
    assert!(
        try_prepare_width(&mut app, 80, true),
        "恢复后相同高度的视口发布不能重复入队"
    );
    assert_eq!(
        app.session_mgr.current().messages.last_resize_width,
        Some(80)
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn test_width_recovery_uses_current_full_width_height_before_overflow_preparation() {
    let (mut app, mut receiver) = make_mock_app().await;
    assert!(!try_prepare_width(&mut app, 80, true));
    let (request, reply) = make_width_request(&mut receiver);
    assert_eq!(request.extra_rows, 0, "夹具只计算消息正文高度");
    let fitting_rows = request.available_rows / 2;
    make_complete_width(&app, reply, 80, fitting_rows, fitting_rows);
    assert!(try_prepare_width(&mut app, 80, true));
    make_publish_rows(&app, 80, fitting_rows + 1);
    assert_eq!(
        message_rows(&app, 80),
        fitting_rows + 1,
        "同宽视口实测高度应覆盖旧的全宽测量快照"
    );
    assert!(try_prepare_width(&mut app, 80, true));
    let overflow_rows = request.available_rows + 1;
    make_publish_rows(&app, 80, overflow_rows);
    assert_eq!(message_rows(&app, 80), overflow_rows);
    assert!(
        !try_prepare_width(&mut app, 80, true),
        "全宽实测从可容纳变溢出后必须准备滚动条窄宽"
    );
    let (overflow, reply) = make_width_request(&mut receiver);
    assert_eq!(overflow, request);
    make_complete_width(&app, reply, 79, overflow_rows, overflow_rows);
    assert!(try_prepare_width(&mut app, 80, true));
    make_publish_rows(&app, 79, overflow_rows);
    assert!(
        try_prepare_width(&mut app, 80, true),
        "窄宽确认后同一实测值不能触发反复重排"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn test_width_recovery_real_virtual_worker_remeasures_full_window_and_restores_anchor() {
    let mut app = make_fullscreen_app().await;
    let hidden_count = 32;
    let mut messages: Vec<_> = (0..hidden_count).map(make_hidden_group).collect();
    messages.push(MessageViewModel::user("真实可见尾条".into()));
    let tail_anchor = Some(ViewportAnchor {
        message_index: hidden_count,
        logical_line: 0,
        row_in_line: 0,
    });
    let requested_top = 4096;
    let sender = app.session_mgr.current().messages.render_tx.clone();
    app.session_mgr.current_mut().messages.view_messages = messages.clone();
    app.session_mgr
        .current()
        .messages
        .send_render_event(RenderEvent::SetDetailStrategy("virtual".into()))
        .expect("真实虚拟模式应入队");
    sender
        .send(RenderEvent::Viewport {
            scroll_revision: 0,
            top: requested_top,
            height: 1,
            follow_tail: false,
            anchor: tail_anchor,
        })
        .await
        .expect("初始尾部锚点应入队");
    app.session_mgr
        .current()
        .messages
        .send_render_event(RenderEvent::Rebuild(messages))
        .expect("真实消息应入队");
    make_worker_barrier(&app, 80).await;
    make_wait_for_width(&mut app, 80).await;
    let request = app
        .session_mgr
        .current()
        .messages
        .width_preparation
        .context
        .as_ref()
        .expect("真实布局应记录准备条件")
        .request;
    let revision = app.session_mgr.current().messages.layout_revision();
    {
        let cache = app.session_mgr.current().messages.render_cache.read();
        let layout = cache.virtual_layout.as_ref().expect("必须使用真实虚拟渲染");
        assert_eq!(cache.width, 79, "未物化历史的估计值应先产生窄宽布局");
        assert!(cache.total_lines > request.available_rows);
        assert!(
            layout.diagnostics.estimated_segments >= hidden_count,
            "尾部小视口必须留下真实的未物化历史估计"
        );
    }
    sender
        .send(RenderEvent::Viewport {
            scroll_revision: 0,
            top: 0,
            height: request.available_rows,
            follow_tail: false,
            anchor: None,
        })
        .await
        .expect("头部实测视口应入队");
    make_worker_barrier(&app, 79).await;
    let fitting_rows = {
        let cache = app.session_mgr.current().messages.render_cache.read();
        let layout = cache.virtual_layout.as_ref().expect("实测后仍为虚拟模式");
        assert_eq!(
            layout.diagnostics.estimated_segments, 0,
            "夹具必须真实测完隐藏历史"
        );
        assert!(cache.total_lines > 0, "可见尾条不能丢失");
        assert!(cache.total_lines <= request.available_rows);
        assert!(cache
            .lines
            .iter()
            .any(|line| line.to_string().contains("真实可见尾条")));
        cache.total_lines
    };
    sender
        .send(RenderEvent::Viewport {
            scroll_revision: 0,
            top: requested_top,
            height: 1,
            follow_tail: false,
            anchor: tail_anchor,
        })
        .await
        .expect("恢复原尾部锚点应入队");
    make_worker_barrier(&app, 79).await;
    assert!(
        !try_prepare_width(&mut app, 80, true),
        "真实窄宽高度修正后必须重新准备全宽"
    );
    make_wait_for_width(&mut app, 80).await;
    {
        let cache = app.session_mgr.current().messages.render_cache.read();
        let layout = cache
            .virtual_layout
            .as_ref()
            .expect("恢复全宽后仍为虚拟模式");
        assert_eq!(cache.width, 80, "全宽重测不能退回旧的局部估计窄宽");
        assert_eq!(
            cache.total_lines, fitting_rows,
            "全宽必须保留完整窗口的实测高度"
        );
        assert_eq!(layout.diagnostics.estimated_segments, 0);
        assert_eq!(
            layout.requested_top, requested_top,
            "临时头部测量不能覆盖阅读位置"
        );
        assert_eq!(layout.requested_height, 1, "发布前必须恢复原视口高度");
        assert!(cache
            .lines
            .iter()
            .any(|line| line.to_string().contains("真实可见尾条")));
    }
    assert_eq!(
        app.session_mgr.current().messages.layout_revision(),
        revision,
        "整个恢复过程只能由视口物化推动，不能重建消息"
    );
    for offset in 1..=3 {
        sender
            .send(RenderEvent::Viewport {
                scroll_revision: 0,
                top: requested_top + offset,
                height: 1,
                follow_tail: false,
                anchor: tail_anchor,
            })
            .await
            .expect("恢复后的稳定视口应入队");
        make_worker_barrier(&app, 80).await;
        assert!(
            try_prepare_width(&mut app, 80, true),
            "稳定视口不能重新等待布局屏障"
        );
    }
    assert!(
        app.session_mgr
            .current()
            .messages
            .width_preparation
            .pending
            .is_none(),
        "恢复后不能留下反复准备的请求"
    );
}
