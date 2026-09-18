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
    // 使用 flush=True 确保 Windows 上输出立即刷新
    std::fs::write(
        &script_path,
        "import time\nprint('ready', flush=True)\ntime.sleep(3)\n",
    )
    .unwrap();
    let command = format!("python {}", script_path.display());
    let mut execution = execute_shell_command_streaming(&command, ".", None);
    let seen = tokio::time::timeout(std::time::Duration::from_secs(5), async {
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

#[cfg(windows)]
#[tokio::test]
async fn test_execute_shell_command_streaming_preserves_quoted_absolute_path() {
    // agent Bash 走 streaming executor，必须保留 Windows 命令里的内层引号。
    let cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let command = format!("type \"{}\"", cargo_toml.display());
    let execution = execute_shell_command_streaming(&command, ".", None);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), execution.result)
        .await
        .expect("streaming 命令应在 5 秒内结束")
        .expect("streaming result channel 不应关闭")
        .expect("带引号绝对路径命令应执行成功");
    assert_eq!(output.exit_code, 0, "命令应成功执行: {output:?}");
    assert!(
        output.stdout.contains("peri-tui"),
        "应读取 peri-tui Cargo.toml，实际输出: {:?}",
        output.stdout
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

/// 回归测试 #149：命令 fork 出常驻子进程后，主进程退出应正常返回结果，
/// 不应因管道句柄被继承而永远等待 EOF。
///
/// 使用 PowerShell Start-Process 真正分离常驻子进程。Start-Process 创建的进程
/// 继承了 cmd.exe 的管道写句柄（Windows 句柄继承机制），即使主进程已退出，
/// 管道写端仍未关闭，读取端永远收不到 EOF。
#[cfg(windows)]
#[tokio::test]
async fn test_streaming_detached_child_does_not_hang() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bat_path = temp_dir.path().join("detach.bat");
    // Start-Process -WindowStyle Hidden 创建独立进程，但默认继承控制台句柄。
    // -RedirectStandardOutput/-RedirectStandardError 会重定向子进程自身的输出，
    // 但管道句柄仍被继承。主进程（cmd）执行完 bat 后立即退出。
    std::fs::write(
        &bat_path,
        "@echo off\r\necho detached_ok\r\npowershell -NoProfile -Command \"Start-Process node -ArgumentList '-e','setInterval(function(){},1e9)' -WindowStyle Hidden\"\r\nexit /b 0\r\n",
    )
    .unwrap();
    let command = format!("\"{}\"", bat_path.display());
    let execution = execute_shell_command_streaming(&command, ".", None);
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), execution.result)
        .await
        .expect("带常驻子进程的命令应在超时内正常返回（修复 #149）")
        .expect("result channel 不应关闭")
        .expect("命令应执行成功");
    assert_eq!(result.exit_code, 0, "主进程应正常退出");
    assert!(
        result.stdout.contains("detached_ok"),
        "应捕获到主进程输出，实际: {:?}",
        result.stdout
    );
}

/// 回归测试 #149（非流式路径）：同样验证 execute_shell_command 不会因常驻子进程挂起。
#[cfg(windows)]
#[tokio::test]
async fn test_non_streaming_detached_child_does_not_hang() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bat_path = temp_dir.path().join("detach.bat");
    std::fs::write(
        &bat_path,
        "@echo off\r\necho detached_ok\r\npowershell -NoProfile -Command \"Start-Process node -ArgumentList '-e','setInterval(function(){},1e9)' -WindowStyle Hidden\"\r\nexit /b 0\r\n",
    )
    .unwrap();
    let command = format!("\"{}\"", bat_path.display());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        execute_shell_command(&command, "."),
    )
    .await
    .expect("非流式路径也应在超时内正常返回（修复 #149）")
    .expect("命令应执行成功");
    assert_eq!(result.exit_code, 0, "主进程应正常退出");
    assert!(
        result.stdout.contains("detached_ok"),
        "应捕获到主进程输出，实际: {:?}",
        result.stdout
    );
}
