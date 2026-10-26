use super::*;
use cc_agent::tools::BaseTool;
use serde_json::json;
use std::path::Path;
use tempfile::tempdir;

fn make_skill(directory: &Path, name: &str, frontmatter: &str, body: &str) {
    let directory = directory.join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("SKILL.md"), format!("---\nname: {name}\n{frontmatter}\n---\n{body}")).unwrap();
}

#[tokio::test]
async fn test_skill_returns_complete_instruction_and_camel_case_metadata() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "audit", "allowed-tools: [Read, Grep]\nmodel: haiku", "检查 $ARGUMENTS 和 ${CLAUDE_SESSION_ID}");
    let tool = SkillTool::new(vec![directory.path().to_path_buf()]).with_session_id(Some("session-test".into()));
    let response = tool.invoke_content(json!({"skill":"audit", "args":"中文项目"})).await.expect("技能应成功返回 ToolContent");
    let output: serde_json::Value = serde_json::from_str(&response.output).expect("输出应为有效 JSON");
    assert_eq!(output["success"], true);
    assert_eq!(output["commandName"], "audit");
    assert_eq!(output["status"], "inline");
    assert_eq!(output["allowedTools"], json!(["Read", "Grep"]));
    assert_eq!(output["model"], "haiku");
    assert!(output["instruction"].as_str().expect("应有完整指令").contains("检查 中文项目 和 session-test"));
    assert!(output.get("command_name").is_none());
    assert!(output.get("agentId").is_none());
}

#[tokio::test]
async fn test_skill_invalid_requests_return_errors() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "audit", "", "正文");
    let tool = SkillTool::new(vec![directory.path().to_path_buf()]);
    for input in [json!({}), json!({"skill": 1}), json!({"skill":""}), json!({"skill":"../audit"}), json!({"skill":"missing"}), json!({"skill":"audit", "args":42})] {
        assert!(tool.invoke(input.clone()).await.is_err(), "无效输入必须返回 Err：{input}");
    }
}

#[tokio::test]
async fn test_skill_disabled_model_invocation_allows_explicit_user_loading() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "manual", "disable-model-invocation: true", "显式工作流");
    let tool = SkillTool::new(vec![directory.path().to_path_buf()]);
    assert!(tool.invoke(json!({"skill":"manual"})).await.is_err());
    let output = tool.invoke_for_user(SkillToolInput {skill: "manual".into(), args: None}).await.expect("用户显式调用应允许加载");
    assert!(output.instruction.expect("应有指令").contains("显式工作流"));
}

#[tokio::test]
async fn test_skill_hidden_user_invocation_remains_available_to_model() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "internal", "user-invocable: false", "内部知识");
    let tool = SkillTool::new(vec![directory.path().to_path_buf()]);
    assert!(tool.invoke(json!({"skill":"internal"})).await.is_ok());
    assert!(tool.invoke_for_user(SkillToolInput {skill: "internal".into(), args: None}).await.is_err());
}

#[tokio::test]
async fn test_skill_fork_without_runner_does_not_fall_back_to_inline() {
    let directory = tempdir().unwrap();
    make_skill(directory.path(), "isolated", "context: fork", "独立执行");
    let tool = SkillTool::new(vec![directory.path().to_path_buf()]);
    let result = tool.invoke(json!({"skill":"isolated"})).await;
    assert!(result.is_err(), "缺少隔离执行上下文时不能污染主对话");
}

#[test]
fn test_skill_schema_is_core_and_does_not_create_file_snapshots() {
    let tool = SkillTool::new(Vec::new());
    assert_eq!(tool.name(), "Skill");
    assert_eq!(tool.parameters()["required"], json!(["skill"]));
    assert!(tool.parameters()["properties"]["skill"]["description"].as_str().expect("应有参数说明").contains("REQUIRED"));
    assert!(!crate::tool_search::core_tools::is_deferred_tool(tool.name()));
    assert!(tool.file_snapshot(&json!({"skill":"audit"})).is_none());
}
