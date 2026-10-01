use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

struct TaskDropMarker(Arc<AtomicBool>);

impl Drop for TaskDropMarker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_drain_pipe_task_timeout_aborts_and_reaps_reader() {
    let dropped = Arc::new(AtomicBool::new(false));
    let marker = TaskDropMarker(dropped.clone());
    let task = tokio::spawn(async move {
        let _marker = marker;
        std::future::pending::<()>().await;
    });
    drain_pipe_task(task).await;
    assert!(
        dropped.load(Ordering::SeqCst),
        "超时返回前必须取消并回收 reader，不能仅丢弃句柄"
    );
}

#[tokio::test]
async fn test_shell_io_tasks_drop_stops_pending_readers_and_stdin() {
    let mut tasks = ShellIoTasks::default();
    let mut handles = Vec::new();
    for _ in 0..3 {
        handles.push(tasks.spawn(std::future::pending::<()>()));
    }
    drop(tasks);
    for handle in handles {
        let result = tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("取消 owner 后 IO 任务必须及时退出");
        assert!(result.expect_err("IO 任务应被取消").is_cancelled());
    }
}

#[tokio::test]
async fn test_streaming_deadline_preserves_root_exit_during_slow_output_drain() {
    let dir = tempfile::tempdir().unwrap();
    let (child, started) = spawn_streaming_child(
        "printf stdout; printf stderr >&2; exit 7",
        &dir.path().to_string_lossy(),
        false,
        ShellDialect::Bash,
    )
    .expect("实际启动立即退出的根进程");
    // 两条流只能先发送一条，故意让第二条在根进程退出后等待消费者。
    let (output_tx, mut output_rx) = mpsc::channel(1);
    let mut result = tokio::spawn(run_streaming_child(
        child,
        None,
        output_tx,
        Some(started + Duration::from_secs(1)),
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(1250), &mut result)
            .await
            .is_err(),
        "根进程按期退出后应继续排空，不能在执行期限到达时误报超时"
    );
    while output_rx.recv().await.is_some() {}
    let output = result
        .await
        .expect("执行任务不能 panic")
        .expect("输出排空不应覆盖根进程退出结果");
    assert_eq!(output.exit_code, 7, "应保留真实退出码");
    assert_eq!(output.stdout, "stdout");
    assert_eq!(output.stderr, "stderr");
}

#[tokio::test]
async fn test_streaming_deadline_reports_timeout_and_closes_io() {
    let dir = tempfile::tempdir().unwrap();
    let (stdin_tx, stdin_rx) = mpsc::channel(1);
    let mut execution = execute_shell_command_streaming_with_timeout(
        "printf started; sleep 3; printf leaked > leaked",
        &dir.path().to_string_lossy(),
        Some(stdin_rx),
        ShellDialect::Bash,
        Duration::from_millis(500),
    )
    .expect("实际启动待超时命令");
    let result = tokio::time::timeout(Duration::from_secs(5), &mut execution.result)
        .await
        .expect("执行超时和输出收尾都有界")
        .expect("超时应返回明确错误而不是取消 result channel");
    let error = result.expect_err("未按时退出必须报告超时");
    assert!(
        error.downcast_ref::<ShellExecutionTimedOut>().is_some(),
        "调用方必须能可靠区分执行超时和普通执行失败"
    );
    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(chunk) = execution.output_rx.recv().await {
            output.extend_from_slice(&chunk);
        }
        stdin_tx.closed().await;
    })
    .await
    .expect("超时后 stdout/stderr/stdin 任务都应退出");
    assert_eq!(output, b"started", "命令必须实际启动并保留已有输出");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !dir.path().join("leaked").exists(),
        "超时后不能继续执行命令"
    );
}

#[cfg(windows)]
fn make_descendant_scripts(dir: &std::path::Path, wait_after_spawn: bool) {
    std::fs::write(
        dir.join("child.ps1"),
        "Set-Content -LiteralPath ($PSScriptRoot + '/child-ready') -Value ready\nStart-Sleep -Milliseconds 2000\nSet-Content -LiteralPath ($PSScriptRoot + '/leaked') -Value leaked\n",
    )
    .expect("写入无害子进程脚本");
    let wait = if wait_after_spawn {
        "Start-Sleep -Seconds 30"
    } else {
        "exit 0"
    };
    std::fs::write(
        dir.join("parent.ps1"),
        format!(
            "$taskChild = Start-Process powershell -WindowStyle Hidden -PassThru -WorkingDirectory $PSScriptRoot -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','child.ps1'\nwhile (!(Test-Path -LiteralPath ($PSScriptRoot + '/child-ready'))) {{ Start-Sleep -Milliseconds 20 }}\n[Console]::Out.WriteLine('descendant-ready')\n{wait}\n"
        ),
    )
    .expect("写入无害父进程脚本");
}

#[cfg(windows)]
#[tokio::test]
async fn test_streaming_abort_cleans_cmd_and_git_bash_powershell_descendants() {
    for shell in [ShellDialect::PlatformDefault, ShellDialect::Bash] {
        let dir = tempfile::tempdir().unwrap();
        make_descendant_scripts(dir.path(), true);
        let (stdin_tx, stdin_rx) = mpsc::channel(1);
        let mut execution = execute_shell_command_streaming_with_shell(
            "powershell -NoProfile -ExecutionPolicy Bypass -File parent.ps1",
            &dir.path().to_string_lossy(),
            Some(stdin_rx),
            shell,
        )
        .expect("CMD/Git Bash 应启动 PowerShell");
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            let mut output = Vec::new();
            while let Some(chunk) = execution.output_rx.recv().await {
                output.extend_from_slice(&chunk);
                if String::from_utf8_lossy(&output).contains("descendant-ready") {
                    return true;
                }
            }
            false
        })
        .await;
        execution.abort.abort();
        let cancelled = tokio::time::timeout(Duration::from_secs(2), execution.result)
            .await
            .expect("取消应及时关闭 result");
        assert!(cancelled.is_err(), "取消不能伪装成正常执行成功");
        assert!(ready.expect("子孙进程应实际启动"), "应收到就绪标记");
        assert!(dir.path().join("child-ready").exists());
        tokio::time::timeout(Duration::from_secs(1), async {
            while execution.output_rx.recv().await.is_some() {}
            stdin_tx.closed().await;
        })
        .await
        .expect("取消后 stdout/stderr/stdin 任务不能悬空");
        tokio::time::sleep(Duration::from_millis(2300)).await;
        assert!(
            !dir.path().join("leaked").exists(),
            "{shell:?} 取消后嵌套 PowerShell 子孙不能继续运行"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn test_streaming_root_exit_cleans_remaining_descendants() {
    let dir = tempfile::tempdir().unwrap();
    make_descendant_scripts(dir.path(), false);
    let mut execution = execute_shell_command_streaming_with_shell(
        "powershell -NoProfile -ExecutionPolicy Bypass -File parent.ps1",
        &dir.path().to_string_lossy(),
        None,
        ShellDialect::Bash,
    )
    .expect("实际启动含子孙的命令");
    let result = tokio::time::timeout(Duration::from_secs(10), &mut execution.result).await;
    execution.abort.abort();
    let output = result
        .expect("根进程退出后不应等待子孙睡眠")
        .expect("result 不应取消")
        .expect("执行应成功");
    assert_eq!(output.exit_code, 0, "保留根进程真实退出码");
    assert!(dir.path().join("child-ready").exists());
    assert!(output.stdout.contains("descendant-ready"));
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert!(
        !dir.path().join("leaked").exists(),
        "根进程退出后不应留下独立运行的 Job 后代"
    );
}
