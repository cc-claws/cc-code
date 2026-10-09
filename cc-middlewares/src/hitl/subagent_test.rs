use serde_json::json;

use super::*;

#[test]
fn test_deferred_command_rewrite_updates_actual_parameters() {
    let original = ToolCall::new(
        "rewrite-test",
        crate::tool_search::EXECUTE_EXTRA_TOOL_NAME,
        json!({"tool_name":"Bash","params":{"command":"cargo build","timeout":1000}}),
    );
    let rewritten = apply_command_rewrite(&original, Some("rtk cargo build".to_string()));
    assert_eq!(rewritten.input["params"]["command"], "rtk cargo build");
    assert_eq!(rewritten.input["params"]["timeout"], 1000);
    assert!(
        rewritten.input.get("command").is_none(),
        "代理调用不能在顶层写入无效命令"
    );
    assert_eq!(
        original.input["params"]["command"], "cargo build",
        "原始意图应保留用于显式规则"
    );
}

#[test]
fn test_command_rewrite_preserves_direct_and_unchanged_calls() {
    let original = ToolCall::new("rewrite-test", "Bash", json!({"command":"cargo build"}));
    let rewritten = apply_command_rewrite(&original, Some("rtk cargo build".to_string()));
    assert_eq!(rewritten.input["command"], "rtk cargo build");
    assert_eq!(apply_command_rewrite(&original, None).input, original.input);
}
