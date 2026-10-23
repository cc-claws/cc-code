use super::*;
use cc_agent::{
    agent::events::{AgentEvent as ExecutorEvent, FnEventHandler},
    messages::MessageId,
    shell::ShellDialect,
    tools::ToolOutputContext,
};
use std::time::Duration;

fn make_output_context() -> (ToolOutputContext, mpsc::UnboundedReceiver<ExecutorEvent>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let context = ToolOutputContext::new(
        MessageId::new(),
        "streamed-shell".into(),
        Some("source-child".into()),
        Arc::new(FnEventHandler(move |event| {
            let _ = sender.send(event);
        })),
    );
    (context, receiver)
}

fn make_request(cwd: String, command: &str, background: bool) -> ShellRequest {
    ShellRequest {
        owner_session_id: None,
        invocation: None,
        command: command.into(),
        original_command: command.into(),
        shell: ShellDialect::Bash,
        cwd,
        timeout_ms: 5_000,
        execution_timeout_ms: 5_000,
        run_in_background: background,
    }
}

#[tokio::test]
async fn test_executor_streams_real_stdout_and_stderr_before_completion() {
    let directory = tempfile::tempdir().expect("创建隔离目录");
    let cwd = directory.path().to_string_lossy().into_owned();
    let (sender, mut registrations) = mpsc::unbounded_channel();
    let executor = AgentShellExecutor::new(sender, cwd.clone(), "output-contract".into());
    let (context, mut events) = make_output_context();
    let handle = ToolOutputContext::scope(
        Some(context),
        executor.execute(make_request(
            cwd,
            "printf stdout; sleep 0.1; printf stderr >&2; sleep 1.2; printf last",
            false,
        )),
    )
    .await
    .expect("应启动Bash");
    let registration = registrations.recv().await.expect("应登记同一个进程");
    let first = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("完成前应采样输出")
        .expect("应有ToolOutput事件");
    assert!(
        matches!(&first, ExecutorEvent::ToolOutput { tool_call_id, source_agent_id:Some(source), tail, .. } if tool_call_id == "streamed-shell" && source == "source-child" && tail.contains("stdout")),
        "必须在spawn前捕获调用身份"
    );
    assert!(
        !registration.exit_signal.is_exited(),
        "首个输出不能延迟到进程完成"
    );
    let second = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("安静期间也应发布dirty尾部")
        .expect("应有后续ToolOutput");
    assert!(
        matches!(&second, ExecutorEvent::ToolOutput { tail, total_bytes, .. } if tail.contains("stdout") && tail.contains("stderr") && *total_bytes >= 12),
        "stdout/stderr必须共享有序尾缓存"
    );
    let result = tokio::time::timeout(Duration::from_secs(5), handle.result_rx)
        .await
        .expect("进程应完成")
        .expect("结果通道可用")
        .expect("进程应成功");
    assert_eq!(result.stdout, "stdoutlast", "模型可见结果仍使用原完整捕获");
    assert_eq!(result.stderr, "stderr");
    let disk = tokio::fs::read_to_string(&registration.output_path)
        .await
        .expect("完整输出已写盘");
    assert!(disk.contains("stdout") && disk.contains("stderr") && disk.contains("last"));
}

#[tokio::test]
async fn test_output_tee_background_handoff_drops_dirty_timer_but_keeps_disk_bytes() {
    let (context, mut events) = make_output_context();
    let handoff = Arc::new(ShellHandoff::new(true, false));
    let (sender, receiver) = mpsc::channel(4);
    let mut disk = output_for_disk(
        receiver,
        Arc::clone(&handoff),
        Some(context.clone().stream()),
    );
    sender.send(b"first".to_vec()).await.expect("发送首个输出");
    assert_eq!(disk.recv().await, Some(b"first".to_vec()));
    assert!(events.recv().await.is_some(), "首个输出应立即发布");
    sender
        .send(b"dirty".to_vec())
        .await
        .expect("发送节流中的输出");
    assert_eq!(disk.recv().await, Some(b"dirty".to_vec()));
    assert!(handoff.background(), "前台应移交到后台");
    assert!(
        tokio::time::timeout(Duration::from_millis(1200), events.recv())
            .await
            .is_err(),
        "没有新chunk时也必须撤销待发timer"
    );
    sender
        .send(b"background".to_vec())
        .await
        .expect("后台仍可输出");
    drop(sender);
    assert_eq!(
        disk.recv().await,
        Some(b"background".to_vec()),
        "后台输出仍原样送入磁盘链路"
    );
    assert!(disk.recv().await.is_none());
    assert!(events.try_recv().is_err(), "后台不得再产生前台ToolOutput");
    drop(context);
}

#[tokio::test]
async fn test_executor_direct_background_does_not_stream_and_preserves_result() {
    let directory = tempfile::tempdir().expect("创建隔离目录");
    let cwd = directory.path().to_string_lossy().into_owned();
    let (sender, mut registrations) = mpsc::unbounded_channel();
    let executor = AgentShellExecutor::new(sender, cwd.clone(), "background-output".into());
    let (context, mut events) = make_output_context();
    let handle = ToolOutputContext::scope(
        Some(context),
        executor.execute(make_request(cwd, "printf background", true)),
    )
    .await
    .expect("应启动后台Bash");
    let registration = registrations.recv().await.expect("后台仍需登记");
    let result = tokio::time::timeout(Duration::from_secs(5), handle.result_rx)
        .await
        .expect("进程应完成")
        .expect("结果通道可用")
        .expect("进程应成功");
    assert_eq!(result.stdout, "background");
    assert!(registration.direct_background);
    assert!(
        events.try_recv().is_err(),
        "直接后台任务不能创建前台输出事件"
    );
    assert_eq!(
        tokio::fs::read_to_string(registration.output_path)
            .await
            .expect("后台磁盘输出应完整"),
        "background"
    );
}
