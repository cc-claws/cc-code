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
    app.session_mgr.current_mut().shell_command = ShellCommandRuntime {
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
        !app.session_mgr.current().shell_command.is_running(),
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
