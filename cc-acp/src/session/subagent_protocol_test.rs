use super::*;
use crate::{
    event::map_event,
    session::event_sink::{EventSink, TransportEventSink},
    transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport},
};
use std::{sync::Arc, time::Duration};

fn make_child_event(id: &str, text: &str) -> AgentEvent {
    AgentEvent::SubagentEvent {
        source_agent_id: id.into(),
        event: Box::new(AgentEvent::AiReasoning(text.into())),
    }
}

#[test]
fn test_reasoning_keeps_independent_child_sources() {
    for (id, text) in [("child-one", "武汉天气"), ("child-two", "随州天气")] {
        let event = make_child_event(id, text);
        let mapped = map_event(&event, 100_000);
        assert_eq!(mapped.len(), 1, "每个事件只能映射一次");
        assert_eq!(mapped[0].source_agent_id.as_deref(), Some(id));
        assert!(!mapped[0].forward_to_tui, "标准事件禁止重复走原始事件通道");
        let payload = session_update_payload(
            "root",
            &mapped[0].updates[0],
            mapped[0].source_agent_id.as_deref(),
        )
        .expect("事件应可以序列化");
        assert_eq!(payload["_peri"]["sourceAgentId"], id);
        assert_eq!(payload["update"]["content"]["text"], text);
    }
    let main = map_event(&AgentEvent::AiReasoning("主任务".into()), 100_000);
    assert!(
        main[0].source_agent_id.is_none(),
        "主任务不应获得子任务标识"
    );
}

#[test]
fn test_sdk_extensions_preserve_target_thread_identifier() {
    let prompt: RoutedPromptRequest = serde_json::from_value(json!({
        "sessionId": "root", "targetThreadId": "child-one",
        "prompt": [{ "type": "text", "text": "补充条件" }],
    }))
    .expect("定向输入应可解析");
    assert_eq!(prompt.target_thread_id.as_deref(), Some("child-one"));
    let cancel: RoutedCancelNotification = serde_json::from_value(json!({
        "sessionId": "root", "targetThreadId": "child-two",
    }))
    .expect("定向取消应可解析");
    assert_eq!(cancel.target_thread_id.as_deref(), Some("child-two"));
    assert_eq!(
        target_thread_id(&json!({"_meta":{"_peri":{"targetThreadId":"child-three"}}})),
        Some("child-three")
    );
}

#[test]
fn test_nested_usage_keeps_innermost_source_and_cache_statistics() {
    let event = AgentEvent::SubagentEvent {
        source_agent_id: "parent-child".into(),
        event: Box::new(AgentEvent::SubagentEvent {
            source_agent_id: "nested-child".into(),
            event: Box::new(AgentEvent::LlmCallEnd {
                step: 1, model: "child-model".into(), output: "done".into(), stop_reason: None,
                usage: Some(cc_agent::llm::types::TokenUsage {
                    input_tokens: 40, output_tokens: 9, cache_creation_input_tokens: None,
                    cache_read_input_tokens: Some(20), request_id: None,
                }),
            }),
        }),
    };
    let mapped = map_event(&event, 100_000);
    assert_eq!(mapped[0].source_agent_id.as_deref(), Some("nested-child"));
    let payload = session_update_payload("root", &mapped[0].updates[0], mapped[0].source_agent_id.as_deref()).expect("用量应可序列化");
    assert_eq!(payload["update"]["_meta"]["inputTokens"], 40);
    assert_eq!(payload["update"]["_meta"]["cacheReadTokens"], 20);
}

#[tokio::test]
async fn test_lifecycle_is_sent_once_with_preview_and_timeout() {
    let (client, server) = mpsc_transport_pair();
    let sink = TransportEventSink::new(Arc::new(server));
    sink.push_event(
        "root",
        &AgentEvent::SubagentStarted {
            agent_name: "general-purpose".into(),
            instance_id: "child-one".into(),
            is_background: true,
            task_preview: "查询武汉天气".into(),
            timeout_secs: Some(3600),
            model: Some("test-model".into()),
            prompt: None,
        },
        100_000,
    )
    .await;
    let received = tokio::time::timeout(Duration::from_secs(1), client.recv())
        .await
        .expect("应立即收到生命周期");
    let Some(IncomingMessage::Notification { method, params }) = received else {
        panic!("应收到通知");
    };
    assert_eq!(method, "peri/subagent_lifecycle");
    assert_eq!(params["taskPreview"], "查询武汉天气");
    assert_eq!(params["timeoutSecs"], 3600);
    assert!(
        tokio::time::timeout(Duration::from_millis(25), client.recv())
            .await
            .is_err(),
        "生命周期不应再走第二条通道"
    );
}

#[tokio::test]
async fn test_child_events_remain_available_after_root_done() {
    let (client, server) = mpsc_transport_pair();
    let sink = TransportEventSink::new(Arc::new(server));
    sink.push_done("root").await;
    sink.push_event("root", &make_child_event("child-one", "仍在处理"), 100_000)
        .await;
    let first = client.recv().await;
    assert!(
        matches!(first, Some(IncomingMessage::Notification { method, .. }) if method == "peri/agent_event_done")
    );
    let second = client.recv().await;
    let Some(IncomingMessage::Notification { method, params }) = second else {
        panic!("后台事件应继续到达");
    };
    assert_eq!(method, "session/update");
    assert_eq!(params["_peri"]["sourceAgentId"], "child-one");
}

#[test]
fn test_unknown_child_and_empty_mailbox_messages_are_rejected() {
    let registry = BackgroundTaskRegistry::default();
    assert!(cancel_target(&registry, "missing").is_err());
    assert!(cancel_target(&registry, "*").is_ok());
    for content in [
        json!(""),
        json!([{ "type": "image", "data": "x" }]),
        json!("hello"),
    ] {
        assert!(
            enqueue_message(
                &registry,
                &json!({
                    "targetThreadId": "missing", "message": { "content": content },
                })
            )
            .is_err(),
            "无效目标或消息必须报错"
        );
    }
}
