use crate::{
    app::{events::AgentEvent, App},
    ui::{
        message_view::{MessageViewModel, ToolPhase},
        render_thread::RenderEvent,
    },
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn make_start(id: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolStart {
        tool_call_id: id.into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "build".into(),
        input: serde_json::json!({"command": "build"}),
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_output(id: &str, tail: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolOutput {
        tool_call_id: id.into(),
        tail: tail.into(),
        total_lines: 1,
        total_bytes: tail.len(),
        elapsed_ms: 2500,
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_frames(receiver: &mut mpsc::Receiver<RenderEvent>) -> Vec<Vec<MessageViewModel>> {
    let mut frames = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        match event {
            RenderEvent::Rebuild(messages) | RenderEvent::RebuildWithAnchor { messages, .. } => {
                frames.push(messages)
            }
            _ => {}
        }
    }
    frames
}

fn make_tail<'a>(messages: &'a [MessageViewModel], id: &str) -> Option<&'a str> {
    messages.iter().find_map(|message| match message {
        MessageViewModel::ToolBlock {
            tool_call_id,
            live_output: Some(output),
            ..
        } if tool_call_id == id => Some(output.tail.as_str()),
        _ => None,
    })
}

#[tokio::test]
async fn test_output_refresh_coalesces_staggered_tools_and_flushes_without_new_events() {
    for with_acp in [false, true] {
        let (mut app, _handle) = App::new_headless(100, 30).await;
        let (sender, mut receiver) = mpsc::channel(32);
        app.session_mgr.current_mut().messages.render_tx = sender;
        let (_acp_sender, acp_receiver) = mpsc::unbounded_channel();
        app.session_mgr.current_mut().agent.acp_notification_rx = with_acp.then_some(acp_receiver);
        app.handle_agent_event(make_start("first", None));
        app.handle_agent_event(make_start("second", None));
        assert_eq!(make_frames(&mut receiver).len(), 2, "工具开始不受输出节流");
        app.handle_agent_event(make_output("first", "first tail", None));
        app.poll_agent();
        let first_frames = make_frames(&mut receiver);
        assert_eq!(first_frames.len(), 1, "首次输出应立即发布");
        assert_eq!(make_tail(&first_frames[0], "first"), Some("first tail"));
        app.handle_agent_event(make_output("second", "second tail", None));
        app.poll_agent();
        app.handle_agent_event(make_output("second", "second latest tail", None));
        app.poll_agent();
        assert!(
            make_frames(&mut receiver).is_empty(),
            "多个工具错开输出也不能绕过共享一秒窗口"
        );
        assert!(app.session_mgr.current().agent.tool_output_dirty);
        app.session_mgr
            .current_mut()
            .agent
            .last_tool_output_rebuild_at = Some(Instant::now() - Duration::from_secs(1));
        app.poll_agent();
        let trailing_frames = make_frames(&mut receiver);
        assert_eq!(trailing_frames.len(), 1, "安静期间轮询也要补发最新尾部");
        assert_eq!(make_tail(&trailing_frames[0], "first"), Some("first tail"));
        assert_eq!(
            make_tail(&trailing_frames[0], "second"),
            Some("second latest tail")
        );
        assert!(!app.session_mgr.current().agent.tool_output_dirty);
        app.poll_agent();
        assert!(
            make_frames(&mut receiver).is_empty(),
            "无dirty时不得重复重建"
        );
    }
}

#[tokio::test]
async fn test_output_refresh_forced_tool_completion_clears_deferred_refresh() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, mut receiver) = mpsc::channel(32);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.handle_agent_event(make_start("first", None));
    make_frames(&mut receiver);
    app.handle_agent_event(make_output("first", "first tail", None));
    app.poll_agent();
    make_frames(&mut receiver);
    app.handle_agent_event(make_output("first", "first latest tail", None));
    app.handle_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "first".into(),
        name: "Bash".into(),
        output: "final-result".into(),
        is_error: false,
        source_agent_id: None,
    });
    let frames = make_frames(&mut receiver);
    assert_eq!(frames.len(), 1, "完成事件不能等待输出刷新窗口");
    assert!(frames[0].iter().any(|vm| matches!(vm, MessageViewModel::ToolBlock { phase:ToolPhase::Finished, content, live_output:None, .. } if content == "final-result")));
    assert!(
        !app.session_mgr.current().agent.tool_output_dirty,
        "强制重建已消费旧dirty"
    );
    app.session_mgr
        .current_mut()
        .agent
        .last_tool_output_rebuild_at = Some(Instant::now() - Duration::from_secs(1));
    app.poll_agent();
    assert!(
        make_frames(&mut receiver).is_empty(),
        "完成后不得补发旧输出帧"
    );
}

#[tokio::test]
async fn test_output_refresh_subagent_and_main_share_one_view_clock() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, mut receiver) = mpsc::channel(32);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.handle_agent_event(AgentEvent::SubAgentStart {
        agent_id: "general-purpose".into(),
        instance_id: "child-id".into(),
        task_preview: "task".into(),
        is_background: false,
        prompt: None,
    });
    app.handle_agent_event(make_start("main", None));
    app.handle_agent_event(make_start("child", Some("child-id")));
    make_frames(&mut receiver);
    app.handle_agent_event(make_output("main", "main tail", None));
    app.poll_agent();
    assert_eq!(make_frames(&mut receiver).len(), 1);
    app.handle_agent_event(make_output("child", "child tail", Some("child-id")));
    app.poll_agent();
    assert!(
        make_frames(&mut receiver).is_empty(),
        "未聚焦子Agent的输出也应共用主视图窗口"
    );
    app.session_mgr.current_mut().focused_instance_id = Some("child-id".into());
    app.session_mgr
        .current_mut()
        .agent
        .last_tool_output_rebuild_at = Some(Instant::now() - Duration::from_secs(1));
    app.poll_agent();
    let frames = make_frames(&mut receiver);
    assert_eq!(frames.len(), 1, "子视口应由统一补发路径发布");
    assert_eq!(make_tail(&frames[0], "child"), Some("child tail"));
    assert!(
        make_tail(&frames[0], "main").is_none(),
        "子视口不能串入主工具输出"
    );
}

#[tokio::test]
async fn test_output_refresh_focused_child_completion_preserves_main_tail() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, mut receiver) = mpsc::channel(32);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.handle_agent_event(AgentEvent::SubAgentStart {
        agent_id: "general-purpose".into(),
        instance_id: "child-id".into(),
        task_preview: "task".into(),
        is_background: true,
        prompt: None,
    });
    app.handle_agent_event(make_start("main", None));
    app.handle_agent_event(make_start("child", Some("child-id")));
    app.handle_agent_event(make_output("main", "main latest tail", None));
    app.session_mgr.current_mut().focused_instance_id = Some("child-id".into());
    make_frames(&mut receiver);
    app.handle_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "child".into(),
        name: "Bash".into(),
        output: "child done".into(),
        is_error: false,
        source_agent_id: Some("child-id".into()),
    });
    let frames = make_frames(&mut receiver);
    assert_eq!(frames.len(), 1, "聚焦子工具完成应立即发布");
    assert!(frames[0].iter().any(|vm| matches!(vm, MessageViewModel::ToolBlock { phase: ToolPhase::Finished, content, .. } if content == "child done")));
    assert_eq!(
        make_tail(&app.session_mgr.current().messages.view_messages, "main"),
        Some("main latest tail"),
        "消费全局dirty前必须同步主视图尾部"
    );
    assert!(!app.session_mgr.current().agent.tool_output_dirty);
}
