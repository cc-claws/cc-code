//! cli_print StreamJson 事件输出的测试

use super::{OutputFormat, PrintCollector};
use cc_agent::agent::AgentEvent;
use cc_agent::messages::MessageId;

/// 构造 StreamJson 模式的收集器
fn make_collector() -> PrintCollector {
    PrintCollector::new(OutputFormat::StreamJson)
}

/// ToolStart 事件必须把真实 input 转发到 JSON，而非 null（回归 #tool_use-input-null）
#[test]
fn test_stream_json_tool_start_forwards_input() {
    let mut c = make_collector();
    let ev = AgentEvent::ToolStart {
        message_id: MessageId::new(),
        tool_call_id: "call_00_abc".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({"command": "pwd && ls -la", "description": "show dir"}),
        source_agent_id: None,
    };
    let line = c.handle_event(ev).expect("ToolStart 应产生输出行");
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["type"], "tool_use");
    assert_eq!(v["name"], "Bash");
    assert_eq!(v["id"], "call_00_abc");
    assert_eq!(v["input"]["command"], "pwd && ls -la");
    assert_eq!(v["input"]["description"], "show dir");
    assert!(!v["input"].is_null(), "input 不应为 null");
}

/// 空对象参数也应原样输出为空对象，而非 null
#[test]
fn test_stream_json_tool_start_empty_input_stays_object() {
    let mut c = make_collector();
    let ev = AgentEvent::ToolStart {
        message_id: MessageId::new(),
        tool_call_id: "tc1".to_string(),
        name: "Read".to_string(),
        input: serde_json::json!({}),
        source_agent_id: None,
    };
    let line = c.handle_event(ev).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["input"], serde_json::json!({}));
}

/// ToolEnd 事件仍输出 tool_result 结构
#[test]
fn test_stream_json_tool_end_shape() {
    let mut c = make_collector();
    let ev = AgentEvent::ToolEnd {
        message_id: MessageId::new(),
        tool_call_id: "tc1".to_string(),
        name: "Read".to_string(),
        output: "file contents".to_string(),
        is_error: false,
        source_agent_id: None,
    };
    let line = c.handle_event(ev).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["type"], "tool_result");
    assert_eq!(v["output"], "file contents");
}
