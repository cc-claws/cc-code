use super::*;
use peri_agent::messages::BaseMessage;

fn make_record(
    thread_id: &str,
    command: &str,
    anchor_message_id: Option<String>,
) -> ShellCommandRecord {
    let now = Utc::now();
    ShellCommandRecord {
        id: uuid::Uuid::now_v7().to_string(),
        thread_id: thread_id.to_string(),
        command: command.to_string(),
        cwd: ".".to_string(),
        stdin: Vec::new(),
        stdout: "done".to_string(),
        stderr: String::new(),
        exit_code: 0,
        started_at: now,
        completed_at: now,
        anchor_message_id,
    }
}

#[tokio::test]
async fn test_merge_shell_records_inserts_after_anchor_without_origin_messages() {
    let (app, _handle) = App::new_headless(80, 24).await;
    let base_msgs = vec![BaseMessage::human("q1"), BaseMessage::ai("a1")];
    let anchor_id = base_msgs[0].id().as_uuid().to_string();
    let view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, ".");
    let record = make_record("thread-a", "echo done", Some(anchor_id));

    let merged = app.merge_shell_records_into_view(view_msgs, &base_msgs, vec![record]);

    assert!(
        matches!(merged.get(1), Some(MessageViewModel::ShellCommand { command, .. }) if command == "echo done"),
        "shell 记录应按锚点插入到对应 BaseMessage 后"
    );
    assert_eq!(
        base_msgs.len(),
        2,
        "合并 shell VM 不应改变 Agent BaseMessage"
    );
}

#[tokio::test]
async fn test_merge_shell_records_without_anchor_stays_at_thread_start() {
    let (app, _handle) = App::new_headless(80, 24).await;
    let base_msgs = vec![BaseMessage::human("q1")];
    let view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, ".");
    let record = make_record("thread-a", "pwd", None);

    let merged = app.merge_shell_records_into_view(view_msgs, &base_msgs, vec![record]);

    assert!(
        matches!(merged.first(), Some(MessageViewModel::ShellCommand { command, .. }) if command == "pwd"),
        "无 Agent 锚点的 shell-only 记录应恢复到 thread 开头"
    );
}

#[tokio::test]
async fn test_cancel_shell_command_aborts_task_and_replaces_pending_vm() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let record_id = uuid::Uuid::now_v7().to_string();
    let thread_id = "thread-shell-cancel".to_string();
    let task = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    let abort_handle = task.abort_handle();

    app.session_mgr.current_mut().current_thread_id = Some(thread_id.clone());
    app.session_mgr.current_mut().messages.view_messages.push(
        MessageViewModel::shell_command_pending(
            record_id.clone(),
            "sleep 60".to_string(),
            ".".to_string(),
        ),
    );
    app.session_mgr.current_mut().shell_pool.foreground.runtime = ShellCommandRuntime {
        stdin_tx: None,
        running_record_id: Some(record_id.clone()),
        stdin_lines: vec!["hello".to_string()],
        abort_handle: Some(abort_handle),
        command: "sleep 60".to_string(),
        cwd: ".".to_string(),
        thread_id: Some(thread_id),
        started_at: Some(Utc::now()),
        anchor_message_id: None,
    };
    app.set_loading(true);

    assert!(app.cancel_shell_command(), "应成功取消运行中的 shell 命令");
    let join_result = task.await;
    assert!(
        join_result.unwrap_err().is_cancelled(),
        "取消 shell 命令应 abort 后台任务"
    );
    assert!(
        !app.session_mgr.current().shell_pool.foreground.runtime.is_running(),
        "取消后应清理 ShellCommandRuntime"
    );
    assert!(
        !app.session_mgr.current().ui.loading,
        "取消后应退出 loading"
    );
    assert!(
        matches!(
            app.session_mgr.current().messages.view_messages.last(),
            Some(MessageViewModel::ShellCommand {
                id,
                stderr,
                exit_code: Some(-1),
                ..
            }) if id == &record_id && stderr.contains("cancelled")
        ),
        "pending shell VM 应替换为取消结果"
    );
}

#[tokio::test]
async fn test_cleanup_finished_background_shells_超量移除最旧已完成() {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;

    use crate::app::{BackgroundShell, ShellStatus};
    use crate::shell_exec::CommandOutput;

    let (mut app, _handle) = App::new_headless(80, 24).await;

    // 构造已完成任务的 helper（ended_at = ago_secs 前）
    let make_bg = |id: &str, ago_secs: u64| -> BackgroundShell {
        let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
        let task = tokio::spawn(async {});
        let mut bg = BackgroundShell::new(
            id.to_string(),
            "cmd".to_string(),
            PathBuf::from("."),
            PathBuf::from(format!("/tmp/peri-test-{}.output", id)),
            rx,
            task.abort_handle(),
            std::time::Instant::now(),
        );
        bg.status = ShellStatus::Completed;
        bg.notified = true;
        bg.ended_at = Some(Instant::now() - Duration::from_secs(ago_secs));
        bg
    };

    // 填 21 个已完成任务（task-0 最旧，100s 前）
    for i in 0..21u64 {
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_bg(&format!("task-{}", i), 100 - i));
    }
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        21,
        "前置条件：21 个任务"
    );

    app.cleanup_finished_background_shells();
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        20,
        "超量时应移除最旧的 1 个已完成任务"
    );
    let remaining_ids: Vec<&str> = app
        .session_mgr
        .current()
        .background_shells
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(
        !remaining_ids.contains(&"task-0"),
        "最旧的 task-0 应被移除: {:?}",
        remaining_ids
    );
}

#[tokio::test]
async fn test_cleanup_finished_background_shells_未超量不移除() {
    use std::path::PathBuf;
    use tokio::sync::oneshot;

    use crate::app::{BackgroundShell, ShellStatus};
    use crate::shell_exec::CommandOutput;

    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
    let task = tokio::spawn(async {});
    let mut bg = BackgroundShell::new(
        "task-1".to_string(),
        "cmd".to_string(),
        PathBuf::from("."),
        PathBuf::from("/tmp/x.output"),
        rx,
        task.abort_handle(),
        std::time::Instant::now(),
    );
    bg.status = ShellStatus::Completed;
    bg.notified = true;
    app.session_mgr
        .current_mut()
        .background_shells
        .push(bg);

    app.cleanup_finished_background_shells();
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        1,
        "未超量时不应移除"
    );
}
