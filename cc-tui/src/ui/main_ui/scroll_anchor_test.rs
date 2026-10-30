use crate::{
    app::{message_pipeline::PipelineAction, App},
    ui::{
        headless::HeadlessHandle,
        message_view::MessageViewModel,
        render_thread::{RenderEvent, ScrollAnchor, ViewportAnchor},
    },
};
use ratatui::layout::Rect;
use tokio::sync::{mpsc, oneshot};

async fn make_render_barrier(sender: &mpsc::Sender<RenderEvent>) {
    let (reply, ready) = oneshot::channel();
    sender
        .send(RenderEvent::ResizeAndWait(79, reply))
        .await
        .expect("渲染屏障应入队");
    tokio::time::timeout(std::time::Duration::from_secs(5), ready)
        .await
        .expect("渲染必须及时完成")
        .expect("渲染屏障不能丢失回复");
}

fn make_draw(app: &mut App, handle: &mut HeadlessHandle) {
    handle
        .terminal
        .draw(|frame| {
            super::message_area::render_messages(
                frame,
                app,
                Rect::default(),
                Rect::new(0, 0, 80, 30),
            );
        })
        .expect("消息区应绘制成功");
}

async fn make_history() -> (App, HeadlessHandle) {
    let (mut app, mut handle) = App::new_headless(80, 30).await;
    app.global_ui.viewport.startup_welcome = false;
    app.global_ui.viewport.fullscreen_latched = true;
    let messages = vec![
        MessageViewModel::user("开始任务".into()),
        MessageViewModel::user(
            (0..220)
                .map(|index| format!("历史内容第 {index} 行\n"))
                .collect(),
        ),
        MessageViewModel::system("等待后台任务完成".into()),
    ];
    let session = app.session_mgr.current_mut();
    session.messages.view_messages = messages;
    session.messages.round_start_vm_idx = 3;
    app.render_rebuild();
    make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
    make_draw(&mut app, &mut handle);
    app.session_mgr.current_mut().ui.scroll_offset = 100;
    app.session_mgr.current_mut().ui.scroll_follow = false;
    (app, handle)
}

#[tokio::test]
async fn test_scroll_anchor_rebuild_preserves_position_inside_long_history() {
    let (mut app, mut handle) = make_history().await;
    let original = app.session_mgr.current().ui.scroll_offset;
    app.apply_pipeline_action(PipelineAction::RebuildAll {
        prefix_len: 3,
        tail_vms: Vec::new(),
    });
    make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        original,
        "后台重建应保留长消息内部的阅读位置，不能选中下一条尾部消息"
    );
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "阅读历史不能恢复跟底"
    );
}

#[tokio::test]
async fn test_scroll_anchor_queue_retry_cannot_override_later_manual_scroll() {
    let (mut app, mut handle) = make_history().await;
    let worker = app.session_mgr.current().messages.render_tx.clone();
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(
        sender.try_send(RenderEvent::Rebuild(Vec::new())).is_ok(),
        "填满队列"
    );
    app.session_mgr.current_mut().messages.render_tx = sender;
    let anchor = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(100)
        .expect("阅读位置应有精确锚点");
    app.render_rebuild_with_anchor(anchor);
    app.scroll_up();
    let requested = app.session_mgr.current().ui.scroll_offset;
    receiver.try_recv().expect("释放队列容量");
    app.poll_agent();
    worker
        .send(receiver.try_recv().expect("最新内容快照仍须重试"))
        .await
        .expect("重试快照应送达真实渲染线程");
    make_render_barrier(&worker).await;
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        requested,
        "旧锚点返回不能覆盖用户随后上滚的位置"
    );
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "旧锚点不能恢复跟底"
    );
}

#[tokio::test]
async fn test_scroll_anchor_passive_shrink_and_growth_preserve_manual_follow_choice() {
    let (mut app, mut handle) = make_history().await;
    let (sender, _receiver) = mpsc::channel(8);
    app.session_mgr.current_mut().messages.render_tx = sender;
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.scroll_anchor = None;
        cache.total_lines = 90;
    }
    make_draw(&mut app, &mut handle);
    let clamped = app.session_mgr.current().ui.scroll_offset;
    assert_eq!(clamped, 60, "布局缩短时只钳位到新的可滚动范围");
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "被动缩短不能恢复跟底"
    );
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .total_lines = 230;
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        clamped,
        "内容重新增长不能追尾"
    );
    app.scroll_to_bottom();
    make_draw(&mut app, &mut handle);
    assert!(
        app.session_mgr.current().ui.scroll_follow,
        "用户主动回到底部仍须恢复跟随"
    );
}

#[tokio::test]
async fn test_scroll_anchor_manual_round_trip_rejects_old_revision_at_same_offset() {
    let (mut app, mut handle) = make_history().await;
    let revision = app.session_mgr.current().ui.scroll_revision;
    let position = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(100)
        .expect("阅读位置应可定位");
    app.scroll_up();
    app.scroll_down();
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        100,
        "往返滚动回到同一坐标"
    );
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .scroll_anchor = Some(ScrollAnchor {
        position,
        visual_row: 220,
        scroll_revision: revision,
    });
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        100,
        "相同坐标不能让旧用户意图重新有效"
    );
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "旧版本不能开启跟随"
    );
}

#[tokio::test]
async fn test_scroll_anchor_background_continuation_preserves_follow_choice() {
    for follow in [false, true] {
        let (mut app, mut handle) = make_history().await;
        if follow {
            app.scroll_to_bottom();
            make_draw(&mut app, &mut handle);
        }
        let original = (
            app.session_mgr.current().ui.scroll_offset,
            app.session_mgr.current().ui.scroll_revision,
        );
        app.submit_bg_continuation(vec![cc_agent::agent::events::BackgroundTaskResult {
            task_id: "child".into(),
            agent_name: "general-purpose".into(),
            prompt_summary: "后台任务".into(),
            success: true,
            output: "完成结果".into(),
            tool_calls_count: 1,
            duration_ms: 100,
            child_thread_id: None,
        }]);
        assert_eq!(
            app.session_mgr.current().ui.scroll_follow,
            follow,
            "自动续接应保持跟随选择"
        );
        assert_eq!(
            app.session_mgr.current().ui.scroll_offset,
            original.0,
            "自动续接不能主动改写阅读位置"
        );
        assert_eq!(
            app.session_mgr.current().ui.scroll_revision,
            original.1,
            "自动续接不属于用户定位"
        );
        make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
        make_draw(&mut app, &mut handle);
        if !follow {
            assert_eq!(
                app.session_mgr.current().ui.scroll_offset,
                original.0,
                "续接结果绘制后仍保留历史位置"
            );
        }
    }
}

#[tokio::test]
async fn test_scroll_anchor_consecutive_rebuilds_keep_unconsumed_semantic_position() {
    let (mut app, mut handle) = make_history().await;
    let original = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(100)
        .expect("应命中长历史");
    for count in [11, 21] {
        let mut messages = app.session_mgr.current().messages.view_messages.clone();
        messages[0] = MessageViewModel::user("前置行\n".repeat(count));
        app.apply_pipeline_action(PipelineAction::RebuildAll {
            prefix_len: 0,
            tail_vms: messages,
        });
        make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
        let published = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .scroll_anchor
            .expect("重排应发布精确锚点");
        assert_eq!(
            published.position, original,
            "未绘制期间连续重建仍须保持同一个逻辑位置"
        );
        assert_eq!(
            app.session_mgr.current().ui.scroll_offset,
            100,
            "此时尚未绘制消费锚点"
        );
    }
    let expected = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .scroll_anchor
        .expect("应有待消费位置")
        .visual_row;
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        expected,
        "最后一帧恢复原内容位置"
    );
    assert!(!app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_scroll_anchor_virtual_reflow_with_welcome_uses_message_coordinates() {
    let (mut app, mut handle) = make_history().await;
    app.global_ui.viewport.startup_welcome = true;
    let prefix = super::viewport_layout::welcome_rows(&app, 79);
    assert!(prefix > 0, "回归必须实际包含欢迎前缀");
    let top = 100 - prefix;
    let anchor = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(top)
        .expect("消息空间应有精确位置");
    let sender = app.session_mgr.current().messages.render_tx.clone();
    sender
        .send(RenderEvent::SetDetailStrategy("virtual".into()))
        .await
        .expect("切虚拟布局");
    sender
        .send(RenderEvent::Viewport {
            top,
            height: 30,
            follow_tail: false,
            anchor: Some(anchor),
            scroll_revision: app.session_mgr.current().ui.scroll_revision,
        })
        .await
        .expect("同步消息空间视口");
    make_render_barrier(&sender).await;
    let mut messages = app.session_mgr.current().messages.view_messages.clone();
    messages[0] = MessageViewModel::user("新增前置行\n".repeat(11));
    app.apply_pipeline_action(PipelineAction::RebuildAll {
        prefix_len: 0,
        tail_vms: messages,
    });
    make_render_barrier(&sender).await;
    let published = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .scroll_anchor
        .expect("虚拟布局应发布位置");
    assert!(published.visual_row > top, "前置内容增长必须改变视觉偏移");
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        published.visual_row + prefix,
        "消息空间锚点应加回欢迎前缀"
    );
    assert!(
        !app.session_mgr.current().ui.scroll_follow,
        "合法重排不能恢复跟随"
    );
}

#[tokio::test]
async fn test_scroll_anchor_virtual_home_before_growth_does_not_inherit_follow_tail() {
    let (mut app, mut handle) = make_history().await;
    let sender = app.session_mgr.current().messages.render_tx.clone();
    sender
        .send(RenderEvent::SetDetailStrategy("virtual".into()))
        .await
        .expect("切虚拟布局");
    sender
        .send(RenderEvent::Viewport {
            top: 0,
            height: 30,
            follow_tail: true,
            anchor: None,
            scroll_revision: 0,
        })
        .await
        .expect("旧布局仍处于跟随模式");
    make_render_barrier(&sender).await;
    app.scroll_to_top();
    let revision = app.session_mgr.current().ui.scroll_revision;
    // Home 的下一帧尚未上报 Viewport，先到来的重建仍必须遵守新的阅读意图。
    app.render_rebuild_with_anchor(ViewportAnchor::default());
    make_render_barrier(&sender).await;
    let published = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .scroll_anchor
        .expect("应发布阅读锚点");
    assert_eq!(published.scroll_revision, revision);
    assert_eq!(published.visual_row, 0, "显式阅读锚点不能继承旧的跟底状态");
    make_draw(&mut app, &mut handle);
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        0,
        "Home 后的重建不能抢回尾部"
    );
    assert!(!app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_scroll_anchor_classic_resize_and_modes_preserve_consumed_reading_anchor() {
    let (mut app, mut handle) = make_history().await;
    let sender = app.session_mgr.current().messages.render_tx.clone();
    app.session_mgr.current_mut().messages.view_messages[1] = MessageViewModel::system(
        (0..220)
            .map(|index| format!("第{index}行中文内容\n"))
            .collect(),
    );
    app.render_rebuild();
    make_render_barrier(&sender).await;
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(101, false);
    make_draw(&mut app, &mut handle);
    make_render_barrier(&sender).await;
    let original = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(101)
        .expect("应有折行内锚点");
    assert!(
        app.session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .scroll_anchor
            .is_none(),
        "正常绘制已消费旧锚点"
    );
    let (reply, ready) = oneshot::channel();
    sender
        .send(RenderEvent::ResizeAndWait(9, reply))
        .await
        .expect("窄宽重排");
    ready.await.expect("窄宽重排完成");
    let pending = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .scroll_anchor
        .expect("已消费后仍应从阅读视口恢复语义位置");
    assert_eq!(pending.position, original);
    assert_ne!(pending.visual_row, 101, "长CJK行在窄宽下应有不同视觉偏移");
    sender
        .send(RenderEvent::ToggleDetail(true))
        .await
        .expect("详情重排");
    sender
        .send(RenderEvent::ToggleDiff(false))
        .await
        .expect("差异重排");
    sender
        .send(RenderEvent::ToggleHookRows(false))
        .await
        .expect("hook重排");
    let (reply, ready) = oneshot::channel();
    sender
        .send(RenderEvent::ResizeAndWait(9, reply))
        .await
        .expect("模式屏障");
    ready.await.expect("模式重排完成");
    let cache = app.session_mgr.current().messages.render_cache.read();
    let pending = cache.scroll_anchor.expect("模式变更不能丢失位置");
    assert_eq!(pending.position, original);
    assert_eq!(
        Some(pending.visual_row),
        cache.visual_row_for_anchor(original),
        "位置必须按最终宽度重新解析"
    );
}

#[tokio::test]
async fn test_scroll_anchor_strategy_round_trip_and_classic_positions_are_consistent() {
    let (mut app, mut handle) = make_history().await;
    let sender = app.session_mgr.current().messages.render_tx.clone();
    make_draw(&mut app, &mut handle);
    make_render_barrier(&sender).await;
    let original = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(100)
        .expect("应有阅读位置");
    for strategy in ["cache", "virtual", "cache"] {
        sender
            .send(RenderEvent::SetDetailStrategy(strategy.into()))
            .await
            .expect("切换渲染策略");
        make_render_barrier(&sender).await;
        make_draw(&mut app, &mut handle);
        make_render_barrier(&sender).await;
        assert!(!app.session_mgr.current().ui.scroll_follow);
        let cache = app.session_mgr.current().messages.render_cache.read();
        let offset = app.session_mgr.current().ui.scroll_offset;
        assert_eq!(
            cache.viewport_anchor_at(offset),
            Some(original),
            "经典与虚拟锚点使用相同原始行坐标"
        );
        if strategy == "cache" {
            assert_eq!(
                cache.line_origins.len(),
                cache.lines.len(),
                "空行去重/尾行裁剪后来源映射必须对齐"
            );
            for row in 0..cache.total_lines {
                let anchor = cache.viewport_anchor_at(row).expect("每个可见行都应可定位");
                assert_eq!(
                    cache.visual_row_for_anchor(anchor),
                    Some(row),
                    "所有经典视觉行定位必须可逆：{row}"
                );
            }
        }
    }
}

#[tokio::test]
async fn test_scroll_anchor_scroll_before_mode_reflow_preserves_latest_user_position() {
    for strategy in ["cache", "virtual"] {
        let (mut app, mut handle) = make_history().await;
        let sender = app.session_mgr.current().messages.render_tx.clone();
        app.session_mgr.current_mut().messages.view_messages[0] =
            MessageViewModel::system_with_detail("任务概览".into(), "展开的明细\n".repeat(20));
        app.session_mgr
            .current()
            .send_render_event(RenderEvent::SetDetailStrategy(strategy.into()))
            .expect("模式请求应入队");
        app.render_rebuild();
        make_render_barrier(&sender).await;
        app.session_mgr
            .current_mut()
            .ui
            .set_scroll_position(100, false);
        make_draw(&mut app, &mut handle);
        make_render_barrier(&sender).await;
        app.scroll_up();
        let revision = app.session_mgr.current().ui.scroll_revision;
        let top = app.session_mgr.current().ui.scroll_offset;
        let original = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .viewport_anchor_at(top)
            .expect("新请求应有精确位置");
        // 用户上滚后尚未绘制，模式命令必须自带新位置，不能等下一帧上报。
        app.toggle_detail_mode();
        make_render_barrier(&sender).await;
        let published = app
            .session_mgr
            .current()
            .messages
            .render_cache
            .read()
            .scroll_anchor
            .expect("重排应带回位置");
        assert_eq!(
            published.scroll_revision, revision,
            "不能采用上一帧版本：{strategy}"
        );
        assert_eq!(published.position, original, "必须保留新滚动的内容位置");
        assert!(published.visual_row > top, "详情增长应实际改变布局");
        make_draw(&mut app, &mut handle);
        assert_eq!(
            app.session_mgr.current().ui.scroll_offset,
            published.visual_row
        );
        assert!(!app.session_mgr.current().ui.scroll_follow);
    }
}

#[tokio::test]
async fn test_scroll_anchor_scroll_before_width_preparation_preserves_latest_user_position() {
    let (mut app, mut handle) = make_history().await;
    let sender = app.session_mgr.current().messages.render_tx.clone();
    app.session_mgr.current_mut().messages.view_messages[1] = MessageViewModel::system(
        (0..220)
            .map(|index| format!("第{index}行中文内容\n"))
            .collect(),
    );
    app.render_rebuild();
    make_render_barrier(&sender).await;
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(101, false);
    make_draw(&mut app, &mut handle);
    make_render_barrier(&sender).await;
    app.scroll_up();
    let top = app.session_mgr.current().ui.scroll_offset;
    let revision = app.session_mgr.current().ui.scroll_revision;
    let original = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(top)
        .expect("新请求应有精确位置");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !super::viewport_layout::try_prepare_width(&mut app, 10, false) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("宽度准备不能阻塞");
    let published = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .scroll_anchor
        .expect("宽度准备应带回位置");
    assert_eq!(published.scroll_revision, revision);
    assert_eq!(published.position, original);
    assert_ne!(published.visual_row, top, "回归必须实际发生折行重排");
    handle
        .terminal
        .draw(|frame| {
            super::message_area::render_messages(
                frame,
                &mut app,
                Rect::default(),
                Rect::new(0, 0, 11, 30),
            )
        })
        .expect("新宽度应能绘制");
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        published.visual_row
    );
    assert!(!app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_scroll_anchor_view_switch_cannot_relabel_old_child_cache_with_parent_revision() {
    let (mut app, mut handle) = make_history().await;
    app.global_ui.viewport.startup_welcome = true;
    let worker = app.session_mgr.current().messages.render_tx.clone();
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started", "instanceId": "child", "agentType": "general-purpose",
        "taskPreview": "后台任务", "isBackground": true,
    }));
    app.handle_subagent_event(
        "child",
        crate::app::AgentEvent::AssistantChunk {
            chunk: "子任务历史内容\n".repeat(230),
            source_agent_id: None,
        },
    );
    app.switch_subagent_viewport(Some("child".into()));
    make_render_barrier(&worker).await;
    make_draw(&mut app, &mut handle);
    let child_position = app
        .session_mgr
        .current()
        .messages
        .render_cache
        .read()
        .viewport_anchor_at(100)
        .expect("旧子缓存必须有可误用的位置");
    assert_eq!(app.session_mgr.current().ui.message_prefix_rows, 0);
    let (sender, mut receiver) = mpsc::channel(16);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.switch_subagent_viewport(None);
    let revision = app.session_mgr.current().ui.scroll_revision;
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 100);
    assert!(app.session_mgr.current().messages.last_viewport.is_none());
    match app
        .session_mgr
        .current()
        .with_render_viewport(RenderEvent::ToggleDetail(true))
    {
        RenderEvent::WithViewport { viewport, .. } => {
            assert_eq!(viewport.scroll_revision, revision);
            assert!(
                viewport.anchor.is_none(),
                "不能把旧子缓存位置标为主视口的新版本"
            );
        }
        _ => panic!("重排仍应携带恢复的阅读意图"),
    }
    let len = app.session_mgr.current().messages.view_messages.len();
    app.apply_pipeline_action(PipelineAction::RebuildAll {
        prefix_len: len,
        tail_vms: Vec::new(),
    });
    while let Ok(event) = receiver.try_recv() {
        assert!(
            matches!(event.into_payload(), RenderEvent::Rebuild(_)),
            "首帧前不能从旧视图采重建锚点"
        );
    }
    app.session_mgr
        .current()
        .messages
        .render_cache
        .write()
        .scroll_anchor = Some(ScrollAnchor {
        position: child_position,
        visual_row: 150,
        scroll_revision: revision,
    });
    make_draw(&mut app, &mut handle);
    assert!(
        app.session_mgr.current().ui.message_prefix_rows > 0,
        "主视图恢复自己的欢迎前缀"
    );
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        100,
        "首帧必须保留恢复的全局位置"
    );
    assert!(!app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_scroll_anchor_compact_view_switch_restores_parent_and_invalidates_child() {
    let (mut app, _handle) = make_history().await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started", "instanceId": "child", "agentType": "general-purpose",
        "taskPreview": "后台任务", "isBackground": true,
    }));
    app.switch_subagent_viewport(Some("child".into()));
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(7, false);
    let revision = app.session_mgr.current().ui.scroll_revision;
    app.handle_compact_started(false);
    let session = app.session_mgr.current();
    assert!(session.focused_instance_id.is_none(), "压缩回到主视口");
    assert_eq!(session.ui.scroll_offset, 100, "回到主视口应恢复主阅读位置");
    assert!(!session.ui.scroll_follow, "不能继承子视口跟随选择");
    assert!(
        session.ui.scroll_revision > revision,
        "压缩切视口也须作废子视口的在途锚点"
    );
    assert!(
        session.messages.last_viewport.is_none(),
        "旧子视口上报不能被复用"
    );
}

#[tokio::test]
async fn test_scroll_anchor_shell_completion_preserves_history_but_manual_submit_follows_tail() {
    let (mut app, _handle) = make_history().await;
    let notice = "<background-task-completed>\n<task-id>shell</task-id>\n<command>cargo test</command>\n<status>completed (exit 0)</status>\n<output>test.output</output>\n</background-task-completed>";
    app.submit_message(notice.into());
    assert!(
        app.session_mgr
            .current()
            .messages
            .last_submitted_text
            .is_none(),
        "此回归必须走自动通知分支"
    );
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        100,
        "后台shell通知不能抢走阅读位置"
    );
    assert!(!app.session_mgr.current().ui.scroll_follow);
    app.submit_message("继续处理".into());
    assert_eq!(
        app.session_mgr.current().ui.scroll_offset,
        usize::MAX,
        "用户主动发送仍要查看最新内容"
    );
    assert!(app.session_mgr.current().ui.scroll_follow);
}

#[tokio::test]
async fn test_scroll_anchor_foreground_shell_completion_preserves_current_follow_choice() {
    for follow in [false, true] {
        let (mut app, mut handle) = make_history().await;
        let now = chrono::Utc::now();
        let record = crate::shell_history::ShellCommandRecord {
            id: "scroll-anchor-shell".into(),
            thread_id: "scroll-anchor-thread".into(),
            command: "cargo test".into(),
            cwd: ".".into(),
            stdin: Vec::new(),
            stdout: "完成输出\n".repeat(40),
            stderr: String::new(),
            exit_code: 0,
            started_at: now,
            completed_at: now,
            anchor_message_id: None,
        };
        let session = app.session_mgr.current_mut();
        session.current_thread_id = Some(record.thread_id.clone());
        session.shell_pool.foreground.runtime.running_record_id = Some(record.id.clone());
        session.ui.set_scroll_position(100, follow);
        app.set_loading(true);
        make_draw(&mut app, &mut handle);
        make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
        let offset = app.session_mgr.current().ui.scroll_offset;
        let revision = app.session_mgr.current().ui.scroll_revision;
        app.handle_shell_command_completed(record);
        assert_eq!(
            app.session_mgr.current().ui.scroll_offset,
            offset,
            "自动完成不能覆盖当前阅读位置"
        );
        assert_eq!(app.session_mgr.current().ui.scroll_follow, follow);
        assert_eq!(
            app.session_mgr.current().ui.scroll_revision,
            revision,
            "完成回调不能冒充用户滚动"
        );
        assert!(
            !app.session_mgr.current().ui.loading,
            "仍应结束前台运行状态"
        );
        make_render_barrier(&app.session_mgr.current().messages.render_tx).await;
        make_draw(&mut app, &mut handle);
        let ui = &app.session_mgr.current().ui;
        assert_eq!(ui.scroll_follow, follow);
        if follow {
            assert_eq!(
                ui.scroll_offset, ui.scrollbar_max_offset,
                "原本跟底仍应跟底"
            );
        } else {
            assert_eq!(ui.scroll_offset, offset, "重绘也不能抢走历史阅读位置");
        }
    }
}
