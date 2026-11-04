use super::*;
use crate::test_lock::{PROCESS_TEST_LOCK, PROCESS_TEST_TIMEOUT};

#[tokio::test]
async fn test_execute_shell_command_basic() {
    let _guard = PROCESS_TEST_LOCK.lock().await;
    let output = execute_shell_command("echo hello", ".").await.unwrap();
    assert_eq!(output.stdout.trim(), "hello");
    assert_eq!(output.exit_code, 0);
}

#[tokio::test]
async fn test_execute_shell_command_error() {
    let _guard = PROCESS_TEST_LOCK.lock().await;
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
    let _guard = PROCESS_TEST_LOCK.lock().await;
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
    let _guard = PROCESS_TEST_LOCK.lock().await;
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
    let command = format!("python \"{}\"", script_path.display());
    let mut execution = execute_shell_command_streaming(&command, ".", None);
    let seen = tokio::time::timeout(PROCESS_TEST_TIMEOUT, async {
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
    let _guard = PROCESS_TEST_LOCK.lock().await;
    // `!` 走 Bash 方言，读文件用 `cat`，Windows 绝对路径必须靠引号保住反斜杠。
    let cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let command = format!("cat \"{}\"", cargo_toml.display());
    let execution = execute_shell_command_streaming(&command, ".", None);
    let output = tokio::time::timeout(PROCESS_TEST_TIMEOUT, execution.result)
        .await
        .expect("streaming 命令应在超时预算内结束")
        .expect("streaming result channel 不应关闭")
        .expect("带引号绝对路径命令应执行成功");
    assert_eq!(output.exit_code, 0, "命令应成功执行: {output:?}");
    assert!(
        output.stdout.contains("cc-tui"),
        "应读取 cc-tui Cargo.toml，实际输出: {:?}",
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

/// 回归测试 #149：验证管道 reader task 超时机制不会挂死，
/// 且在超时后已累积的数据完整保留，不被丢弃。
#[tokio::test]
async fn test_drain_pipe_task_timeout_preserves_accumulated_data() {
    let acc = Arc::new(Mutex::new(Vec::new()));
    let acc_clone = acc.clone();
    // 模拟一个写入部分数据后因写端句柄被常驻子进程持有而永不退出的 reader task
    let hanging_task = tokio::spawn(async move {
        acc_clone
            .lock()
            .unwrap()
            .extend_from_slice(b"partial output before detach");
        // 模拟管道未关闭，read 无限等待
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    });

    let start = std::time::Instant::now();
    drain_pipe_task(hanging_task).await;
    let elapsed = start.elapsed();

    // 应在 PIPE_DRAIN_TIMEOUT（2s）左右退出，绝不能等待 60s
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "drain_pipe_task 应在超时时间内退出，实际耗时: {:?}",
        elapsed
    );

    // 关键验证：超时后已累积的数据必须完整保留
    let saved_output = acc.lock().unwrap().clone();
    assert_eq!(
        String::from_utf8_lossy(&saved_output),
        "partial output before detach",
        "已累积的输出在超时后不应被丢弃"
    );
}

/// 验证正常结束的 reader task 会立即完成，无需等待超时。
#[tokio::test]
async fn test_drain_pipe_task_normal_completion_is_immediate() {
    let acc = Arc::new(Mutex::new(Vec::new()));
    let acc_clone = acc.clone();
    let quick_task = tokio::spawn(async move {
        acc_clone
            .lock()
            .unwrap()
            .extend_from_slice(b"immediate output");
    });

    let start = std::time::Instant::now();
    drain_pipe_task(quick_task).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "正常完成的任务应立即结束，无需等待超时，实际耗时: {:?}",
        elapsed
    );
    assert_eq!(
        String::from_utf8_lossy(&acc.lock().unwrap()),
        "immediate output"
    );
}

/// `!` 命令必须走 Bash 方言，而不是平台默认（Windows = cmd）。
///
/// 回归：平台默认下输入 `! sleep 10` 只会得到「不是内部或外部命令」，
/// 与 Bash 工具的 Unix 语义不一致。
#[tokio::test]
async fn test_streaming_shell_uses_bash_dialect() {
    let _guard = PROCESS_TEST_LOCK.lock().await;
    if cfg!(windows) && cc_middlewares::process::git_bash_path().is_none() {
        // 与 Bash 工具同一前置条件：缺少 Git Bash 时不执行，避免环境相关失败。
        return;
    }
    let execution =
        execute_shell_command_streaming("echo ${BASH_VERSION:+bash-dialect}", ".", None);
    let ShellExecution {
        result, output_rx, ..
    } = execution;
    let drain = tokio::spawn(async move {
        let mut rx = output_rx;
        while rx.recv().await.is_some() {}
    });
    let output = result.await.expect("执行结果").expect("命令成功");
    drain.await.expect("排空输出");
    assert_eq!(output.exit_code, 0, "{output:?}");
    assert!(
        output.stdout.contains("bash-dialect"),
        "应运行在 Bash 方言下，实际输出: {:?}",
        output.stdout
    );
}
