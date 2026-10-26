use super::*;
use crate::messages::{MessageContent, ToolCallRequest};
use serde_json::json;

fn make_read_call(id: &str, path: &str) -> BaseMessage {
    BaseMessage::ai_with_tool_calls(
        MessageContent::text("reading file"),
        vec![ToolCallRequest::new(id, "Read", json!({"path": path}))],
    )
}

fn make_skill_call(id: &str, name: &str) -> BaseMessage {
    BaseMessage::ai_with_tool_calls(
        MessageContent::text("loading skill"),
        vec![ToolCallRequest::new(id, "Skill", json!({"skill": name}))],
    )
}

fn make_skill_output(name: &str, instruction: &str) -> serde_json::Value {
    json!({
        "success": true,
        "commandName": name,
        "status": "inline",
        "allowedTools": ["Read", "Bash"],
        "instruction": instruction
    })
}

fn make_skill_result(id: &str, name: &str, instruction: &str) -> BaseMessage {
    BaseMessage::tool_result(id, make_skill_output(name, instruction).to_string())
}

fn make_skill_message(name: &str, instruction: &str) -> BaseMessage {
    skill_invocation_message(&make_skill_output(name, instruction))
}

fn make_file(dir: &std::path::Path, name: &str, content: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn test_skill_invocation_payload_requires_complete_envelope() {
    let output = make_skill_output("review", "中文规则");
    let text = format_skill_invocation(&output);
    assert_eq!(
        skill_invocation_payload(&text),
        Some(output.to_string().as_str())
    );
    assert!(skill_invocation_payload(&format!("prefix{text}")).is_none());
    assert!(skill_invocation_payload(&format!("{text}suffix")).is_none());
    assert!(skill_invocation_payload(&output.to_string()).is_none());
}

#[test]
fn test_extract_recent_files_keeps_latest_unique_paths() {
    let messages = vec![
        make_read_call("first", "/a.rs"),
        make_read_call("second", "/b.rs"),
        make_read_call("third", "/a.rs"),
        make_read_call("fourth", "/c.rs"),
    ];
    let paths = extract_recent_files(&messages, 2);
    assert_eq!(paths, vec!["/c.rs", "/a.rs"]);
}

#[test]
fn test_extract_recent_files_does_not_infer_skill_from_path() {
    let messages = vec![
        make_read_call("first", "/project/.claude/skills/test/SKILL.md"),
        make_read_call("second", "/src/main.rs"),
        make_skill_call("third", "test"),
    ];
    let paths = extract_recent_files(&messages, 5);
    assert_eq!(
        paths,
        vec!["/src/main.rs", "/project/.claude/skills/test/SKILL.md"]
    );
}

#[test]
fn test_extract_recent_files_skips_other_message_types() {
    let messages = vec![
        BaseMessage::human("question"),
        make_read_call("first", "/a.rs"),
        BaseMessage::tool_result("first", "content"),
        BaseMessage::ai("done"),
    ];
    let paths = extract_recent_files(&messages, 5);
    assert_eq!(paths, vec!["/a.rs"]);
}

#[test]
fn test_extract_recent_files_honors_zero_limit() {
    let messages = vec![make_read_call("first", "/a.rs")];
    let paths = extract_recent_files(&messages, 0);
    assert!(paths.is_empty(), "文件上限为零时不得恢复文件");
}

#[test]
fn test_extract_active_skills_matches_successful_tool_results() {
    let messages = vec![
        BaseMessage::human("review"),
        make_skill_call("load", "review"),
        make_skill_result("load", "review", "完整审查规则"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].command_name, "review");
    assert_eq!(skills[0].instruction.as_deref(), Some("完整审查规则"));
    assert_eq!(
        skills[0].allowed_tools,
        Some(vec!["Read".into(), "Bash".into()])
    );
}

#[test]
fn test_extract_active_skills_ignores_failed_unmatched_and_pending_results() {
    let mut failed = make_skill_output("failed", "未加载");
    failed["success"] = json!(false);
    let messages = vec![
        make_skill_call("first", "failed"),
        BaseMessage::tool_result("first", failed.to_string()),
        make_skill_call("second", "error"),
        BaseMessage::tool_error("second", make_skill_output("error", "失败").to_string()),
        make_skill_call("third", "pending"),
        make_skill_result("unrelated", "other", "无对应调用"),
        make_read_call("fourth", "/tmp/SKILL.md"),
        make_skill_result("fourth", "read", "普通读取结果"),
    ];
    let skills = extract_active_skills(&messages);
    assert!(skills.is_empty(), "仅真实 Skill 的成功配对结果可恢复");
}

#[test]
fn test_extract_active_skills_rejects_malformed_or_incomplete_payloads() {
    let messages = vec![
        make_skill_call("first", "review"),
        BaseMessage::tool_result("first", "invalid JSON"),
        make_skill_call("third", "review"),
        BaseMessage::tool_result("third", r#"{"success":true,"commandName":"review"}"#),
    ];
    let skills = extract_active_skills(&messages);
    assert!(skills.is_empty());
}

#[test]
fn test_extract_active_skills_accepts_canonical_name_for_alias_call() {
    let messages = vec![
        make_skill_call("load", "/Review"),
        make_skill_result("load", "plugin:review", "按真实工具调用结果恢复规范名称"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].command_name, "plugin:review");
}

#[test]
fn test_extract_active_skills_keeps_latest_instruction_per_skill() {
    let messages = vec![
        make_skill_call("first", "review"),
        make_skill_result("first", "review", "旧规则"),
        make_skill_call("second", "other"),
        make_skill_result("second", "other", "其他规则"),
        make_skill_call("third", "review"),
        make_skill_result("third", "review", "新参数对应的新规则"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].command_name, "other");
    assert_eq!(skills[1].instruction.as_deref(), Some("新参数对应的新规则"));
}

#[test]
fn test_extract_active_skills_preserves_invocations_across_user_rounds() {
    let messages = vec![
        BaseMessage::human("old task"),
        make_skill_call("first", "old"),
        make_skill_result("first", "old", "旧任务规则"),
        BaseMessage::ai("done"),
        BaseMessage::human("new task"),
        make_skill_call("second", "new"),
        make_skill_result("second", "new", "当前任务规则"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].command_name, "old");
    assert_eq!(skills[1].command_name, "new");
}

#[test]
fn test_extract_active_skills_preserves_skill_without_current_activation() {
    let messages = vec![
        make_skill_message("old", "旧任务规则"),
        BaseMessage::human("new task without a skill"),
        BaseMessage::ai("done"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 1, "后续用户轮次不应丢失已调用技能");
    assert_eq!(skills[0].command_name, "old");
}

#[test]
fn test_extract_active_skills_does_not_treat_request_as_new_user_round() {
    let messages = vec![
        BaseMessage::human("/review"),
        make_skill_call("first", "first"),
        make_skill_result("first", "first", "第一项规则"),
        BaseMessage::human(
            "<skill-request>\n[{\"skill\":\"review\"}]\n</skill-request>\nInvoke Skill first.",
        ),
        make_skill_call("second", "review"),
        make_skill_result("second", "review", "审查规则"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 2);
}

#[test]
fn test_extract_active_skills_excludes_fork_results() {
    let mut forked = make_skill_output("review", "隔离执行摘要");
    forked["status"] = json!("forked");
    let messages = vec![
        make_skill_call("first", "review"),
        make_skill_result("first", "review", "主上下文旧规则"),
        make_skill_call("second", "review"),
        BaseMessage::tool_result("second", forked.to_string()),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 1, "forked 摘要不得覆盖原有主上下文技能指令");
    assert_eq!(skills[0].instruction.as_deref(), Some("主上下文旧规则"));
}

#[test]
fn test_extract_active_skills_restores_metadata_without_tool_messages() {
    let messages = vec![
        BaseMessage::human("<system-reminder>summary</system-reminder>"),
        make_skill_message("review", "压缩前完整规则"),
    ];
    let skills = extract_active_skills(&messages);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].instruction.as_deref(), Some("压缩前完整规则"));
}

#[test]
fn test_invoked_skill_registry_keeps_latest_inline_content_and_order() {
    let registry = InvokedSkillRegistry::default();
    assert!(registry.record(&make_skill_output("review", "旧规则")));
    assert!(registry.record(&make_skill_output("other", "其他规则")));
    assert!(registry.record(&make_skill_output("review", "新参数对应的新规则")));
    let skills = registry.snapshot();
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].command_name, "other");
    assert_eq!(skills[1].command_name, "review");
    assert_eq!(skills[1].instruction.as_deref(), Some("新参数对应的新规则"));
}

#[test]
fn test_invoked_skill_registry_rejects_failed_forked_and_restored_payloads() {
    let registry = InvokedSkillRegistry::default();
    assert!(registry.record(&make_skill_output("review", "完整规则")));
    let mut failed = make_skill_output("review", "失败内容");
    failed["success"] = json!(false);
    let mut forked = make_skill_output("review", "子代理内容");
    forked["status"] = json!("forked");
    let mut restored = make_skill_output("review", "截短内容");
    restored["restored"] = json!(true);
    assert!(!registry.record(&failed));
    assert!(!registry.record(&forked));
    assert!(!registry.record(&restored));
    assert!(!registry.record(&json!({"success": true})));
    assert_eq!(registry.snapshot().len(), 1);
    assert_eq!(
        registry.snapshot()[0].instruction.as_deref(),
        Some("完整规则"),
        "失败、forked 和 compact 短载荷均不得覆盖完整正文"
    );
}

#[test]
fn test_invoked_skill_registry_seeds_empty_history_only_once() {
    let registry = InvokedSkillRegistry::default();
    registry.seed_from_messages(&[]);
    registry.seed_from_messages(&[make_skill_message("parent", "父代理规则")]);
    assert!(
        registry.snapshot().is_empty(),
        "空历史初始化后，不应在后续轮次混入父代理历史"
    );
}

#[test]
fn test_invoked_skill_registry_shares_clones_and_isolates_new_agents() {
    let registry = InvokedSkillRegistry::default();
    let same_agent = registry.clone();
    let child_agent = InvokedSkillRegistry::default();
    registry.record(&make_skill_output("review", "当前代理规则"));
    assert_eq!(same_agent.snapshot().len(), 1);
    assert!(
        child_agent.snapshot().is_empty(),
        "新代理不得继承父代理 registry"
    );
    same_agent.record(&make_skill_output("other", "同一代理新增规则"));
    assert_eq!(registry.snapshot().len(), 2);
}

#[test]
fn test_invoked_skill_registry_resets_from_retained_history() {
    let registry = InvokedSkillRegistry::default();
    registry.record(&make_skill_output("review", "回滚后不应保留的新规则"));
    registry.record(&make_skill_output("later", "回滚后应移除的技能"));
    registry.reset_from_messages(&[make_skill_message("review", "保留历史中的规则")]);
    assert_eq!(registry.snapshot().len(), 1);
    assert_eq!(
        registry.snapshot()[0].instruction.as_deref(),
        Some("保留历史中的规则")
    );
    registry.reset_from_messages(&[]);
    registry.seed_from_messages(&[make_skill_message("review", "已清除的旧历史")]);
    assert!(registry.snapshot().is_empty(), "clear 后不得重新扫描旧历史");
}

#[test]
fn test_invoked_skill_registry_seeds_restored_history_on_session_resume() {
    let registry = InvokedSkillRegistry::default();
    let mut output = make_skill_output("review", "持久化历史中仍可恢复的规则");
    output["restored"] = json!(true);
    registry.seed_from_messages(&[skill_invocation_message(&output)]);
    assert_eq!(registry.snapshot().len(), 1);
    assert_eq!(
        registry.snapshot()[0].instruction.as_deref(),
        Some("持久化历史中仍可恢复的规则")
    );
}

#[test]
fn test_truncate_to_budget_keeps_whole_files() {
    let mut contents = vec![
        ("first".into(), "x".repeat(8000)),
        ("second".into(), "x".repeat(8000)),
        ("third".into(), "x".repeat(8000)),
    ];
    let count = truncate_to_budget(&mut contents, 5000);
    assert_eq!(count, 2);
    assert_eq!(contents.len(), 2);
}

#[test]
fn test_truncate_to_budget_keeps_exact_boundary() {
    let mut contents = vec![("first".into(), "x".repeat(4000))];
    let count = truncate_to_budget(&mut contents, 1000);
    assert_eq!(count, 1);
}

#[test]
fn test_truncate_to_budget_rejects_oversized_file() {
    let mut contents = vec![("first".into(), "x".repeat(10000))];
    let count = truncate_to_budget(&mut contents, 100);
    assert_eq!(count, 0);
    assert!(contents.is_empty());
}

#[tokio::test]
async fn test_read_file_with_budget_reads_small_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = make_file(directory.path(), "test.txt", "hello world");
    let result = read_file_with_budget(&path, 100).await;
    assert_eq!(result, Some("hello world".to_string()));
}

#[tokio::test]
async fn test_read_file_with_budget_truncates_by_character() {
    let directory = tempfile::tempdir().unwrap();
    let path = make_file(directory.path(), "test.txt", &"中文".repeat(100));
    let result = read_file_with_budget(&path, 10).await;
    assert_eq!(result, Some(format!("{}...(已截断)", "中文".repeat(20))));
}

#[tokio::test]
async fn test_read_file_with_budget_skips_missing_file() {
    let directory = tempfile::tempdir().unwrap();
    let result =
        read_file_with_budget(&directory.path().join("missing").to_string_lossy(), 100).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_re_inject_reads_files_and_restores_skill_instructions() {
    let directory = tempfile::tempdir().unwrap();
    let path = make_file(directory.path(), "main.rs", "fn main() {}");
    let messages = vec![
        BaseMessage::human("review"),
        make_read_call("read", &path),
        make_skill_call("skill", "review"),
        make_skill_result("skill", "review", "完整审查规则"),
    ];
    let result = re_inject(&messages, &CompactConfig::default(), "").await;
    assert_eq!(result.files_injected, 1);
    assert_eq!(result.skills_injected, 1);
    assert_eq!(result.messages.len(), 2);
    assert!(result.messages[0].is_system());
    assert!(matches!(result.messages[1], BaseMessage::Human { .. }));
    assert!(result.messages[1].content().contains("完整审查规则"));
    assert!(result
        .messages
        .iter()
        .all(|message| !message.has_tool_calls()));
}

#[tokio::test]
async fn test_re_inject_truncates_skill_body_by_character_budget() {
    let instruction = "长技能规则\n".repeat(100);
    let messages = vec![
        make_skill_call("load", "review"),
        make_skill_result("load", "review", &instruction),
    ];
    let config = CompactConfig {
        re_inject_max_tokens_per_file: 10,
        ..Default::default()
    };
    let result = re_inject(&messages, &config, "").await;
    assert_eq!(result.skills_injected, 1);
    let skills = extract_active_skills(&result.messages);
    assert_eq!(
        skills[0].instruction.as_deref(),
        Some(instruction.chars().take(40).collect::<String>().as_str())
    );
}

#[tokio::test]
async fn test_re_inject_keeps_complete_skills_within_total_budget() {
    let messages = vec![
        make_skill_call("first", "large"),
        make_skill_result("first", "large", &"规则".repeat(500)),
        make_skill_call("second", "small"),
        make_skill_result("second", "small", "完整小技能"),
    ];
    let config = CompactConfig {
        re_inject_skills_budget: 100,
        ..Default::default()
    };
    let result = re_inject(&messages, &config, "").await;
    assert_eq!(result.skills_injected, 1);
    let skills = extract_active_skills(&result.messages);
    assert_eq!(skills[0].command_name, "small");
    assert_eq!(skills[0].instruction.as_deref(), Some("完整小技能"));
}

#[tokio::test]
async fn test_re_inject_marks_skill_truncation_within_budget() {
    let instruction = "中文规则\n".repeat(5000);
    let messages = vec![
        make_skill_call("load", "review"),
        make_skill_result("load", "review", &instruction),
    ];
    let result = re_inject(&messages, &CompactConfig::default(), "").await;
    let skills = extract_active_skills(&result.messages);
    let restored = skills[0].instruction.as_deref().expect("应恢复技能正文");
    assert_eq!(restored.chars().count(), 20_000);
    assert!(restored.ends_with(SKILL_TRUNCATION_MARKER));
    assert!(restored.starts_with("中文规则\n"));
}

#[tokio::test]
async fn test_re_inject_preserves_skill_across_repeated_compaction() {
    let original = vec![
        BaseMessage::human("review"),
        make_skill_call("load", "review"),
        make_skill_result("load", "review", "保留参数插值后的规则"),
    ];
    let config = CompactConfig::default();
    let first = re_inject(&original, &config, "").await;
    let compacted = build_compacted_messages("summary", first.messages);
    let second = re_inject(&compacted, &config, "").await;
    assert_eq!(second.skills_injected, 1);
    assert_eq!(extract_skill_names(&second.messages), vec!["review"]);
    assert_eq!(
        extract_active_skills(&second.messages)[0]
            .instruction
            .as_deref(),
        Some("保留参数插值后的规则")
    );
    assert!(second
        .messages
        .iter()
        .all(|message| matches!(message, BaseMessage::Human { .. })));
}

#[tokio::test]
async fn test_re_inject_prioritizes_recent_skills_across_repeated_compaction() {
    let original = vec![
        make_skill_call("first", "older"),
        make_skill_result("first", "older", "12345678"),
        make_skill_call("second", "newer"),
        make_skill_result("second", "newer", "abcdefgh"),
    ];
    let default_config = CompactConfig::default();
    let first = re_inject(&original, &default_config, "").await;
    let compacted = build_compacted_messages("summary", first.messages);
    let narrow_config = CompactConfig {
        re_inject_skills_budget: 2,
        ..Default::default()
    };
    let second = re_inject(&compacted, &narrow_config, "").await;
    assert_eq!(second.skills_injected, 1);
    assert_eq!(extract_skill_names(&second.messages), vec!["newer"]);
}

#[tokio::test]
async fn test_re_inject_with_registry_retains_complete_content_after_truncation() {
    let registry = InvokedSkillRegistry::default();
    let instruction = "中文完整规则\n".repeat(100);
    let mut output = make_skill_output("review", &instruction);
    output["model"] = json!("sonnet");
    output["display"] = json!(true);
    registry.record(&output);
    let config = CompactConfig {
        re_inject_max_tokens_per_file: 10,
        ..Default::default()
    };
    let first = re_inject_with_registry(&[], &config, "", &registry).await;
    let first_payload: serde_json::Value = serde_json::from_str(
        skill_invocation_payload(&first.messages[0].content()).expect("应包含完整元消息外壳"),
    )
    .expect("恢复载荷应为合法 JSON");
    assert_eq!(
        first_payload["instruction"]
            .as_str()
            .map(|value| value.chars().count()),
        Some(40)
    );
    assert_eq!(first_payload["restored"], json!(true));
    assert!(
        first_payload.get("display").is_none(),
        "恢复不应重复显示工具卡片"
    );
    assert!(!registry.record(&first_payload));
    let compacted = build_compacted_messages("summary", first.messages);
    let second =
        re_inject_with_registry(&compacted, &CompactConfig::default(), "", &registry).await;
    let skills = extract_active_skills(&second.messages);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].instruction.as_deref(), Some(instruction.as_str()));
    assert_eq!(skills[0].model.as_deref(), Some("sonnet"));
    assert_eq!(
        skills[0].allowed_tools,
        Some(vec!["Read".into(), "Bash".into()])
    );
}

#[tokio::test]
async fn test_re_inject_with_registry_recovers_skills_dropped_by_previous_budget() {
    let registry = InvokedSkillRegistry::default();
    registry.record(&make_skill_output("older", "12345678"));
    registry.record(&make_skill_output("newer", "abcdefgh"));
    let narrow_config = CompactConfig {
        re_inject_skills_budget: 2,
        ..Default::default()
    };
    let first = re_inject_with_registry(&[], &narrow_config, "", &registry).await;
    assert_eq!(extract_skill_names(&first.messages), vec!["newer"]);
    let compacted = build_compacted_messages("summary", first.messages);
    let second =
        re_inject_with_registry(&compacted, &CompactConfig::default(), "", &registry).await;
    assert_eq!(
        extract_skill_names(&second.messages),
        vec!["older", "newer"]
    );
    assert_eq!(
        extract_active_skills(&second.messages)[0]
            .instruction
            .as_deref(),
        Some("12345678"),
        "前一次预算省略的技能仍应从独立 registry 恢复"
    );
}

#[tokio::test]
async fn test_re_inject_empty_messages_produces_no_recovery() {
    let result = re_inject(&[], &CompactConfig::default(), "").await;
    assert_eq!(result.files_injected, 0);
    assert_eq!(result.skills_injected, 0);
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn test_re_inject_skips_unreadable_file() {
    let directory = tempfile::tempdir().unwrap();
    let messages = vec![make_read_call("read", "missing.rs")];
    let result = re_inject(
        &messages,
        &CompactConfig::default(),
        &directory.path().to_string_lossy(),
    )
    .await;
    assert_eq!(result.files_injected, 0);
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn test_re_inject_resolves_relative_file_paths() {
    let directory = tempfile::tempdir().unwrap();
    make_file(directory.path(), "main.rs", "fn main() {}");
    let messages = vec![make_read_call("read", "main.rs")];
    let result = re_inject(
        &messages,
        &CompactConfig::default(),
        &directory.path().to_string_lossy(),
    )
    .await;
    assert_eq!(result.files_injected, 1);
    assert!(result.messages[0].content().contains("fn main() {}"));
}

#[tokio::test]
async fn test_re_inject_respects_file_limits() {
    let directory = tempfile::tempdir().unwrap();
    let messages: Vec<_> = (0..5)
        .map(|index| {
            let path = make_file(
                directory.path(),
                &format!("file{index}.rs"),
                &"x".repeat(8000),
            );
            make_read_call(&format!("read{index}"), &path)
        })
        .collect();
    let config = CompactConfig {
        re_inject_file_budget: 5000,
        re_inject_max_files: 3,
        ..Default::default()
    };
    let result = re_inject(&messages, &config, "").await;
    assert_eq!(result.files_injected, 2);
}

#[test]
fn test_extract_skill_names_reads_metadata_and_legacy_display_labels() {
    let messages = vec![
        make_skill_message("review", "规则"),
        BaseMessage::system("[激活的 Skill 指令: legacy\n旧显示格式"),
        BaseMessage::system("[最近读取的文件: main.rs]\n代码"),
    ];
    let names = extract_skill_names(&messages);
    assert_eq!(names, vec!["review", "legacy"]);
}

#[test]
fn test_build_compacted_messages_wraps_summary_in_system_reminder() {
    let summary = "用户讨论了 compact 修复";
    let messages = build_compacted_messages(summary, vec![]);
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], BaseMessage::Human { .. }));
    assert!(messages[0].content().starts_with("<system-reminder>\n"));
    assert!(messages[0].content().ends_with("\n</system-reminder>"));
    assert!(messages[0].content().contains(summary));
    assert!(messages[0]
        .content()
        .contains("[Context compacted; continue based on the summary]"));
}

#[test]
fn test_build_compacted_messages_places_recovery_after_summary() {
    let recovery = vec![
        BaseMessage::system("[最近读取的文件: main.rs]\nfn main() {}"),
        make_skill_message("review", "完整规则"),
    ];
    let messages = build_compacted_messages("summary", recovery);
    assert_eq!(messages.len(), 3);
    assert!(matches!(messages[0], BaseMessage::Human { .. }));
    assert!(messages[1].is_system());
    assert!(matches!(messages[2], BaseMessage::Human { .. }));
    assert_eq!(extract_skill_names(&messages), vec!["review"]);
}
