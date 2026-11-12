use super::*;
use cc_agent::messages::MessageId;

#[derive(Default)]
struct MockTransport {
    notifications: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    fail_next_cwd: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl AcpTransport for MockTransport {
    async fn send_request(
        &self,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, crate::transport::types::AcpError> {
        unreachable!("事件接收器只发送通知")
    }

    async fn send_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), crate::transport::types::AcpError> {
        // 让并发测试覆盖通知发送期间的任务切换。
        tokio::task::yield_now().await;
        if method == "notifications/peri/cwd_changed"
            && self
                .fail_next_cwd
                .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(crate::transport::types::AcpError::new(
                -32603,
                "send failed",
            ));
        }
        self.notifications
            .lock()
            .expect("应可访问通知记录")
            .push((method.to_string(), params));
        Ok(())
    }

    async fn recv(&self) -> Option<crate::transport::types::IncomingMessage> {
        unreachable!("事件接收器只发送通知")
    }

    async fn send_response(
        &self,
        _id: crate::transport::types::RequestId,
        _result: Result<serde_json::Value, crate::transport::types::AcpError>,
    ) -> Result<(), crate::transport::types::AcpError> {
        unreachable!("事件接收器只发送通知")
    }
}

fn make_worktree_tool_end(name: &str) -> ExecutorEvent {
    ExecutorEvent::ToolEnd {
        message_id: MessageId::new(),
        tool_call_id: "worktree-call".into(),
        name: name.into(),
        output: "directory switched".into(),
        is_error: false,
        source_agent_id: None,
    }
}

#[tokio::test]
async fn test_transport_sink_notifies_worktree_enter_and_exit_before_tool_results() {
    let transport = std::sync::Arc::new(MockTransport::default());
    let cwd = cc_agent::session_cwd::new_session_cwd("/repo");
    let sink = TransportEventSink::new(transport.clone()).with_session_cwd(cwd.clone());
    for (previous, next, tool) in [
        ("/repo", "/repo/.claude/worktrees/test", "ExecuteExtraTool"),
        ("/repo/.claude/worktrees/test", "/repo", "ExitWorktree"),
    ] {
        cc_agent::session_cwd::switch(&cwd, next);
        sink.push_event("session", &make_worktree_tool_end(tool), 200_000)
            .await;
        {
            let notifications = transport.notifications.lock().expect("应可访问通知记录");
            assert_eq!(notifications[0].0, "notifications/peri/cwd_changed");
            assert_eq!(
                notifications[0].1,
                json!({"sessionId": "session", "oldCwd": previous, "newCwd": next})
            );
            assert_eq!(notifications[1].0, "session/update");
        }
        transport
            .notifications
            .lock()
            .expect("应可访问通知记录")
            .clear();
        // 同一轮后续工具结果不得重复通知。
        sink.push_event("session", &make_worktree_tool_end("Bash"), 200_000)
            .await;
        assert!(transport
            .notifications
            .lock()
            .expect("应可访问通知记录")
            .iter()
            .all(|(method, _)| method != "notifications/peri/cwd_changed"));
        transport
            .notifications
            .lock()
            .expect("应可访问通知记录")
            .clear();
    }
    sink.push_done("session").await;
    let notifications = transport.notifications.lock().expect("应可访问通知记录");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].0, "peri/agent_event_done");
}

#[tokio::test]
async fn test_transport_sink_notifies_cwd_before_done_without_tool_end() {
    let transport = std::sync::Arc::new(MockTransport::default());
    let cwd = cc_agent::session_cwd::new_session_cwd("/repo");
    let sink = TransportEventSink::new(transport.clone()).with_session_cwd(cwd.clone());
    cc_agent::session_cwd::switch(&cwd, "/worktree");
    sink.push_done("session").await;
    let notifications = transport.notifications.lock().expect("应可访问通知记录");
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[0].0, "notifications/peri/cwd_changed");
    assert_eq!(notifications[0].1["newCwd"], "/worktree");
    assert_eq!(notifications[1].0, "peri/agent_event_done");
}

#[tokio::test]
async fn test_transport_sink_unchanged_or_untracked_cwd_does_not_notify() {
    for track_cwd in [false, true] {
        let transport = std::sync::Arc::new(MockTransport::default());
        let mut sink = TransportEventSink::new(transport.clone());
        if track_cwd {
            sink = sink.with_session_cwd(cc_agent::session_cwd::new_session_cwd("/repo"));
        }
        sink.push_done("session").await;
        let notifications = transport.notifications.lock().expect("应可访问通知记录");
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, "peri/agent_event_done");
    }
}

#[tokio::test]
async fn test_transport_sink_concurrent_pumps_notify_cwd_once() {
    let transport = std::sync::Arc::new(MockTransport::default());
    let cwd = cc_agent::session_cwd::new_session_cwd("/repo");
    let sink = TransportEventSink::new(transport.clone()).with_session_cwd(cwd.clone());
    cc_agent::session_cwd::switch(&cwd, "/worktree");
    tokio::join!(sink.push_done("session"), sink.push_done("session"));
    let notifications = transport.notifications.lock().expect("应可访问通知记录");
    assert_eq!(
        notifications
            .iter()
            .filter(|(method, _)| method == "notifications/peri/cwd_changed")
            .count(),
        1
    );
    assert_eq!(notifications[0].0, "notifications/peri/cwd_changed");
}

#[tokio::test]
async fn test_transport_sink_retries_failed_cwd_notification() {
    let transport = std::sync::Arc::new(MockTransport::default());
    let cwd = cc_agent::session_cwd::new_session_cwd("/repo");
    let sink = TransportEventSink::new(transport.clone()).with_session_cwd(cwd.clone());
    cc_agent::session_cwd::switch(&cwd, "/worktree");
    transport
        .fail_next_cwd
        .store(true, std::sync::atomic::Ordering::SeqCst);
    sink.push_event("session", &make_worktree_tool_end("EnterWorktree"), 200_000)
        .await;
    transport
        .notifications
        .lock()
        .expect("应可访问通知记录")
        .clear();
    sink.push_done("session").await;
    let notifications = transport.notifications.lock().expect("应可访问通知记录");
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[0].0, "notifications/peri/cwd_changed");
    assert_eq!(
        notifications[0].1,
        json!({"sessionId": "session", "oldCwd": "/repo", "newCwd": "/worktree"})
    );
    assert_eq!(notifications[1].0, "peri/agent_event_done");
}

#[test]
fn test_stdio_sink_ignores_main_and_subagent_tool_output() {
    let event = ExecutorEvent::ToolOutput {
        message_id: MessageId::new(),
        tool_call_id: "streamed".into(),
        tail: "private running output".into(),
        total_lines: 1,
        total_bytes: 22,
        elapsed_ms: 1000,
        source_agent_id: None,
    };
    assert!(
        map_stdio_event(&event, 200_000).is_empty(),
        "stdio 不输出瞬时尾部"
    );
    let child = ExecutorEvent::SubagentEvent {
        source_agent_id: "child".into(),
        event: Box::new(event),
    };
    assert!(
        map_stdio_event(&child, 200_000).is_empty(),
        "子 Agent 输出同样不污染 stdout"
    );
}

#[test]
fn test_stdio_sink_ignores_main_and_subagent_tool_pending() {
    let event = ExecutorEvent::ToolPending {
        message_id: MessageId::new(),
        tool_call_id: "queued".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "git status"}),
        source_agent_id: None,
    };
    assert!(
        map_stdio_event(&event, 200_000).is_empty(),
        "stdio 不输出排队状态"
    );
    let child = ExecutorEvent::SubagentEvent {
        source_agent_id: "child".into(),
        event: Box::new(event),
    };
    assert!(
        map_stdio_event(&child, 200_000).is_empty(),
        "子 Agent 排队同样不污染 stdout"
    );
}

#[test]
fn test_stdio_sink_keeps_final_tool_result() {
    let event = ExecutorEvent::ToolEnd {
        message_id: MessageId::new(),
        tool_call_id: "finished".into(),
        name: "Bash".into(),
        output: "actual result".into(),
        is_error: false,
        source_agent_id: None,
    };
    let mapped = map_stdio_event(&event, 200_000);
    assert_eq!(mapped.len(), 1, "最终结果应继续传输");
    let update = serde_json::to_value(&mapped[0].updates[0]).expect("结果应可序列化");
    assert_eq!(update["rawOutput"], "actual result");
    assert_eq!(update["status"], "completed");
}
