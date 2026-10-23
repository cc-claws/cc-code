use super::map_executor_event;
use crate::app::AgentEvent;
use cc_agent::agent::events::{AgentEvent as ExecutorEvent, TodoEntry, TodoStatus};

#[test]
fn test_map_executor_event_tool_output_uses_session_update() {
    let event = ExecutorEvent::ToolOutput {
        message_id: Default::default(),
        tool_call_id: "running-tool".into(),
        tail: "output".into(),
        total_lines: 1,
        total_bytes: 6,
        elapsed_ms: 1000,
        source_agent_id: None,
    };
    assert!(
        map_executor_event(event, "/tmp").is_none(),
        "输出只从 ACP session/update 消费"
    );
}

#[test]
fn test_map_executor_event_tool_pending_uses_session_update() {
    let event = ExecutorEvent::ToolPending {
        message_id: Default::default(),
        tool_call_id: "pending-tool".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "git status"}),
        source_agent_id: None,
    };
    assert!(
        map_executor_event(event, "/tmp").is_none(),
        "排队状态只从 ACP session/update 消费，避免重复工具行"
    );
}

#[test]
fn test_map_executor_event_todo_update_returns_none() {
    // TodoUpdate 属于类别①，已由 session/update → handle_session_update_peri() 处理
    let event = ExecutorEvent::TodoUpdate(vec![
        TodoEntry {
            content: "Fix the bug".into(),
            active_form: Some("Fixing the bug".into()),
            status: TodoStatus::InProgress,
        },
        TodoEntry {
            content: "Write tests".into(),
            active_form: None,
            status: TodoStatus::Pending,
        },
    ]);

    let result = map_executor_event(event, "/tmp");
    assert!(
        result.is_none(),
        "TodoUpdate should return None — handled by session/update bridge"
    );
}

#[test]
fn test_map_executor_event_execution_failed() {
    let event = ExecutorEvent::AgentExecutionFailed {
        message: "LLM HTTP 错误 (400)".to_string(),
    };
    let result = map_executor_event(event, "/tmp");
    assert!(result.is_some(), "AgentExecutionFailed should map to Some");
    match result.unwrap() {
        AgentEvent::Error(msg) => {
            assert_eq!(msg, "LLM HTTP 错误 (400)");
        }
        _ => panic!("Expected AgentEvent::Error, got a different variant"),
    }
}

#[test]
fn test_map_executor_event_interrupted() {
    let event = ExecutorEvent::AgentExecutionFailed {
        message: "Interrupted by user".to_string(),
    };
    let result = map_executor_event(event, "/tmp");
    assert!(
        result.is_some(),
        "AgentExecutionFailed(Interrupted) should map to Some"
    );
    match result.unwrap() {
        AgentEvent::Interrupted => {}
        _ => panic!("Expected AgentEvent::Interrupted, got a different variant"),
    }
}

#[test]
fn test_map_executor_event_text_chunk_returns_none() {
    // TextChunk 属于类别①，已由 session/update 处理
    let event = ExecutorEvent::TextChunk {
        message_id: Default::default(),
        chunk: "hello".to_string(),
        source_agent_id: None,
    };
    let result = map_executor_event(event, "/tmp");
    assert!(result.is_none(), "TextChunk should return None");
}

#[test]
fn test_map_executor_event_compact_started() {
    let auto_event = ExecutorEvent::CompactStarted { manual: false };
    let mapped_auto = map_executor_event(auto_event, "/tmp");
    assert!(matches!(
        mapped_auto,
        Some(AgentEvent::CompactStarted { manual: false })
    ));

    let manual_event = ExecutorEvent::CompactStarted { manual: true };
    let mapped_manual = map_executor_event(manual_event, "/tmp");
    assert!(matches!(
        mapped_manual,
        Some(AgentEvent::CompactStarted { manual: true })
    ));
}

#[test]
fn test_map_executor_event_tool_start_returns_none() {
    // ToolStart 属于类别①
    let event = ExecutorEvent::ToolStart {
        message_id: Default::default(),
        tool_call_id: "tc_1".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
        file_snapshot: None,
        source_agent_id: None,
    };
    let result = map_executor_event(event, "/tmp");
    assert!(result.is_none(), "ToolStart should return None");
}

#[test]
fn test_map_executor_event_tool_end_returns_none() {
    // ToolEnd 属于类别①
    let event = ExecutorEvent::ToolEnd {
        message_id: Default::default(),
        tool_call_id: "tc_1".to_string(),
        name: "Bash".to_string(),
        output: "file1.txt\nfile2.txt".to_string(),
        is_error: false,
        source_agent_id: None,
    };
    let result = map_executor_event(event, "/tmp");
    assert!(result.is_none(), "ToolEnd should return None");
}
