use super::*;

fn make_record(thread_id: &str, command: &str) -> ShellCommandRecord {
    let now = Utc::now();
    ShellCommandRecord {
        id: uuid::Uuid::now_v7().to_string(),
        thread_id: thread_id.to_string(),
        command: command.to_string(),
        cwd: ".".to_string(),
        stdin: Vec::new(),
        stdout: "ok".to_string(),
        stderr: String::new(),
        exit_code: 0,
        started_at: now,
        completed_at: now,
        anchor_message_id: None,
    }
}

#[tokio::test]
async fn test_shell_command_store_append_and_load_for_thread() {
    let dir = tempfile::tempdir().unwrap();
    let store = ShellCommandStore::new(dir.path().join("shell.jsonl"));
    let target = make_record("thread-a", "echo a");
    let other = make_record("thread-b", "echo b");
    store.append(&target).await.unwrap();
    store.append(&other).await.unwrap();
    let records = store
        .load_for_thread(&"thread-a".to_string())
        .await
        .unwrap();
    assert_eq!(records, vec![target]);
}

#[tokio::test]
async fn test_shell_command_store_missing_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = ShellCommandStore::new(dir.path().join("missing.jsonl"));
    let records = store
        .load_for_thread(&"thread-a".to_string())
        .await
        .unwrap();
    assert!(records.is_empty());
}
