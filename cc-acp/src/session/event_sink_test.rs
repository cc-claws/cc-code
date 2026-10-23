use super::*;
use cc_agent::messages::MessageId;

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
