use super::*;
use cc_agent::agent::state::AgentState;
use serde_json::json;
use std::path::Path;

fn make_skill(directory: &Path, name: &str, body: &str, extra: &str) {
    let skill_directory = directory.join(name.replace(':', "-"));
    std::fs::create_dir_all(&skill_directory).unwrap();
    let content = format!("---\nname: '{name}'\ndescription: '测试技能'\n{extra}---\n\n{body}\n");
    std::fs::write(skill_directory.join("SKILL.md"), content).unwrap();
}

fn make_middleware(names: &[&str], directory: &Path) -> SkillPreloadMiddleware {
    SkillPreloadMiddleware::new(
        names.iter().map(|name| name.to_string()).collect(),
        &directory.to_string_lossy(),
    )
    .with_skill_tool(SkillTool::new(vec![directory.to_path_buf()]))
}

fn make_state(directory: &Path, input: Option<&str>) -> AgentState {
    let mut state = AgentState::new(directory.to_string_lossy().as_ref());
    if let Some(input) = input {
        state.add_message(BaseMessage::human(input));
    }
    state
}

fn make_payload(message: &BaseMessage) -> serde_json::Value {
    serde_json::from_str(skill_invocation_payload(&message.content()).unwrap()).unwrap()
}

#[test]
fn test_skill_name_from_token_handles_namespace_and_multiple_tokens() {
    assert_eq!(skill_name_from_token("/plugin:review"), Some("plugin:review"));
    assert_eq!(skill_name_from_token("/fix-issue"), Some("fix-issue"));
    assert_eq!(skill_name_from_token("/api.guide"), Some("api.guide"));
}

#[test]
fn test_skill_name_from_token_rejects_paths_and_incomplete_tokens() {
    assert!(skill_name_from_token("/").is_none(), "空 token 不应被当作技能");
    assert!(skill_name_from_token("/foo/bar").is_none(), "目录路径不应被当作技能");
    assert!(skill_name_from_token("/review?").is_none(), "不完整指令不应被当作技能");
    assert!(skill_name_from_token("https://example.com").is_none(), "网址不应被当作技能");
}

#[tokio::test]
async fn test_preload_without_user_input_or_explicit_names_is_noop() {
    let directory = tempfile::tempdir().unwrap();
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), None);
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert!(state.messages().is_empty());
}

#[tokio::test]
async fn test_preload_explicit_skill_adds_only_human_metadata() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "review", "完整审查规则", "");
    let middleware = make_middleware(&["review"], directory.path());
    let mut state = make_state(directory.path(), None);
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 1);
    assert!(matches!(state.messages()[0], BaseMessage::Human { .. }));
    assert!(!state.messages()[0].has_tool_calls());
    let payload = make_payload(&state.messages()[0]);
    assert_eq!(payload["commandName"], "review");
    assert_eq!(payload["success"], true);
    assert_eq!(payload["display"], true);
    assert!(payload["instruction"]
        .as_str()
        .is_some_and(|body| body.contains("完整审查规则")));
}

#[tokio::test]
async fn test_preload_preserves_user_message_and_appends_in_requested_order() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "first", "第一项", "");
    make_skill(directory.path(), "second", "第二项", "");
    let middleware = make_middleware(&["second", "first"], directory.path());
    let mut state = make_state(directory.path(), Some("执行预配置技能"));
    let original_id = state.messages()[0].id();
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 3);
    assert_eq!(state.messages()[0].id(), original_id);
    assert_eq!(make_payload(&state.messages()[1])["commandName"], "second");
    assert_eq!(make_payload(&state.messages()[2])["commandName"], "first");
    assert!(state
        .messages()
        .iter()
        .all(|message| matches!(message, BaseMessage::Human { .. })));
}

#[tokio::test]
async fn test_preload_slash_preserves_quoted_unicode_arguments() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "review", "参数：$ARGUMENTS", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/review 中文  \"two words\" --flag"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2);
    let payload = make_payload(&state.messages()[1]);
    assert_eq!(payload["args"], "中文  \"two words\" --flag");
    assert!(payload["instruction"]
        .as_str()
        .is_some_and(|body| body.contains("中文  \"two words\" --flag")));
}

#[tokio::test]
async fn test_preload_multiple_slashes_split_arguments_at_known_skill_tokens() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "first", "$ARGUMENTS", "");
    make_skill(directory.path(), "second", "$ARGUMENTS", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/first one two /second 三 四"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 3);
    assert_eq!(make_payload(&state.messages()[1])["args"], "one two");
    assert_eq!(make_payload(&state.messages()[2])["args"], "三 四");
}

#[tokio::test]
async fn test_preload_quoted_skill_token_remains_an_argument() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "first", "$ARGUMENTS", "");
    make_skill(directory.path(), "second", "不应加载", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/first \"literal /second value\""));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2, "引号中的技能 token 只是参数");
    assert_eq!(
        make_payload(&state.messages()[1])["args"],
        "\"literal /second value\""
    );
}

#[tokio::test]
async fn test_preload_slash_in_sentence_remains_supported() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "review", "$ARGUMENTS", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("帮我用 /review 调试这个问题"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2);
    assert_eq!(make_payload(&state.messages()[1])["args"], "调试这个问题");
}

#[tokio::test]
async fn test_preload_skips_unknown_skills_builtin_commands_and_paths() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "review", "审查规则", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/help /unknown /review/SKILL.md"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 1, "未匹配可用技能不得加载");
}

#[tokio::test]
async fn test_preload_explicit_list_skips_missing_names() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "exists", "存在技能", "");
    let middleware = make_middleware(&["unknown", "exists"], directory.path());
    let mut state = make_state(directory.path(), None);
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 1);
    assert_eq!(make_payload(&state.messages()[0])["commandName"], "exists");
}

#[tokio::test]
async fn test_preload_prefers_exact_namespace_over_basename() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "first:review", "第一插件", "");
    make_skill(directory.path(), "second:review", "第二插件", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/FIRST:REVIEW"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2);
    assert_eq!(
        make_payload(&state.messages()[1])["commandName"],
        "first:review"
    );
}

#[tokio::test]
async fn test_preload_accepts_unique_unqualified_basename() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "plugin:review", "唯一插件", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/review"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2);
    assert_eq!(
        make_payload(&state.messages()[1])["commandName"],
        "plugin:review"
    );
}

#[tokio::test]
async fn test_preload_rejects_ambiguous_or_different_namespace() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "first:review", "第一插件", "");
    make_skill(directory.path(), "second:review", "第二插件", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/review /third:review"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 1, "不能误选另一个插件的同名技能");
}

#[tokio::test]
async fn test_preload_user_can_invoke_model_disabled_skill() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(
        directory.path(),
        "manual",
        "仅用户主动加载",
        "disable-model-invocation: true\n",
    );
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/manual"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 2);
    assert_eq!(make_payload(&state.messages()[1])["commandName"], "manual");
}

#[tokio::test]
async fn test_preload_is_idempotent_and_does_not_parse_skill_body() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(
        directory.path(),
        "review",
        "正文提及 /other 不应自动加载",
        "",
    );
    make_skill(directory.path(), "other", "另一个技能", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/review"));
    let first = middleware.before_agent(&mut state).await;
    let second = middleware.before_agent(&mut state).await;
    assert!(first.is_ok());
    assert!(second.is_ok());
    assert_eq!(
        state.messages().len(),
        2,
        "同一轮不重复加载，也不扫描技能正文触发技能"
    );
    assert_eq!(make_payload(&state.messages()[1])["commandName"], "review");
}

#[tokio::test]
async fn test_preload_new_user_turn_can_reload_same_skill() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "review", "审查规则", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), Some("/review"));
    let first = middleware.before_agent(&mut state).await;
    state.add_message(BaseMessage::ai("done"));
    state.add_message(BaseMessage::human("/review"));
    let second = middleware.before_agent(&mut state).await;
    assert!(first.is_ok());
    assert!(second.is_ok());
    assert_eq!(state.messages().len(), 5, "新用户轮次的显式调用应重新加载");
}

#[tokio::test]
async fn test_preload_restored_metadata_is_not_user_input() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "other", "不应自动加载", "");
    let middleware = make_middleware(&[], directory.path());
    let mut state = make_state(directory.path(), None);
    state.add_message(skill_invocation_message(&json!({
        "success": true,
        "commandName": "review",
        "status": "inline",
        "instruction": "/other"
    })));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages().len(), 1);
}

#[tokio::test]
async fn test_preload_loading_failure_returns_error_without_partial_messages() {
    let directory = tempfile::tempdir().unwrap();
    make_skill(directory.path(), "inline", "内联规则", "");
    make_skill(directory.path(), "forked", "隔离任务", "context: fork\n");
    let middleware = make_middleware(&["inline", "forked"], directory.path());
    let mut state = make_state(directory.path(), Some("执行技能"));
    let result = middleware.before_agent(&mut state).await;
    assert!(result.is_err(), "没有 fork 执行上下文时必须返回真实错误");
    assert_eq!(state.messages().len(), 1, "加载失败不得留下部分注入消息");
    assert_eq!(state.messages()[0].content(), "执行技能");
}
