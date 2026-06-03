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
