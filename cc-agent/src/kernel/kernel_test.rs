use serde_json::{json, Value};
use std::sync::Arc;

use super::*;
use crate::agent::react::{Reasoning, ToolCall};
use crate::messages::{BaseMessage, ContentBlock, MessageContent};
use crate::tools::{ToolDefinition, ToolParameters};

fn make_def(name: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: format!("desc of {name}"),
        parameters: ToolParameters::Json(parameters),
    }
}

fn read_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {"type": "string"},
            "offset": {"type": "integer"}
        },
        "required": ["file_path"]
    })
}

#[test]
fn test_kernel_all_specs_ids_unique() {
    let mut ids: Vec<&str> = ALL_SPECS.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids, vec!["cc", "dsh", "pi"]);
    assert_eq!(ids.len(), count);
    for spec in ALL_SPECS {
        assert!(Kernel::from_config_checked(spec.id).is_some());
    }
}

#[test]
fn test_unknown_kernel_falls_back_cc() {
    assert_eq!(Kernel::from_config(Some("xxx")), Kernel::cc());
    assert_eq!(Kernel::from_config(None), Kernel::cc());
    assert!(Kernel::from_config_checked("xxx").is_none());
    assert!(Kernel::try_from_id("dsh-v2").is_err());
    assert_eq!(Kernel::try_from_id("pi").unwrap().id(), "pi");
}

static BY_MODEL_PROMPT: &str = "default prompt";
static BY_MODEL_SPEC: KernelSpec = KernelSpec {
    id: "by-model-test",
    prompt: PromptSource::ByModel {
        default: BY_MODEL_PROMPT,
        overrides: &[("gpt-5", "gpt5 prompt"), ("gpt-5.2", "gpt52 prompt")],
    },
    tools: &[],
    governed_tools: &[],
    meta_tools: &[],
    exposes_tool_search: false,
};

#[test]
fn test_prompt_source_single() {
    let kernel = Kernel::from_config_checked("pi").unwrap();
    let a = kernel.prompt_for("gpt-4o");
    let b = kernel.prompt_for("claude-sonnet");
    assert_eq!(a, b);
    assert!(a.contains("pi"));
}

#[test]
fn test_prompt_source_by_model() {
    let kernel = Kernel(&BY_MODEL_SPEC);
    assert_eq!(kernel.prompt_for("gpt-5"), "gpt5 prompt");
    assert_eq!(kernel.prompt_for("gpt-5-mini"), "gpt5 prompt");
    assert_eq!(kernel.prompt_for("gpt-5.2-codex"), "gpt52 prompt");
    assert_eq!(kernel.prompt_for("gpt-52-legacy"), BY_MODEL_PROMPT);
    assert_eq!(kernel.prompt_for("openai/gpt-5"), BY_MODEL_PROMPT);
}

#[test]
fn test_mutation_form_from_spec() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let form = pi
        .spec()
        .tools
        .iter()
        .find(|t| t.canonical == "Edit")
        .and_then(|t| t.mutation_form)
        .unwrap_or_default();
    assert_eq!(form, MutationForm::BatchReplace);
    let cc = Kernel::cc();
    assert_eq!(
        cc.spec()
            .tools
            .iter()
            .find(|t| t.canonical == "Edit")
            .and_then(|t| t.mutation_form)
            .unwrap_or_default(),
        MutationForm::SingleReplace
    );
}

#[test]
fn test_kernel_canonical_superset() {
    let kernel = Kernel::cc();
    for name in ["Read", "read", "read_file", "reading"] {
        assert_eq!(kernel.canonical_name(name), Some("Read"), "name={name}");
    }
    assert_eq!(kernel.canonical_name("find"), Some("Glob"));
    assert_eq!(kernel.canonical_name("shell"), Some("Bash"));
    assert_eq!(kernel.canonical_name("task"), Some("Agent"));
    assert_eq!(kernel.canonical_name("mcp__slack__send"), None);
}

#[test]
fn test_cc_kernel_equivalence() {
    let defs = vec![make_def("Read", read_schema()), make_def("Bash", json!({}))];
    let before = serde_json::to_string(&defs).unwrap();
    let after = serde_json::to_string(&Kernel::cc().translate_definitions(defs.clone())).unwrap();
    assert_eq!(before, after);
    assert_eq!(Kernel::cc().translate_definitions(defs)[0].name, "Read");
}

#[test]
fn test_translate_read_schema_pi() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let out = pi.translate_definitions(vec![make_def("Read", read_schema())]);
    assert_eq!(out[0].name, "read");
    let schema = out[0].parameters.as_json().unwrap();
    assert!(schema["properties"].get("path").is_some());
    assert!(schema["properties"].get("file_path").is_none());
    assert_eq!(schema["required"], json!(["path"]));
}

#[test]
fn test_translate_edit_schema_pi() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let batch = json!({
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "edits": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "oldText": {"type": "string"},
                        "newText": {"type": "string"}
                    },
                    "required": ["oldText", "newText"]
                }
            }
        },
        "required": ["path", "edits"]
    });
    let out = pi.translate_definitions(vec![make_def("Edit", batch.clone())]);
    assert_eq!(out[0].name, "edit");
    assert_eq!(*out[0].parameters.as_json().unwrap(), batch);
}

#[test]
fn test_normalize_input_pi_batch_edit() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let input = json!({
        "path": "a.rs",
        "edits": [{"oldText": "foo", "newText": "bar"}]
    });
    let out = pi.normalize_input("Edit", &input);
    assert_eq!(out["file_path"], "a.rs");
    assert!(out.get("path").is_none());
    assert_eq!(out["edits"][0]["old_string"], "foo");
    assert_eq!(out["edits"][0]["new_string"], "bar");
}

#[test]
fn test_pi_kernel_no_tool_search() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert!(!pi.exposes_tool_search());
    assert!(pi.spec().meta_tools.is_empty());
    assert!(pi.should_hide("SearchExtraTools"));
    assert!(pi.should_hide("ExecuteExtraTool"));
}

#[test]
fn test_pi_kernel_ls_powershell_hidden() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert_eq!(pi.dialect_name("ls"), None);
    assert_eq!(pi.dialect_name("powershell"), None);
    assert!(!pi.is_exposed("ls"));
    assert!(!pi.is_exposed("powershell"));
}

#[test]
fn test_ungoverned_tools_never_hidden() {
    for kernel in [
        Kernel::cc(),
        Kernel::from_config_checked("dsh").unwrap(),
        Kernel::from_config_checked("pi").unwrap(),
    ] {
        for name in [
            "mcp__slack__send",
            "mcp_read_resource",
            "LSP",
            "CronRemove",
            "AgentResult",
            "some_future_tool",
        ] {
            assert!(!kernel.should_hide(name), "{} hid {name}", kernel.id());
        }
    }
}

#[test]
fn test_governed_unlisted_hidden() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert!(pi.should_hide("WebFetch"));
    assert!(pi.should_hide("TodoWrite"));
    assert!(!pi.should_hide("Read"));
    assert!(!pi.should_hide("Bash"));
    let dsh = Kernel::from_config_checked("dsh").unwrap();
    assert!(dsh.should_hide("WebSearch"));
    assert!(!dsh.should_hide("read"));
}

#[test]
fn test_governed_superset_of_tools() {
    for spec in ALL_SPECS {
        for tool in spec.tools {
            assert!(
                spec.governed_tools.contains(&tool.canonical),
                "{}: {} listed but not governed",
                spec.id,
                tool.canonical
            );
        }
    }
}

#[test]
fn test_meta_tools_consistency() {
    for spec in ALL_SPECS {
        assert_eq!(
            spec.exposes_tool_search,
            !spec.meta_tools.is_empty(),
            "{} meta/exposes mismatch",
            spec.id
        );
    }
    let cc = Kernel::cc();
    assert!(cc.is_meta_tool("SearchExtraTools"));
    assert!(!cc.should_hide("SearchExtraTools"));
}

#[test]
fn test_normalize_reasoning_updates_source_message() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let mut reasoning = Reasoning::with_tools(
        "",
        vec![ToolCall::new("c1", "read", json!({"path": "a.rs"}))],
    );
    reasoning.source_message = Some(BaseMessage::Ai {
        id: crate::messages::MessageId::new(),
        content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
            id: "c1".to_string(),
            name: "read".to_string(),
            input: json!({"path": "a.rs"}),
        }]),
        tool_calls: vec![crate::messages::ToolCallRequest::new(
            "c1",
            "read",
            json!({"path": "a.rs"}),
        )],
    });
    let out = pi.normalize_reasoning(reasoning);
    assert_eq!(out.tool_calls[0].name, "Read");
    assert_eq!(out.tool_calls[0].input["file_path"], "a.rs");
    assert_eq!(out.raw_tool_calls.as_ref().unwrap()[0].name, "read");
    let msg = out.source_message.unwrap();
    let BaseMessage::Ai {
        content,
        tool_calls,
        ..
    } = msg
    else {
        panic!("expected Ai message");
    };
    let blocks = content.content_blocks();
    assert!(matches!(&blocks[0], ContentBlock::ToolUse { name, .. } if name == "Read"));
    assert_eq!(tool_calls[0].name, "Read");
    assert_eq!(tool_calls[0].arguments["file_path"], "a.rs");
}

#[test]
fn test_micro_compact_after_normalize() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let reasoning = Reasoning::with_tools(
        "",
        vec![ToolCall::new("c1", "bash", json!({"command": "ls"}))],
    );
    let out = pi.normalize_reasoning(reasoning);
    let compactable = ["Bash", "Read", "Glob", "Grep", "Write", "Edit"];
    assert!(compactable.contains(&out.tool_calls[0].name.as_str()));
}

#[test]
fn test_dialectize_input_edit_single_to_batch() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let input = json!({
        "file_path": "a.rs",
        "old_string": "foo",
        "new_string": "bar",
        "replace_all": false
    });
    let out = pi.dialectize_input("Edit", &input);
    assert_eq!(out["path"], "a.rs");
    assert_eq!(out["edits"][0]["oldText"], "foo");
    assert_eq!(out["edits"][0]["newText"], "bar");
    assert!(out.get("old_string").is_none());
    assert!(out.get("file_path").is_none());
}

#[test]
fn test_dialectize_param_names() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert_eq!(
        pi.dialectize_param_names("missing file_path, got old_string"),
        "missing path, got oldText"
    );
    assert_eq!(
        Kernel::cc().dialectize_param_names("missing file_path"),
        "missing file_path"
    );
}

#[test]
fn test_dialectize_hint() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert_eq!(
        pi.dialectize_hint("call the Read tool with file_path=..."),
        "call the read tool with file_path=..."
    );
    assert!(matches!(
        Kernel::cc().dialectize_hint("Read"),
        Cow::Borrowed(_)
    ));
}

#[test]
fn test_suggest_mismatch_uses_dialect() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let hint = pi
        .suggest_mismatch("Grep", &json!({"path": "a.rs"}))
        .unwrap();
    assert!(hint.contains("read"), "{hint}");
    assert!(!hint.contains("Read"), "{hint}");
}

#[test]
fn test_tool_definition_serde_format() {
    let json_value = read_schema();
    let def = make_def("Read", json_value.clone());
    let encoded = serde_json::to_value(&def).unwrap();
    assert_eq!(encoded["parameters"], json_value);
    let raw = ToolParameters::RawText;
    assert_eq!(serde_json::to_value(&raw).unwrap(), Value::Null);
}

#[test]
fn test_tool_parameters_deserialize_legacy() {
    let decoded: ToolParameters = serde_json::from_value(Value::Null).unwrap();
    assert!(matches!(decoded, ToolParameters::Json(Value::Null)));
    let decoded: ToolParameters = serde_json::from_value(read_schema()).unwrap();
    assert_eq!(decoded.kind(), ToolParametersKind::JsonSchema);
}

#[test]
fn test_all_tools_json_schema() {
    for spec in ALL_SPECS {
        for tool in spec.tools {
            assert_eq!(tool.parameters_kind, ToolParametersKind::JsonSchema);
        }
    }
}

#[test]
fn test_tool_filter_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Arc<dyn Fn(&str) -> bool + Send + Sync>>();
    let kernel = Kernel::from_config_checked("pi").unwrap();
    let filter: Arc<dyn Fn(&str) -> bool + Send + Sync> =
        Arc::new(move |name| kernel.should_hide(name));
    assert!(filter("WebFetch"));
    assert!(!filter("Read"));
    assert!(!filter("mcp__slack__send"));
}

#[test]
fn test_resolve_tool_fail_closed() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    // 管辖但未暴露：模型手写也要拒绝。
    assert!(pi.governs("WebFetch") && !pi.is_exposed("WebFetch"));
    // 非管辖工具不受 fail-closed 限制。
    assert!(!pi.governs("mcp__slack__send"));
    // 已暴露的管辖工具可正常解析。
    assert!(pi.governs("Read") && pi.is_exposed("Read"));
}

#[test]
fn test_error_feedback_uses_dialect() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    assert_eq!(pi.echo_name("Grep"), "grep");
    let reason = format!("Invalid arguments for tool {}", pi.echo_name("Grep"));
    assert!(reason.contains("grep"));
    assert!(!reason.contains("Grep"));
    assert!(matches!(
        Kernel::cc().dialectize_hint("Read"),
        Cow::Borrowed(_)
    ));
}

#[test]
fn test_mismatch_hint_matches_dialect_params() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    // pi 的 `path` 能命中 read 条目；cc 的 `file_path` 条目匹配不到。
    let hint = pi
        .suggest_mismatch("Grep", &json!({"path": "a.rs"}))
        .unwrap();
    assert!(hint.contains("read"), "{hint}");
    assert!(hint.contains("path"), "{hint}");
    assert!(Kernel::cc()
        .suggest_mismatch("Grep", &json!({"path": "a.rs"}))
        .is_none());
}

#[test]
fn test_dialectize_messages_rewrites_history() {
    let pi = Kernel::from_config_checked("pi").unwrap();
    let mut messages = vec![BaseMessage::Ai {
        id: crate::messages::MessageId::new(),
        content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
            id: "c1".to_string(),
            name: "Read".to_string(),
            input: json!({"file_path": "a.rs"}),
        }]),
        tool_calls: vec![crate::messages::ToolCallRequest::new(
            "c1",
            "Read",
            json!({"file_path": "a.rs"}),
        )],
    }];
    pi.dialectize_messages(&mut messages);
    let BaseMessage::Ai {
        content,
        tool_calls,
        ..
    } = &messages[0]
    else {
        panic!("expected Ai");
    };
    assert!(
        matches!(&content.content_blocks()[0], ContentBlock::ToolUse { name, .. } if name == "read")
    );
    assert_eq!(tool_calls[0].name, "read");
    // 参数也要改成方言名，才能与当前 schema 一致。
    assert_eq!(tool_calls[0].arguments["path"], "a.rs");
    assert!(tool_calls[0].arguments.get("file_path").is_none());
}
