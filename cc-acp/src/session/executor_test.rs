use super::*;
use cc_agent::agent::events::BackgroundTaskResult;

fn make_pump_tool_end(name: &str) -> ExecutorEvent {
    ExecutorEvent::ToolEnd {
        message_id: MessageId::new(),
        tool_call_id: format!("{name}-call"),
        name: name.into(),
        output: "finished".into(),
        is_error: false,
        source_agent_id: None,
    }
}

async fn make_pump_trace(
    event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>,
    stop_rx: &mut oneshot::Receiver<()>,
) -> Vec<String> {
    let mut trace = Vec::new();
    while let Some(event) = next_pump_event(event_rx, stop_rx).await {
        match event {
            ExecutorEvent::ToolEnd { name, .. } => trace.push(format!("tool:{name}")),
            ExecutorEvent::AgentExecutionFailed { message } => {
                trace.push(format!("failed:{message}"));
            }
            _ => panic!("测试泵不应收到其他事件"),
        }
    }
    // 对齐实际泵：排空尾部事件后，唯一的退出路径才记录完成。
    trace.push("done".into());
    trace
}

#[tokio::test]
async fn test_event_pump_stop_drains_tail_with_cached_sender() {
    let (cached_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = oneshot::channel();
    cached_tx.send(make_pump_tool_end("Bash")).unwrap();
    cached_tx
        .send(ExecutorEvent::AgentExecutionFailed {
            message: "cancelled".into(),
        })
        .unwrap();
    stop_tx.send(()).unwrap();

    // 缓存工具仍持有 Sender，结束不得依赖它被释放。
    let trace = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        make_pump_trace(&mut event_rx, &mut stop_rx),
    )
    .await
    .expect("显式结束必须及时排空并完成");
    assert_eq!(trace, ["tool:Bash", "failed:cancelled", "done"]);
    assert!(cached_tx.send(make_pump_tool_end("late")).is_err());
    assert!(
        next_pump_event(&mut event_rx, &mut stop_rx).await.is_none(),
        "再次读取关闭的泵不得重读已经完成的 oneshot 或产生迟到事件"
    );
}

#[tokio::test]
async fn test_event_pump_stop_finishes_with_active_async_sender() {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (started_tx, started_rx) = oneshot::channel();
    let (stop_tx, mut stop_rx) = oneshot::channel();
    let producer = tokio::spawn(async move {
        event_tx.send(make_pump_tool_end("async_hook")).unwrap();
        started_tx.send(()).unwrap();
        loop {
            tokio::task::yield_now().await;
            if event_tx.send(make_pump_tool_end("async_hook")).is_err() {
                return;
            }
        }
    });
    started_rx.await.unwrap();
    stop_tx.send(()).unwrap();

    let trace = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        make_pump_trace(&mut event_rx, &mut stop_rx),
    )
    .await
    .expect("异步 hook 持续发送也不能阻止本轮结束");
    assert!(trace.len() >= 2, "至少转发已入队的首条事件并完成");
    assert!(trace[..trace.len() - 1]
        .iter()
        .all(|item| item == "tool:async_hook"));
    assert_eq!(trace.last().map(String::as_str), Some("done"));
    assert_eq!(trace.iter().filter(|item| *item == "done").count(), 1);
    tokio::time::timeout(std::time::Duration::from_secs(1), producer)
        .await
        .expect("关闭后异步发送者必须观察到发送失败")
        .expect("异步发送任务不应 panic");
    assert!(event_rx.try_recv().is_err(), "完成后不得残留迟到事件");
}

#[tokio::test]
async fn test_event_pump_forwards_live_events_until_stop() {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = oneshot::channel();
    for name in ["first", "second"] {
        event_tx.send(make_pump_tool_end(name)).unwrap();
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            next_pump_event(&mut event_rx, &mut stop_rx),
        )
        .await
        .expect("尚未结束时也必须实时转发")
        .expect("收到 stop 前不能提前结束");
        assert!(matches!(event, ExecutorEvent::ToolEnd { name: actual, .. } if actual == name));
        assert!(!event_rx.is_closed(), "正常转发不得关闭接收端");
    }
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            next_pump_event(&mut event_rx, &mut stop_rx),
        )
        .await
        .is_err(),
        "无新事件且 stop 未发送时必须继续等待"
    );
    stop_tx.send(()).unwrap();
    assert!(next_pump_event(&mut event_rx, &mut stop_rx).await.is_none());
}

#[tokio::test]
async fn test_event_pump_natural_eof_finishes_without_stop() {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (_stop_tx, mut stop_rx) = oneshot::channel();
    event_tx.send(make_pump_tool_end("last")).unwrap();
    drop(event_tx);

    let trace = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        make_pump_trace(&mut event_rx, &mut stop_rx),
    )
    .await
    .expect("所有 Sender 自然释放后不得等待 stop");
    assert_eq!(trace, ["tool:last", "done"]);
}

#[tokio::test]
async fn test_event_pump_dropped_stop_sender_closes_and_drains() {
    let (cached_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    cached_tx.send(make_pump_tool_end("last")).unwrap();
    drop(stop_tx);

    let trace = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        make_pump_trace(&mut event_rx, &mut stop_rx),
    )
    .await
    .expect("结束信号发送端异常释放也必须收尾");
    assert_eq!(trace, ["tool:last", "done"]);
    assert!(cached_tx.send(make_pump_tool_end("late")).is_err());
    assert!(
        next_pump_event(&mut event_rx, &mut stop_rx).await.is_none(),
        "关闭后不应再次 poll 已取消的 oneshot"
    );
}

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

fn make_bg_result(task_id: &str, success: bool) -> BackgroundTaskResult {
    BackgroundTaskResult {
        task_id: task_id.into(),
        agent_name: "reviewer".into(),
        prompt_summary: "检查后台任务".into(),
        success,
        output: format!("后台结果 {task_id}"),
        tool_calls_count: 1,
        duration_ms: 100,
        child_thread_id: None,
    }
}

/// 运行期已完成的任务已由 drain_notifications 以 Human 写入历史，续接不得再注入一次。
#[test]
fn test_inject_bg_result_skips_result_already_delivered_by_drain() {
    let result = make_bg_result("bg-delivered-0001", true);
    let delivered = BaseMessage::human(result.to_notification());
    let (history, user_content) = inject_bg_result_messages(
        vec![BaseMessage::human("已有对话"), delivered],
        MessageContent::text("继续处理"),
        &[result],
    );
    assert_eq!(
        history.len(),
        2,
        "已投递的结果不得再以 tool_result 重复注入"
    );
    assert_eq!(history[0].content(), "已有对话");
    assert_eq!(user_content.text_content(), "继续处理");
}

/// 同一批结果中只有未进历史的部分需要注入，工具调用与结果仍须成对。
#[test]
fn test_inject_bg_result_injects_only_results_missing_from_history() {
    let delivered = make_bg_result("bg-delivered-0002", true);
    let pending = make_bg_result("bg-pending-0002", true);
    let pending_notification = pending.to_notification();
    let history_in = vec![
        BaseMessage::human("已有对话"),
        BaseMessage::human(delivered.to_notification()),
    ];
    let (history, _) = inject_bg_result_messages(
        history_in,
        MessageContent::text("继续处理"),
        &[delivered, pending],
    );
    assert_eq!(history.len(), 4, "只注入未进历史的那一条结果");
    let calls = history[2].tool_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["task_id"], "bg-pending-0002");
    assert_eq!(history[3].content(), pending_notification);
}

/// 失败通知同样按前缀判重，避免错误输出重复注入。
#[test]
fn test_inject_bg_result_dedupes_failed_notification() {
    let result = make_bg_result("bg-failed-0003", false);
    let history_in = vec![BaseMessage::human(result.to_notification())];
    let (history, _) =
        inject_bg_result_messages(history_in, MessageContent::text("继续处理"), &[result]);
    assert_eq!(history.len(), 1, "失败通知已进历史时不得重复注入");
}

/// 去重守卫依赖 `to_notification()` 与 `notification_prefix()` 同源。
///
/// 两者若各自硬编码一份 `[Background task ...` 字面量，改措辞时会静默失配，
/// 让已完成的后台结果被重复注入上下文。本测试把「通知正文以判重前缀开头」
/// 锁成不变量：成功与失败两条路径都必须成立。
#[test]
fn test_bg_notification_starts_with_dedup_prefix() {
    for success in [true, false] {
        let result = make_bg_result("bg-invariant-0004", success);
        let notification = result.to_notification();
        let prefix = result.notification_prefix();
        assert!(
            notification.starts_with(&prefix),
            "to_notification() 必须以 notification_prefix() 开头（success={success}），\
             否则 is_bg_result_delivered 的判重会静默失效：prefix={prefix:?} notification={notification:?}"
        );
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
async fn test_session_manager_creation_persists_selected_model_snapshot() -> anyhow::Result<()> {
    use crate::provider::{PeriConfig, ProviderConfig, ProviderModels, ThinkingConfig};
    use cc_agent::thread::{FilesystemThreadStore, ThreadStore};
    let directory = tempfile::tempdir()?;
    let store = std::sync::Arc::new(FilesystemThreadStore::new(directory.path().join("threads")));
    let mut config = PeriConfig::default();
    config.config.active_provider_id = "snapshot".into();
    config.config.active_alias = "sonnet".into();
    config.config.providers = vec![ProviderConfig {
        id: "snapshot".into(),
        provider_type: "openai".into(),
        api_key: "test".into(),
        models: ProviderModels {
            sonnet: "initial-model".into(),
            haiku: "selected-model".into(),
            ..Default::default()
        },
        ..Default::default()
    }];
    let provider = LlmProvider::from_config(&config).expect("测试 provider 配置必须有效");
    let manager = SessionManager::new(
        store.clone(),
        provider,
        std::sync::Arc::new(config),
        cc_middlewares::hitl::shared_mode::SharedPermissionMode::new(
            cc_middlewares::hitl::shared_mode::PermissionMode::Bypass,
        ),
        None,
    );
    let cwd = directory.path().to_string_lossy();
    let (_, initial_id) = manager.new_session(&cwd).await?;
    assert_eq!(
        store.load_meta(&initial_id).await?.model.as_deref(),
        Some("initial-model")
    );
    let (_, selected_id) = manager
        .new_session_with_settings(
            &cwd,
            "snapshot".into(),
            "haiku".into(),
            Some(ThinkingConfig {
                enabled: true,
                budget_tokens: 1024,
                effort: "high".into(),
                max_tokens: 2048,
            }),
        )
        .await?;
    let selected = store.load_meta(&selected_id).await?;
    assert_eq!(
        selected.model.as_deref(),
        Some("selected-model"),
        "创建时必须按指定别名解析真实模型"
    );
    assert_eq!(selected.reasoning_effort.as_deref(), Some("high"));
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
