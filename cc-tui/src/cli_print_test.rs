//! cli_print StreamJson 事件输出的测试

use super::{OutputFormat, PrintCollector};
use cc_agent::agent::AgentEvent;
use cc_agent::messages::MessageId;

/// 构造 StreamJson 模式的收集器
fn make_collector() -> PrintCollector {
    PrintCollector::new(OutputFormat::StreamJson)
}

#[test]
fn test_print_sink_ignores_tool_output() {
    for format in [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::StreamJson,
    ] {
        let mut collector = PrintCollector::new(format);
        let event = AgentEvent::ToolOutput {
            message_id: MessageId::new(),
            tool_call_id: "running-command".into(),
            tail: "running tail".into(),
            total_lines: 1,
            total_bytes: 12,
            elapsed_ms: 1000,
            source_agent_id: None,
        };
        assert!(
            collector.handle_event(event).is_none(),
            "增量输出不能污染任何 print 格式"
        );
        assert!(collector.text_buffer.is_empty(), "增量输出不写入最终文本");
    }
}

#[test]
fn test_print_sink_ignores_tool_pending() {
    for format in [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::StreamJson,
    ] {
        let mut collector = PrintCollector::new(format);
        let output = collector.handle_event(AgentEvent::ToolPending {
            message_id: MessageId::new(),
            tool_call_id: "pending-command".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": "git status"}),
            source_agent_id: None,
        });
        assert!(output.is_none(), "排队状态不能污染任何 print 输出格式");
        assert!(collector.text_buffer.is_empty(), "排队事件不写入最终文本");
    }
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
        file_snapshot: None,
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
        file_snapshot: None,
        source_agent_id: None,
    };
    let line = c.handle_event(ev).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["input"], serde_json::json!({}));
}

#[test]
fn test_stream_json_tool_start_omits_file_snapshot() {
    let mut collector = make_collector();
    let input = serde_json::json!({"file_path": "a.txt", "content": "新内容\n"});
    let event = AgentEvent::ToolStart {
        message_id: MessageId::new(),
        tool_call_id: "write-preview".into(),
        name: "Write".into(),
        input: input.clone(),
        file_snapshot: Some(cc_agent::tools::FileSnapshot {
            old_content: Some("旧内容仅用于界面预览\n".into()),
        }),
        source_agent_id: None,
    };
    let line = collector
        .handle_event(event)
        .expect("Write 开始应产生输出行");
    let output: serde_json::Value = serde_json::from_str(&line).expect("输出应为有效 JSON");
    assert_eq!(
        output,
        serde_json::json!({"type": "tool_use", "id": "write-preview", "name": "Write", "input": input}),
        "文件快照不能混入 print 的工具入参或输出结构"
    );
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
