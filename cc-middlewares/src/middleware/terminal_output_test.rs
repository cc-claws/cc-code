use super::*;
use cc_agent::{
    agent::events::{AgentEvent, FnEventHandler},
    messages::MessageId,
    shell::{ShellDialect, ShellRequest},
    tools::ToolOutputContext,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

fn make_context() -> (ToolOutputContext, UnboundedReceiver<AgentEvent>) {
    let (sender, receiver) = unbounded_channel();
    (
        ToolOutputContext::new(
            MessageId::new(),
            "inline-stream".into(),
            None,
            Arc::new(FnEventHandler(move |event| {
                let _ = sender.send(event);
            })),
        ),
        receiver,
    )
}

fn make_request(command: &str) -> ShellRequest {
    ShellRequest {
        owner_session_id: None,
        invocation: None,
        command: command.into(),
        original_command: command.into(),
        shell: ShellDialect::Bash,
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        timeout_ms: 10000,
        execution_timeout_ms: 10000,
        run_in_background: false,
    }
}

#[tokio::test]
async fn test_inline_executor_streams_output_tail_before_completion() {
    let (context, mut receiver) = make_context();
    let handle = ToolOutputContext::scope(
        Some(context),
        InlineShellExecutor.execute(make_request(
            "printf 'first\n'; sleep 1.2; printf 'second\n' >&2; sleep 1.2; printf 'last\n'",
        )),
    )
    .await
    .expect("前台命令应启动");
    let first = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("首个输出不能等待完成")
        .expect("输出通道打开");
    assert!(
        matches!(first, AgentEvent::ToolOutput { tail, .. } if tail == "first\n"),
        "首个 stdout 应立即显示"
    );
    assert!(
        !handle.exit_signal.is_exited(),
        "命令仍在执行时就应出现输出"
    );
    let second = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("stderr 到达应刷新同一尾部")
        .expect("输出通道打开");
    assert!(
        matches!(second, AgentEvent::ToolOutput {
        tail, total_lines: 2, total_bytes: 13, elapsed_ms, ..
    } if tail == "first\nsecond\n" && elapsed_ms >= 1000),
        "stdout/stderr 应按到达顺序合并"
    );
    let output = tokio::time::timeout(Duration::from_secs(5), handle.result_rx)
        .await
        .expect("命令应结束")
        .expect("结果通道打开")
        .expect("执行成功");
    assert_eq!(
        output.stdout, "first\nlast\n",
        "最终 stdout 不受合并尾部影响"
    );
    assert_eq!(output.stderr, "second\n", "最终 stderr 保留独立原文");
    assert_eq!(output.exit_code, 0);
    let mut remaining = Vec::new();
    while let Some(event) = tokio::time::timeout(Duration::from_millis(500), receiver.recv())
        .await
        .expect("完成后展示任务应释放")
    {
        remaining.push(event);
    }
    assert!(remaining.iter().all(
        |event| matches!(event, AgentEvent::ToolOutput { tail, .. } if tail.len() <= 64 * 1024)
    ));
}

#[tokio::test]
async fn test_inline_executor_bounds_stream_without_truncating_final_output() {
    let (context, mut receiver) = make_context();
    let handle = ToolOutputContext::scope(Some(context), InlineShellExecutor.execute(make_request(
        "printf prefix; for ((i=0;i<10000;i++)); do printf 'payload-%05d\n' \"$i\"; done; sleep 1.2; printf suffix",
    ))).await.expect("大输出命令应启动");
    let output = tokio::time::timeout(Duration::from_secs(10), handle.result_rx)
        .await
        .expect("大输出应无死锁")
        .expect("结果通道打开")
        .expect("执行成功");
    assert!(
        output.stdout.starts_with("prefixpayload-00000\n"),
        "最终结果保留头部"
    );
    assert!(
        output.stdout.ends_with("payload-09999\nsuffix"),
        "最终结果保留尾部"
    );
    assert!(
        output.stdout.len() > 64 * 1024,
        "完整结果不能被展示缓存截断"
    );
    let mut saw_large_output = false;
    while let Ok(AgentEvent::ToolOutput {
        tail, total_bytes, ..
    }) = receiver.try_recv()
    {
        assert!(tail.len() <= 64 * 1024, "任一展示快照都必须有界");
        saw_large_output |= total_bytes > 64 * 1024;
    }
    assert!(
        saw_large_output,
        "静默等待时应刷新已积累的大输出，而非只保留首块"
    );
}

#[tokio::test]
async fn test_no_output_streaming_when_disabled_preserves_final_result() {
    for enabled in [false, true] {
        let (context, mut receiver) = make_context();
        let tool = TerminalMiddleware::new()
            .with_output_streaming(enabled)
            .build_tools(std::env::temp_dir().to_str().expect("临时目录为 UTF-8"))
            .pop()
            .expect("中间件提供 Bash");
        let output = ToolOutputContext::scope(
            Some(context),
            tool.invoke(serde_json::json!({
                "command": "printf preview", "timeout": 5000,
            })),
        )
        .await
        .expect("开关不影响命令执行");
        assert_eq!(output, "preview", "开关不改变模型最终结果");
        assert_eq!(
            receiver.try_recv().is_ok(),
            enabled,
            "关闭时执行器不可见输出通道"
        );
    }
}

#[tokio::test]
async fn test_inline_stream_cancellation_releases_output_capture_and_process() {
    let (context, mut receiver) = make_context();
    let handle = ToolOutputContext::scope(
        Some(context),
        InlineShellExecutor.execute(make_request(
            "printf started; sleep 0.1; printf pending; sleep 30",
        )),
    )
    .await
    .expect("取消测试命令应启动");
    tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("应出现首块输出")
        .expect("输出通道打开");
    handle.kill.abort();
    tokio::time::timeout(Duration::from_secs(2), handle.exit_signal.wait())
        .await
        .expect("取消应发布终态");
    assert_eq!(
        handle.exit_signal.outcome(),
        Some(cc_agent::shell::ShellOutcome::Cancelled)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(500), receiver.recv())
            .await
            .expect("取消后应释放输出 reader 与 timer")
            .is_none(),
        "取消后不能补发尾部"
    );
}

#[tokio::test]
async fn test_inline_stream_timeout_preserves_error_and_releases_output_capture() {
    let (context, mut receiver) = make_context();
    let mut request = make_request("printf started; sleep 30");
    request.execution_timeout_ms = 1000;
    let handle = ToolOutputContext::scope(Some(context), InlineShellExecutor.execute(request))
        .await
        .expect("超时测试命令应启动");
    tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("超时前应收到首块输出")
        .expect("输出通道打开");
    let error = tokio::time::timeout(Duration::from_secs(5), handle.result_rx)
        .await
        .expect("硬超时应结束命令")
        .expect("结果通道打开")
        .expect_err("硬超时继续返回原错误");
    assert!(
        error.to_string().contains("execution timeout"),
        "展示通道不能吞掉硬超时错误"
    );
    assert_eq!(
        handle.exit_signal.outcome(),
        Some(cc_agent::shell::ShellOutcome::TimedOut)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(500), receiver.recv())
            .await
            .expect("超时后应释放输出 reader 与 timer")
            .is_none(),
        "超时后不能遗留展示输出"
    );
}
