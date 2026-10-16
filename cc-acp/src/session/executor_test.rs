use super::*;
use cc_agent::agent::events::BackgroundTaskResult;

#[test]
fn test_background_results_preserve_history_marker_pairs() {
    let results: Vec<_> = [true, false]
        .into_iter()
        .enumerate()
        .map(|(index, success)| BackgroundTaskResult {
            task_id: format!("background-task-{index}"),
            agent_name: "reviewer".into(),
            prompt_summary: "检查后台任务".into(),
            success,
            output: format!("后台结果 {index}"),
            tool_calls_count: 1,
            duration_ms: 100,
            child_thread_id: None,
        })
        .collect();
    let (history, user_content) = inject_bg_result_messages(
        vec![BaseMessage::human("已有对话")],
        MessageContent::text("继续处理"),
        &results,
    );
    assert_eq!(history.len(), 4, "保留历史并追加一组调用与两个结果");
    assert_eq!(history[0].content(), "已有对话");
    assert_eq!(user_content.text_content(), "继续处理");
    let calls = history[1].tool_calls();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].id, calls[1].id, "不同结果须使用不同调用 ID");
    for (index, result) in results.iter().enumerate() {
        assert_eq!(calls[index].name, "AgentResult", "保留历史协议工具名");
        assert_eq!(calls[index].arguments["task_id"], result.task_id);
        let BaseMessage::Tool {
            tool_call_id,
            content,
            is_error,
            ..
        } = &history[index + 2]
        else {
            panic!("每个后台结果都必须写入工具结果消息");
        };
        assert_eq!(tool_call_id, &calls[index].id, "工具调用与结果须配对");
        assert_eq!(content.text_content(), result.to_notification());
        assert_eq!(*is_error, !result.success, "保留后台任务成功或失败状态");
    }
}

#[tokio::test]
async fn test_runtime_start_persists_model_reasoning_and_project_branch() -> anyhow::Result<()> {
    use cc_agent::thread::{SqliteThreadStore, ThreadMeta, ThreadStore};
    let project = tempfile::tempdir()?;
    let storage = tempfile::tempdir()?;
    let output = cc_middlewares::process::shell_command("git", &["init", "--quiet"])
        .current_dir(project.path())
        .output()
        .await?;
    assert!(
        output.status.success(),
        "临时 git 项目必须初始化成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = cc_middlewares::process::shell_command(
        "git",
        &["symbolic-ref", "HEAD", "refs/heads/runtime-snapshot"],
    )
    .current_dir(project.path())
    .output()
    .await?;
    assert!(output.status.success(), "临时 git 分支必须初始化成功");
    // SQLite 会收紧父目录权限；独立存储目录避免改变临时 Git 项目的 ACL。
    let store = SqliteThreadStore::new(storage.path().join("runtime.db")).await?;
    let id = store
        .create_thread(ThreadMeta::new(project.path().to_string_lossy()))
        .await?;
    let provider = LlmProvider::OpenAi {
        api_key: "test".into(),
        base_url: "http://localhost".into(),
        model: "test-model".into(),
        thinking: Some(crate::provider::ThinkingConfig {
            enabled: true,
            budget_tokens: 1024,
            effort: "high".into(),
            max_tokens: 2048,
        }),
    };
    persist_runtime_start(&store, &id, &provider, project.path().to_str().unwrap()).await;
    let meta = store.load_meta(&id).await?;
    assert_eq!(meta.agent_status, "active");
    assert_eq!(meta.model.as_deref(), Some("test-model"));
    assert_eq!(meta.reasoning_effort.as_deref(), Some("high"));
    let branch_probe = cc_middlewares::process::shell_command(
        "git",
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )
    .current_dir(project.path())
    .output()
    .await?;
    assert_eq!(
        meta.branch.as_deref(),
        Some("runtime-snapshot"),
        "分支快照必须保留；复查 status={} stdout={} stderr={} cwd={}",
        branch_probe.status,
        String::from_utf8_lossy(&branch_probe.stdout),
        String::from_utf8_lossy(&branch_probe.stderr),
        project.path().display()
    );
    Ok(())
}

#[tokio::test]
async fn test_runtime_end_persists_completion_cancellation_and_failure() -> anyhow::Result<()> {
    use cc_agent::thread::{SqliteThreadStore, ThreadMeta, ThreadStore};
    let directory = tempfile::tempdir()?;
    let store = SqliteThreadStore::new(directory.path().join("runtime.db")).await?;
    let id = store
        .create_thread(ThreadMeta::new("/test-project"))
        .await?;
    for (ok, reason, expected) in [
        (true, PromptStopReason::EndTurn, "done"),
        (false, PromptStopReason::Cancelled, "cancelled"),
        (true, PromptStopReason::Cancelled, "cancelled"),
        (false, PromptStopReason::EndTurn, "error"),
        (false, PromptStopReason::MaxTurnRequests, "error"),
    ] {
        persist_runtime_end(&store, &id, ok, reason).await;
        assert_eq!(
            store.load_meta(&id).await?.agent_status,
            expected,
            "每条终止路径都必须写入状态"
        );
    }
    Ok(())
}
