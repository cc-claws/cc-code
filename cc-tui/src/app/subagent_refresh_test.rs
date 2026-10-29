use super::*;
use crate::{acp_client::AcpNotification, app::App, ui::render_thread::RenderEvent};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
};
use serde_json::json;
use tokio::sync::mpsc;

fn make_started(app: &mut App) {
    app.handle_subagent_lifecycle(&json!({
        "event": "started", "instanceId": "child", "agentType": "general-purpose",
        "taskPreview": "中文长任务", "isBackground": true,
    }));
}

fn make_chunk(app: &mut App, text: &str) {
    app.handle_subagent_event(
        "child",
        AgentEvent::AssistantChunk {
            chunk: text.into(),
            source_agent_id: None,
        },
    );
}

fn make_text(messages: &[MessageViewModel]) -> String {
    use crate::ui::message_view::ContentBlockView;
    messages
        .iter()
        .filter_map(|message| match message {
            MessageViewModel::AssistantBubble { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flatten()
        .filter_map(|block| match block {
            ContentBlockView::Text { raw, .. } => Some(raw.as_str()),
            _ => None,
        })
        .collect()
}

fn make_refresh_later(app: &mut App) {
    app.session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("child")
        .expect("子会话存在")
        .last_view_refresh_at = Some(Instant::now() + Duration::from_secs(60));
}

fn make_render_messages(event: RenderEvent) -> Vec<MessageViewModel> {
    match event {
        RenderEvent::Rebuild(messages) | RenderEvent::RebuildWithAnchor { messages, .. } => {
            messages
        }
        _ => panic!("应收到完整重建快照"),
    }
}

#[tokio::test]
async fn test_subagent_stream_refresh_coalesces_and_flushes_without_new_events() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.switch_subagent_viewport(Some("child".into()));
    let parent_before = app.session_mgr.current().messages.view_messages.clone();
    let expected: String = (0..1000).map(|index| format!("中文{index}\n")).collect();
    for line in expected.split_inclusive('\n') {
        make_chunk(&mut app, line);
    }
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        0,
        "流式接收不能逐分片构建视图"
    );
    assert!(app.flush_subagent_view_refresh());
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        1
    );
    assert_eq!(
        make_text(app.session_mgr.current().current_view_messages()),
        expected
    );
    assert_eq!(
        app.session_mgr.current().messages.view_messages,
        parent_before,
        "子文本刷新不能重建主时间线"
    );
    make_refresh_later(&mut app);
    make_chunk(&mut app, "末尾分片");
    assert!(!app.flush_subagent_view_refresh(), "刷新窗口内只累积状态");
    app.session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("child")
        .expect("子会话存在")
        .last_view_refresh_at = Some(Instant::now() - SUBAGENT_VIEW_REFRESH_INTERVAL);
    assert!(
        app.poll_agent(),
        "无新通知时也必须刷新节流窗口内留下的末尾分片"
    );
    assert_eq!(
        make_text(app.session_mgr.current().current_view_messages()),
        expected + "末尾分片"
    );
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        2
    );
}

#[tokio::test]
async fn test_hidden_subagent_defers_history_conversion_until_viewport_switch() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    let history: Vec<_> = (0..300)
        .map(|index| BaseMessage::ai(format!("历史{index}\n")))
        .collect();
    let expected_history: String = (0..300).map(|index| format!("历史{index}\n")).collect();
    app.handle_subagent_event("child", AgentEvent::StateSnapshot(history));
    for _ in 0..1000 {
        make_chunk(&mut app, "流式中文\n");
    }
    app.poll_agent();
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(sub.view_rebuild_count, 0, "隐藏子会话不做历史转换");
    assert!(sub.view_messages.is_empty());
    app.switch_subagent_viewport(Some("child".into()));
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        1
    );
    assert_eq!(
        make_text(app.session_mgr.current().current_view_messages()),
        expected_history + &"流式中文\n".repeat(1000)
    );
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert!(sub.pipeline.completed_view_cache_stats().1 > 0);
    let visible = sub.view_messages.clone();
    app.switch_subagent_viewport(None);
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(
        sub.pipeline.completed_view_cache_stats(),
        (0, 0),
        "切离活跃子任务应释放新增缓存"
    );
    assert_eq!(sub.view_messages, visible, "释放投影缓存不能丢失子视图内容");
}

#[tokio::test]
async fn test_subagent_periodic_refresh_reuses_large_edit_history_without_reopening_files() {
    use cc_agent::messages::ToolCallRequest;
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let directory = tempfile::tempdir().expect("创建历史文件目录");
    let mut history = Vec::new();
    let mut paths = Vec::new();
    for index in 0..84 {
        let path = directory.path().join(format!("中文文件{index}.rs"));
        std::fs::write(&path, "前文\n新值\n后文\n").expect("创建 Edit 目标");
        let id = format!("edit-{index}");
        history.push(BaseMessage::ai_with_tool_calls("", vec![ToolCallRequest::new(
            &id, "Edit", json!({"file_path": path.to_string_lossy(), "old_string": "旧值", "new_string": "新值"}),
        )]));
        history.push(BaseMessage::tool_result(id, "修改成功"));
        paths.push(path);
    }
    history.extend((0..133).map(|index| BaseMessage::ai(format!("检查记录{index}\n"))));
    assert_eq!(history.len(), 301, "对齐长任务的历史规模");
    make_started(&mut app);
    app.handle_subagent_event("child", AgentEvent::StateSnapshot(history));
    // 只验证 UI 线程的规范状态和视图构造，渲染队列另有端到端顺序测试。
    let (sender, _receiver) = mpsc::channel(1);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.switch_subagent_viewport(Some("child".into()));
    assert!(
        app.session_mgr.current().subagent_sessions["child"]
            .pipeline
            .completed_view_cache_stats()
            .1
            > 0,
        "现场规模应落入缓存预算，不能只验证退化分支"
    );
    for path in paths {
        std::fs::remove_file(path).expect("删除目标以检测周期刷新是否重读历史文件");
    }
    for _ in 0..20 {
        for _ in 0..20 {
            make_chunk(&mut app, "继续检查中文\n");
        }
        app.session_mgr
            .current_mut()
            .subagent_sessions
            .get_mut("child")
            .expect("子会话存在")
            .last_view_refresh_at = Some(Instant::now() - SUBAGENT_VIEW_REFRESH_INTERVAL);
        assert!(app.flush_subagent_view_refresh());
        let messages = &app.session_mgr.current().subagent_sessions["child"].view_messages;
        let edits: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                MessageViewModel::ToolBlock {
                    tool_name,
                    diff_input: Some(input),
                    ..
                } if tool_name == "Edit" => Some(input),
                _ => None,
            })
            .collect();
        assert_eq!(edits.len(), 84, "所有 Edit 都应保留差异");
        assert!(
            edits
                .iter()
                .all(|input| input.new_content == "前文\n新值\n后文\n"),
            "周期刷新应复用历史，不因目标移除而丢失全文"
        );
    }
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(sub.view_rebuild_count, 21, "400 分片只构建 20 次流式视图");
    assert!(make_text(&sub.view_messages).ends_with(&"继续检查中文\n".repeat(400)));
}

#[tokio::test]
async fn test_subagent_tool_and_completion_flush_pending_text_immediately() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.switch_subagent_viewport(Some("child".into()));
    make_chunk(&mut app, "工具前正文\n");
    app.flush_subagent_view_refresh();
    make_refresh_later(&mut app);
    make_chunk(&mut app, "不能延迟的文本");
    app.handle_session_update_peri(&json!({
        "_peri": {"sourceAgentId": "child"},
        "update": {"sessionUpdate": "tool_call", "toolCallId": "read", "title": "Read", "rawInput": {"file_path": "不存在.rs"}},
    }));
    assert!(make_text(app.session_mgr.current().current_view_messages()).contains("不能延迟的文本"));
    app.handle_session_update_peri(&json!({
        "_peri": {"sourceAgentId": "child"},
        "update": {"sessionUpdate": "tool_call_update", "toolCallId": "read", "title": "Read", "status": "completed", "rawOutput": "读取完成"},
    }));
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"]
            .agent
            .session_tool_stats
            .get("Read"),
        Some(&1)
    );
    make_refresh_later(&mut app);
    make_chunk(&mut app, "最后的答复");
    app.handle_subagent_lifecycle(&json!({
        "event": "stopped", "instanceId": "child", "result": "最后的答复", "isError": false,
    }));
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(sub.status, SubagentRunStatus::Completed);
    assert!(make_text(&sub.view_messages).contains("最后的答复"));
    assert!(!sub.view_dirty);
    assert!(!sub.streaming_refresh_pending);
}

#[tokio::test]
async fn test_hidden_subagent_completion_materializes_only_when_opened() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.handle_subagent_event(
        "child",
        AgentEvent::StateSnapshot(vec![BaseMessage::ai("保存的完整结果")]),
    );
    app.handle_subagent_lifecycle(
        &json!({"event": "stopped", "instanceId": "child", "result": "摘要", "isError": false}),
    );
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(sub.status, SubagentRunStatus::Completed);
    assert_eq!(
        sub.view_rebuild_count, 0,
        "隐藏任务结束不应提前构造最终视图"
    );
    assert_eq!(sub.pipeline.completed_view_cache_stats(), (0, 0));
    app.switch_subagent_viewport(Some("child".into()));
    assert_eq!(
        make_text(app.session_mgr.current().current_view_messages()),
        "保存的完整结果"
    );
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        1
    );
}

#[tokio::test]
async fn test_hidden_subagent_completion_without_stream_preserves_result_fallback() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.handle_subagent_lifecycle(&json!({"event": "stopped", "instanceId": "child", "result": "只有最终结果", "isError": false}));
    assert_eq!(
        app.session_mgr.current().subagent_sessions["child"].view_rebuild_count,
        0
    );
    app.switch_subagent_viewport(Some("child".into()));
    assert_eq!(
        make_text(app.session_mgr.current().current_view_messages()),
        "只有最终结果"
    );
}

#[tokio::test]
async fn test_subagent_final_render_retries_latest_snapshot_after_queue_full() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.switch_subagent_viewport(Some("child".into()));
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(
        sender.try_send(RenderEvent::Rebuild(Vec::new())).is_ok(),
        "填满渲染队列"
    );
    app.session_mgr.current_mut().messages.render_tx = sender;
    make_chunk(&mut app, "中间帧\n");
    app.flush_subagent_view_refresh();
    make_chunk(&mut app, "完整的最终答复");
    app.handle_subagent_lifecycle(&json!({
        "event": "stopped", "instanceId": "child", "result": "完整的最终答复", "isError": false,
    }));
    receiver.try_recv().expect("移除队列中的旧帧");
    assert!(app.poll_agent(), "结束后无新通知仍需重试最终帧");
    let rendered = make_render_messages(receiver.try_recv().expect("应收到最终帧"));
    assert_eq!(make_text(&rendered), "中间帧\n完整的最终答复");
    assert!(
        !app.session_mgr
            .current_mut()
            .messages
            .retry_render_rebuild(),
        "成功后不重复发送"
    );
}

#[tokio::test]
async fn test_pending_subagent_render_is_replaced_when_switching_back_to_parent() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.switch_subagent_viewport(Some("child".into()));
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(
        sender.try_send(RenderEvent::Rebuild(Vec::new())).is_ok(),
        "填满渲染队列"
    );
    app.session_mgr.current_mut().messages.render_tx = sender;
    make_chunk(&mut app, "子会话的待发内容");
    app.flush_subagent_view_refresh();
    let parent = app.session_mgr.current().messages.view_messages.clone();
    app.switch_subagent_viewport(None);
    receiver.try_recv().expect("移除旧帧");
    app.poll_agent();
    let rendered = make_render_messages(receiver.try_recv().expect("应收到当前主视口"));
    assert_eq!(rendered, parent, "重试不能把已离开的子视口画回主视口");
}

#[tokio::test]
async fn test_render_retry_preserves_rebuild_scroll_anchor() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(
        sender.try_send(RenderEvent::Rebuild(Vec::new())).is_ok(),
        "填满渲染队列"
    );
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.session_mgr.current_mut().messages.view_messages = vec![
        MessageViewModel::system("第一条".into()),
        MessageViewModel::system("第二条".into()),
    ];
    app.render_rebuild_with_anchor(1);
    receiver.try_recv().expect("移除旧帧");
    app.poll_agent();
    assert!(
        matches!(receiver.try_recv().expect("应收到带锚点的完整快照"),
        RenderEvent::RebuildWithAnchor { anchor_message_idx: 1, messages } if messages.len() == 2)
    );
}

#[tokio::test]
async fn test_clear_replaces_pending_content_after_render_queue_full() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, mut receiver) = mpsc::channel(1);
    assert!(
        sender.try_send(RenderEvent::Rebuild(Vec::new())).is_ok(),
        "填满渲染队列"
    );
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.session_mgr
        .current_mut()
        .messages
        .view_messages
        .push(MessageViewModel::system("旧会话内容".into()));
    app.render_rebuild();
    app.session_mgr
        .current_mut()
        .messages
        .send_render_snapshot(RenderEvent::Clear);
    receiver.try_recv().expect("移除旧帧");
    app.poll_agent();
    assert!(matches!(
        receiver.try_recv().expect("应重试最新的清空指令"),
        RenderEvent::Clear
    ));
    assert!(
        !app.session_mgr
            .current_mut()
            .messages
            .retry_render_rebuild(),
        "清空后不能再追发旧内容"
    );
}

#[tokio::test]
async fn test_subagent_large_backlog_yields_to_keyboard_and_mouse_without_losing_completion() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app);
    app.handle_subagent_event(
        "child",
        AgentEvent::StateSnapshot(
            (0..300)
                .map(|index| BaseMessage::ai(format!("历史{index}\n")))
                .collect(),
        ),
    );
    app.switch_subagent_viewport(Some("child".into()));
    let (sender, receiver) = mpsc::unbounded_channel();
    let count = 30_232;
    for _ in 0..count {
        sender.send(AcpNotification::SessionUpdate { session_id: String::new(), params: json!({
            "_peri": {"sourceAgentId": "child"},
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "中文\n"}},
        }) }).expect("子事件应成功入队");
    }
    sender
        .send(AcpNotification::Peri {
            session_id: String::new(),
            method: "peri/subagent_lifecycle".into(),
            params: json!({
                "event": "stopped", "instanceId": "child", "result": "完成", "isError": false,
            }),
        })
        .expect("完成通知应成功入队");
    app.session_mgr.current_mut().agent.acp_notification_rx = Some(receiver);
    // 固定刷新窗口，避免 CI 调度停顿改变重建次数；结束事件仍必须强制刷新。
    make_refresh_later(&mut app);
    app.session_mgr.current_mut().ui.scrollbar_max_offset = 100;
    app.session_mgr.current_mut().ui.scrollbar_min_offset = 0;
    assert!(app.poll_agent());
    assert!(
        !app.session_mgr
            .current()
            .agent
            .acp_notification_rx
            .as_ref()
            .expect("通道存在")
            .is_empty(),
        "一帧不能耗尽积压"
    );
    crate::event::keyboard::handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
    )
    .expect("积压期间可打字");
    crate::event::handle_event(
        &mut app,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }),
    )
    .await
    .expect("积压期间可滚动");
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["x"]);
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 97);
    assert!(!app.session_mgr.current().ui.scroll_follow);
    let mut polls = 1;
    while app.session_mgr.current().subagent_sessions["child"]
        .status
        .is_active()
    {
        assert!(polls <= count + 1, "每批至少消费一条，完成通知不能饿死");
        app.poll_agent();
        polls += 1;
    }
    let sub = &app.session_mgr.current().subagent_sessions["child"];
    let expected: String = (0..300)
        .map(|index| format!("历史{index}\n"))
        .collect::<String>()
        + &"中文\n".repeat(count);
    assert_eq!(
        make_text(&sub.view_messages),
        expected,
        "最终内容不能遗漏或重复任何分片"
    );
    assert_eq!(sub.status, SubagentRunStatus::Completed);
    assert_eq!(
        sub.view_rebuild_count, 2,
        "只在切入和结束时重建，不能逐事件重建"
    );
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["x"]);
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 97);
}
