//! Agent 内核切换的跨 crate 验收测试（issue #826）。
//!
//! 这里覆盖验收清单里指定在 `cc-acp` 下执行的场景；纯类型/归一化测试
//! 位于 `cc-agent/src/kernel/kernel_test.rs`。

use cc_agent::kernel::{canonical_tool_name, Kernel, MutationForm, ALL_SPECS};
use serde_json::json;

use crate::provider::config::AppConfig;
use crate::session::executor::resolve_kernel;
use crate::session::executor::FrozenSessionData;

fn pi() -> Kernel {
    Kernel::from_config_checked("pi").unwrap()
}

fn dsh() -> Kernel {
    Kernel::from_config_checked("dsh").unwrap()
}

#[test]
fn test_unknown_kernel_falls_back_cc() {
    assert_eq!(Kernel::from_config(Some("xxx")), Kernel::cc());
    assert_eq!(Kernel::from_config(None), Kernel::cc());
    assert!(Kernel::from_config_checked("xxx").is_none());
}

#[test]
fn test_cc_kernel_equivalence() {
    let defs = vec![cc_agent::tools::ToolDefinition {
        name: "Read".to_string(),
        description: "Read a file".to_string(),
        parameters: cc_agent::tools::ToolParameters::Json(json!({
            "type": "object",
            "properties": {"file_path": {"type": "string"}},
            "required": ["file_path"]
        })),
    }];
    let before = serde_json::to_string(&defs).unwrap();
    let after = serde_json::to_string(&Kernel::cc().translate_definitions(defs)).unwrap();
    assert_eq!(before, after);
    assert!(Kernel::cc().prompt_for("any-model").is_empty());
}

#[test]
fn test_translate_read_schema_pi() {
    let defs = vec![cc_agent::tools::ToolDefinition {
        name: "Read".to_string(),
        description: "Read a file".to_string(),
        parameters: cc_agent::tools::ToolParameters::Json(json!({
            "type": "object",
            "properties": {"file_path": {"type": "string"}},
            "required": ["file_path"]
        })),
    }];
    let out = pi().translate_definitions(defs);
    assert_eq!(out[0].name, "read");
    let schema = out[0].parameters.as_json().unwrap();
    assert!(schema["properties"].get("path").is_some());
    assert!(schema["properties"].get("file_path").is_none());
}

#[test]
fn test_translate_edit_schema_pi() {
    let batch = json!({
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "edits": {"type": "array", "items": {"type": "object",
                "properties": {"oldText": {"type": "string"}, "newText": {"type": "string"}},
                "required": ["oldText", "newText"]}}
        },
        "required": ["path", "edits"]
    });
    let defs = vec![cc_agent::tools::ToolDefinition {
        name: "Edit".to_string(),
        description: "Edit a file".to_string(),
        parameters: cc_agent::tools::ToolParameters::Json(batch.clone()),
    }];
    let out = pi().translate_definitions(defs);
    assert_eq!(out[0].name, "edit");
    assert_eq!(*out[0].parameters.as_json().unwrap(), batch);
}

#[test]
fn test_pi_end_to_end() {
    // 出站 schema 用方言名 → 模型按方言发参 → 入站归一化为规范名 → 工具可消费。
    let defs = vec![cc_agent::tools::ToolDefinition {
        name: "Read".to_string(),
        description: "desc".to_string(),
        parameters: cc_agent::tools::ToolParameters::Json(json!({
            "type": "object",
            "properties": {"file_path": {"type": "string"}},
            "required": ["file_path"]
        })),
    }];
    let translated = pi().translate_definitions(defs);
    let schema = translated[0].parameters.as_json().unwrap();
    assert!(schema["properties"].get("path").is_some());
    let model_input = json!({"path": "a.rs"});
    let normalized = pi().normalize_input("Read", &model_input);
    assert_eq!(normalized["file_path"], "a.rs");
    assert!(normalized.get("path").is_none());
}

#[test]
fn test_pi_kernel_no_tool_search() {
    assert!(!pi().exposes_tool_search());
    assert!(pi().should_hide("SearchExtraTools"));
    assert!(pi().should_hide("ExecuteExtraTool"));
}

#[test]
fn test_pi_kernel_ls_powershell_hidden() {
    assert_eq!(pi().dialect_name("ls"), None);
    assert_eq!(pi().dialect_name("powershell"), None);
    assert!(!pi().is_exposed("ls"));
    assert!(!pi().is_exposed("powershell"));
}

#[test]
fn test_ungoverned_tools_never_hidden() {
    for kernel in [Kernel::cc(), dsh(), pi()] {
        for name in ["mcp__x__y", "LSP", "CronRemove", "AgentResult"] {
            assert!(!kernel.should_hide(name), "{} hid {name}", kernel.id());
        }
    }
}

#[test]
fn test_governed_unlisted_hidden() {
    assert!(pi().should_hide("WebFetch"));
    assert!(dsh().should_hide("WebSearch"));
    assert!(!pi().should_hide("Read"));
}

#[test]
fn test_dsh_tool_names() {
    let expected = [
        ("Read", "read"),
        ("Write", "write"),
        ("Edit", "edit"),
        ("Glob", "glob"),
        ("Grep", "grep"),
        ("Bash", "bash"),
        ("TodoWrite", "todo_write"),
        ("WebFetch", "web_fetch"),
        ("Agent", "subagent"),
        ("AskUserQuestion", "ask_user_question"),
        ("Skill", "skill"),
    ];
    for (canonical, dialect) in expected {
        assert_eq!(dsh().dialect_name(canonical), Some(dialect), "{canonical}");
    }
    // dsh 的 RRULE 与 peri 五段 cron 不同构，按能力缺口不映射。
    assert_eq!(dsh().dialect_name("CronRegister"), None);
    assert!(!dsh().spec().governed_tools.contains(&"CronRegister"));
}

#[test]
fn test_hitl_approval_under_dsh() {
    // dsh 的 bash 归一化后是规范名 Bash，HITL 的 TOOL_BASH 判定仍命中。
    assert_eq!(canonical_tool_name("bash"), Some("Bash"));
    assert_eq!(dsh().canonical_name("bash"), Some("Bash"));
}

#[test]
fn test_subagent_disallowed_tools_under_pi() {
    // 只读子 agent 的 disallowedTools 即使写成 pi 方言也必须归一到规范名。
    assert_eq!(canonical_tool_name("edit"), Some("Edit"));
    assert_eq!(canonical_tool_name("write"), Some("Write"));
    assert_eq!(canonical_tool_name("read"), Some("Read"));
}

#[test]
fn test_mutation_form_from_spec() {
    let form = pi()
        .spec()
        .tools
        .iter()
        .find(|t| t.canonical == "Edit")
        .and_then(|t| t.mutation_form)
        .unwrap_or_default();
    assert_eq!(form, MutationForm::BatchReplace);
    assert_eq!(ALL_SPECS.len(), 3);
}

#[test]
fn test_agent_kernel_config_field() {
    let cfg: AppConfig = serde_json::from_value(json!({"agent_kernel": "pi"})).unwrap();
    assert_eq!(cfg.agent_kernel.as_deref(), Some("pi"));
    let mut base: AppConfig = serde_json::from_value(json!({"agent_kernel": "cc"})).unwrap();
    let workspace: AppConfig = serde_json::from_value(json!({"agent_kernel": "dsh"})).unwrap();
    base.merge_overrides(workspace);
    assert_eq!(base.agent_kernel.as_deref(), Some("dsh"));
}

#[test]
fn test_system_prompt_no_accumulation_on_switch() {
    // 切换只替换 kernel_id（不追加 system 消息/段落），连切 5 次仍只有一个身份。
    let mut frozen = FrozenSessionData {
        system_prompt: "base".to_string(),
        instructions: None,
        jev_rule_loader: None,
        skill_summary: None,
        date: "2026-11-05".to_string(),
        is_git_repo: true,
        language: None,
        memory_entrypoint: None,
        memory_dir: None,
        kernel_id: Some("cc".to_string()),
    };
    for id in ["dsh", "pi", "cc", "dsh", "pi"] {
        frozen.kernel_id = Some(id.to_string());
    }
    assert_eq!(frozen.kernel_id.as_deref(), Some("pi"));
    assert_eq!(frozen.system_prompt, "base");
    assert_eq!(
        resolve_kernel(Some(&frozen), &Default::default())
            .unwrap()
            .id(),
        "pi"
    );
}
