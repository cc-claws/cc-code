use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use peri_agent::llm::types::{StopReason, TokenUsage};

use super::*;

#[test]
fn test_llm_call_end_maps_to_enriched_usage_update() {
    let event = ExecutorEvent::LlmCallEnd {
        step: 1,
        model: "claude-sonnet-4-20250514".to_string(),
        output: "Hello".to_string(),
        usage: Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: Some(10),
            cache_read_input_tokens: Some(200),
            request_id: Some("req-123".to_string()),
        }),
        stop_reason: Some(StopReason::EndTurn),
    };

    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1, "应产出 1 个 MappedEvent");

    let m = &mapped[0];
    assert!(!m.forward_to_tui, "LlmCallEnd 不应转发到 TUI");
    assert_eq!(m.updates.len(), 1, "应包含 1 个 SessionUpdate");

    match &m.updates[0] {
        SessionUpdate::UsageUpdate(usage) => {
            assert_eq!(usage.used, 150);
            assert_eq!(usage.size, 200_000);
            let meta = usage.meta.as_ref().expect("_meta 应包含详细 usage");
            assert_eq!(meta.get("inputTokens").unwrap().as_u64(), Some(100));
            assert_eq!(meta.get("outputTokens").unwrap().as_u64(), Some(50));
            assert_eq!(meta.get("cacheCreationTokens").unwrap().as_u64(), Some(10));
            assert_eq!(meta.get("cacheReadTokens").unwrap().as_u64(), Some(200));
            assert_eq!(
                meta.get("model").unwrap().as_str(),
                Some("claude-sonnet-4-20250514")
            );
            assert_eq!(meta.get("stopReason").unwrap().as_str(), Some("end_turn"));
        }
        other => panic!("预期 UsageUpdate，实际: {:?}", other),
    }
}

#[test]
fn test_llm_call_end_no_optional_fields() {
    // 无缓存 token、无 stop_reason 时 _meta 不含可选字段
    let event = ExecutorEvent::LlmCallEnd {
        step: 2,
        model: "gpt-4o".to_string(),
        output: String::new(),
        usage: Some(TokenUsage {
            input_tokens: 200,
            output_tokens: 30,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            request_id: None,
        }),
        stop_reason: None,
    };

    let mapped = map_event(&event, 128_000);
    assert_eq!(mapped.len(), 1);
    assert!(!mapped[0].forward_to_tui);

    match &mapped[0].updates[0] {
        SessionUpdate::UsageUpdate(usage) => {
            assert_eq!(usage.used, 230);
            let meta = usage.meta.as_ref().unwrap();
            assert!(meta.get("cacheCreationTokens").is_none());
            assert!(meta.get("cacheReadTokens").is_none());
            assert!(meta.get("stopReason").is_none());
        }
        other => panic!("预期 UsageUpdate，实际: {:?}", other),
    }
}

#[test]
fn test_llm_call_end_no_usage_filtered() {
    let event = ExecutorEvent::LlmCallEnd {
        step: 1,
        model: "test".to_string(),
        output: "ERROR".to_string(),
        usage: None,
        stop_reason: None,
    };
    let mapped = map_event(&event, 200_000);
    assert!(
        mapped.is_empty()
            || mapped
                .iter()
                .all(|m| m.updates.is_empty() && !m.forward_to_tui),
        "LlmCallEnd usage=None 应被过滤"
    );
}

#[test]
fn test_context_warning_is_tui_only() {
    let event = ExecutorEvent::ContextWarning {
        used_tokens: 150_000,
        total_tokens: 200_000,
        percentage: 75.0,
    };
    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1);
    assert!(mapped[0].forward_to_tui, "ContextWarning 应转发到 TUI");
    assert!(
        mapped[0].updates.is_empty(),
        "ContextWarning 不应产生 SessionUpdate"
    );
}

#[test]
fn test_llm_retrying_is_tui_only() {
    let event = ExecutorEvent::LlmRetrying {
        attempt: 2,
        max_attempts: 3,
        delay_ms: 1000,
        error: "timeout".to_string(),
    };
    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1);
    assert!(mapped[0].forward_to_tui, "LlmRetrying 应转发到 TUI");
    assert!(
        mapped[0].updates.is_empty(),
        "LlmRetrying 不应产生 SessionUpdate"
    );
}

#[test]
fn test_stop_reason_display_roundtrip() {
    for (reason, expected) in [
        (StopReason::EndTurn, "end_turn"),
        (StopReason::ToolUse, "tool_use"),
        (StopReason::MaxTokens, "max_tokens"),
        (StopReason::Other("custom".to_string()), "custom"),
    ] {
        let s = reason.to_string();
        assert_eq!(&s, expected, "Display 不匹配");
        assert_eq!(StopReason::from_display(&s), reason, "from_display 不匹配");
    }
}
