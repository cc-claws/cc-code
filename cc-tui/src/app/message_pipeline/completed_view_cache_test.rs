use super::*;
use cc_agent::messages::{BaseMessage, ToolCallRequest};
use serde_json::json;

use super::super::{AgentEvent, HookExec};

fn make_tool_history(
    id: &str,
    name: &str,
    input: serde_json::Value,
    output: impl Into<String>,
) -> Vec<BaseMessage> {
    vec![
        BaseMessage::ai_with_tool_calls("", vec![ToolCallRequest::new(id, name, input)]),
        BaseMessage::tool_result(id, output.into()),
    ]
}

fn make_edit_history(path: &std::path::Path) -> Vec<BaseMessage> {
    make_tool_history(
        "edit",
        "Edit",
        json!({"file_path":path.to_string_lossy(),"old_string":"旧值","new_string":"新值"}),
        "已修改",
    )
}

fn make_diff_input(messages: &[MessageViewModel]) -> cc_widgets::DiffInput {
    messages
        .iter()
        .find_map(|message| match message {
            MessageViewModel::ToolBlock {
                diff_input: Some(input),
                ..
            } => Some(input.clone()),
            _ => None,
        })
        .expect("应保留 Edit 的差异输入")
}

#[test]
fn test_completed_view_cache_reuses_edit_without_reopening_target() {
    let directory = tempfile::tempdir().expect("应创建临时目录");
    let path = directory.path().join("中文文件.rs");
    std::fs::write(&path, "前文\n新值\n后文\n").expect("应创建差异目标");
    let mut pipeline = MessagePipeline::new(directory.path().to_string_lossy().into_owned());
    pipeline.set_completed(make_edit_history(&path));
    let first = make_diff_input(&pipeline.build_tail_vms());
    std::fs::remove_file(&path).expect("应移除临时目标以检测重复读盘");
    pipeline.handle_event(AgentEvent::AiReasoning("继续检查中文路径".into()));
    let repeated = make_diff_input(&pipeline.build_tail_vms());
    assert_eq!(first.new_content, "前文\n新值\n后文\n");
    assert_eq!(
        repeated.old_content, first.old_content,
        "历史转换应复用旧全文"
    );
    assert_eq!(
        repeated.new_content, first.new_content,
        "流式事件不应重新读取文件"
    );
}

#[test]
fn test_completed_view_cache_invalidates_after_snapshot_append() {
    let directory = tempfile::tempdir().expect("应创建临时目录");
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, "前文\n新值\n后文\n").expect("应创建差异目标");
    let mut pipeline = MessagePipeline::new(directory.path().to_string_lossy().into_owned());
    pipeline.set_completed(make_edit_history(&path));
    let first = make_diff_input(&pipeline.build_tail_vms());
    std::fs::remove_file(&path).expect("应移除临时目标");
    pipeline.set_completed(vec![BaseMessage::ai("已追加新快照")]);
    let messages = pipeline.build_tail_vms();
    let refreshed = make_diff_input(&messages);
    assert_eq!(first.new_content, "前文\n新值\n后文\n");
    assert_eq!(refreshed.new_content, "新值", "新快照应使旧转换失效");
    assert!(messages.iter().any(|message| matches!(
        message,
        MessageViewModel::AssistantBubble { blocks, .. }
            if blocks.iter().any(|block| matches!(block, crate::ui::message_view::ContentBlockView::Text { raw, .. } if raw == "已追加新快照"))
    )));
}

#[test]
fn test_completed_view_cache_invalidates_after_same_length_history_replacement() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(vec![BaseMessage::human("旧消息")]);
    let original = pipeline.cached_completed_view_models(0);
    let original_len = pipeline.completed_messages().len();
    pipeline.restore_completed(vec![BaseMessage::human("替换后的中文消息")]);
    let replaced = pipeline.cached_completed_view_models(0);
    assert_eq!(pipeline.completed_messages().len(), original_len);
    assert_ne!(replaced, original, "相同长度的替换不能复用旧历史");
    assert!(
        matches!(&replaced[0], MessageViewModel::UserBubble { content, .. } if content == "替换后的中文消息")
    );
}

#[test]
fn test_completed_view_cache_invalidates_after_theme_fingerprint_change() {
    let directory = tempfile::tempdir().expect("应创建临时目录");
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, "前文\n新值\n后文\n").expect("应创建差异目标");
    let mut pipeline = MessagePipeline::new(directory.path().to_string_lossy().into_owned());
    pipeline.set_completed(make_edit_history(&path));
    let first = make_diff_input(&pipeline.build_tail_vms());
    pipeline
        .completed_view_cache
        .get_mut()
        .as_mut()
        .expect("应已缓存历史转换")
        .theme = theme::fingerprint().wrapping_add(1);
    std::fs::remove_file(&path).expect("应移除临时目标");
    let refreshed = make_diff_input(&pipeline.build_tail_vms());
    assert_eq!(first.new_content, "前文\n新值\n后文\n");
    assert_eq!(refreshed.new_content, "新值", "不同主题的历史转换不能复用");
}

#[test]
fn test_completed_view_cache_applies_new_hook_rows_to_cached_history() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(make_tool_history(
        "bash",
        "Bash",
        json!({"command":"echo done"}),
        "done",
    ));
    let initial = pipeline.build_tail_vms();
    pipeline.push_hook_result("bash", HookExec::new("PostToolUse", 1, 7, Vec::new()));
    let updated = pipeline.build_tail_vms();
    assert!(initial.iter().all(|message| !matches!(message, MessageViewModel::ToolBlock { hook_rows, .. } if !hook_rows.is_empty())));
    assert!(updated.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, hook_rows, .. } if tool_call_id == "bash" && hook_rows.iter().any(|row| row.text == "1 PostToolUse hook ran · 7ms"))), "后到的 hook 行必须叠加到缓存副本");
}

#[test]
fn test_completed_view_cache_removes_finished_async_hook_placeholder() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(make_tool_history(
        "bash",
        "Bash",
        json!({"command":"echo done"}),
        "done",
    ));
    let _ = pipeline.build_tail_vms();
    pipeline.start_hook_result("bash", "PostToolUse");
    let running = pipeline.build_tail_vms();
    pipeline.clear_running_hook("bash", "PostToolUse");
    let finished = pipeline.build_tail_vms();
    assert!(running.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, hook_rows, .. } if tool_call_id == "bash" && !hook_rows.is_empty())), "异步 hook 运行占位应叠加到缓存副本");
    assert!(finished.iter().all(|message| !matches!(message, MessageViewModel::ToolBlock { hook_rows, .. } if !hook_rows.is_empty())), "异步 hook 完成后不能从缓存重放运行占位");
}

#[test]
fn test_completed_view_cache_applies_shell_runtime_to_cached_background_marker() {
    let mut pipeline = MessagePipeline::new(".".into());
    let output = "<background-task-started><task-id>task-1</task-id><command>sleep 30</command><output>C:/tmp/task-1.log</output></background-task-started>";
    pipeline.set_completed(make_tool_history(
        "bash",
        "Bash",
        json!({"command":"sleep 30"}),
        output,
    ));
    let _ = pipeline.build_tail_vms();
    let started_at = std::time::Instant::now();
    pipeline.sync_shell_runtime(None, "bash", started_at, 5000, true);
    let running = pipeline.build_tail_vms();
    pipeline.remove_shell_runtime(None, "bash");
    let finished = pipeline.build_tail_vms();
    assert!(running.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, shell_backgrounded: true, started_at: Some(actual), .. } if tool_call_id == "bash" && *actual == started_at)), "shell 运行状态必须叠加到缓存副本");
    assert!(finished.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, shell_backgrounded: false, started_at: None, .. } if tool_call_id == "bash")), "移除的 shell 状态不能残留在缓存中");
}

#[test]
fn test_completed_view_cache_keeps_multiple_tool_call_arguments_and_cjk() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(vec![
        BaseMessage::human("检查中文和多个并发工具"),
        BaseMessage::ai_with_tool_calls(
            "",
            vec![
                ToolCallRequest::new("first", "Bash", json!({"command":"echo 中文"})),
                ToolCallRequest::new("second", "Bash", json!({"command":"echo 第二项"})),
            ],
        ),
        BaseMessage::tool_result("second", "第二项输出"),
        BaseMessage::tool_result("first", "中文输出"),
    ]);
    let first = pipeline.build_tail_vms();
    let repeated = pipeline.build_tail_vms();
    assert_eq!(repeated, first, "缓存命中应保持并发工具顺序和中文内容");
    assert!(repeated.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, args_display: Some(arguments), content, .. } if tool_call_id == "second" && arguments.contains("第二项") && content == "第二项输出")));
    assert!(repeated.iter().any(|message| matches!(message, MessageViewModel::ToolBlock { tool_call_id, args_display: Some(arguments), content, .. } if tool_call_id == "first" && arguments.contains("中文") && content == "中文输出")));
}

#[test]
fn test_completed_view_cache_releases_old_round_and_cleared_history() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(vec![BaseMessage::human("上一轮")]);
    let _ = pipeline.build_tail_vms();
    assert!(
        pipeline.completed_view_cache_stats().1 > 0,
        "缓存必须纳入内存统计"
    );
    pipeline.begin_round();
    assert_eq!(
        pipeline.completed_view_cache_stats(),
        (0, 0),
        "失效后必须释放缓存"
    );
    assert!(
        pipeline.completed_view_cache.borrow().is_none(),
        "新轮次应释放历史缓存"
    );
    pipeline.set_completed(vec![BaseMessage::human("这一轮")]);
    let current = pipeline.build_tail_vms();
    assert!(
        matches!(&current[0], MessageViewModel::UserBubble { content, .. } if content == "这一轮")
    );
    pipeline.clear();
    assert!(
        pipeline.build_tail_vms().is_empty(),
        "清空后不能重放缓存历史"
    );
    assert!(pipeline.cached_completed_view_models(0).is_empty());
}

#[test]
fn test_completed_view_cache_does_not_retain_history_over_byte_budget() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.set_completed(make_tool_history(
        "large",
        "Bash",
        json!({"command":"large output"}),
        "x".repeat(COMPLETED_VIEW_CACHE_BUDGET),
    ));
    let messages = pipeline.cached_completed_view_models(0);
    assert_eq!(messages.len(), 1, "超预算历史仍应正确转换");
    assert!(
        pipeline.completed_view_cache.borrow().is_none(),
        "超预算转换不能驻留第二份完整历史"
    );
}
