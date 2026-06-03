use super::*;

#[tokio::test]
async fn test_execute_shell_command_basic() {
    let output = execute_shell_command("echo hello", ".").await.unwrap();
    assert_eq!(output.stdout.trim(), "hello");
    assert_eq!(output.exit_code, 0);
}

#[tokio::test]
async fn test_execute_shell_command_error() {
    let output = execute_shell_command("definitely_not_a_peri_command_000000", ".")
        .await
        .unwrap();
    assert_ne!(output.exit_code, 0);
    assert!(
        !output.stderr.trim().is_empty() || !output.stdout.trim().is_empty(),
        "命令错误应产生 stdout 或 stderr"
    );
}

#[tokio::test]
async fn test_execute_shell_command_with_stdin() {
    let command = if cfg!(target_os = "windows") {
        "findstr hello"
    } else {
        "grep hello"
    };
    let (tx, rx) = mpsc::channel(4);
    let handle =
        tokio::spawn(async move { execute_shell_command_with_stdin(command, ".", Some(rx)).await });
    tx.send("hello world".to_string()).await.unwrap();
    tx.send("ignored".to_string()).await.unwrap();
    drop(tx);
    let output = handle.await.unwrap().unwrap();
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("hello world"));
}
