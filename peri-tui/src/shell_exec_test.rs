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

#[tokio::test]
async fn test_execute_shell_command_streaming_receives_buffered_python_output_before_exit() {
    if !python_available().await {
        return;
    }
    let temp_dir = tempfile::tempdir().unwrap();
    let script_path = temp_dir.path().join("buffered_output.py");
    std::fs::write(&script_path, "import time\nprint('ready')\ntime.sleep(3)\n").unwrap();
    let command = format!("python {}", script_path.display());
    let mut execution = execute_shell_command_streaming(&command, ".", None);
    let seen = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut bytes = Vec::new();
        while let Some(chunk) = execution.output_rx.recv().await {
            bytes.extend_from_slice(&chunk);
            if String::from_utf8_lossy(&bytes).contains("ready") {
                break;
            }
        }
        bytes
    })
    .await
    .expect("streaming 应在进程退出前收到首行输出");

    execution.abort.abort();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), execution.result).await;

    let text = String::from_utf8_lossy(&seen);
    assert!(
        text.contains("ready"),
        "未显式 flush 的脚本输出应实时进入 streaming channel，实际输出: {text:?}"
    );
}

#[test]
fn test_streaming_command_with_unbuffered_interpreters_adds_php_flush_flags() {
    let command = streaming_command_with_unbuffered_interpreters("php script.php");
    assert_eq!(
        command,
        "php -d output_buffering=0 -d implicit_flush=1 script.php"
    );
}

async fn python_available() -> bool {
    execute_shell_command("python --version", ".")
        .await
        .map(|output| output.exit_code == 0)
        .unwrap_or(false)
}
