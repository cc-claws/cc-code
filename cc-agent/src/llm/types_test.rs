use super::*;

#[test]
fn test_from_openai_stop() {
    assert_eq!(StopReason::from_openai("stop"), StopReason::EndTurn);
}

#[test]
fn test_from_openai_tool_calls() {
    assert_eq!(StopReason::from_openai("tool_calls"), StopReason::ToolUse);
}

#[test]
fn test_from_openai_length() {
    assert_eq!(StopReason::from_openai("length"), StopReason::MaxTokens);
}

#[test]
fn test_from_openai_unknown() {
    assert!(matches!(
        StopReason::from_openai("content_filter"),
        StopReason::Other(_)
    ));
}

#[test]
fn test_from_display_end_turn() {
    assert_eq!(StopReason::from_display("end_turn"), StopReason::EndTurn);
}

#[test]
fn test_from_display_tool_use() {
    assert_eq!(StopReason::from_display("tool_use"), StopReason::ToolUse);
}

#[test]
fn test_from_display_max_tokens() {
    assert_eq!(
        StopReason::from_display("max_tokens"),
        StopReason::MaxTokens
    );
}

#[test]
fn test_from_display_unknown() {
    assert!(matches!(
        StopReason::from_display("pause_turn"),
        StopReason::Other(_)
    ));
}

#[test]
fn test_stop_reason_equality() {
    assert_eq!(StopReason::EndTurn, StopReason::EndTurn);
    assert_ne!(StopReason::EndTurn, StopReason::ToolUse);
}
