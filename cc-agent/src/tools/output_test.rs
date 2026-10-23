use super::*;
use crate::agent::events::FnEventHandler;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

fn make_context() -> (ToolOutputContext, UnboundedReceiver<AgentEvent>) {
    let (sender, receiver) = unbounded_channel();
    let context = ToolOutputContext::new(
        MessageId::new(),
        "stream-call".into(),
        Some("child-instance".into()),
        Arc::new(FnEventHandler(move |event| {
            sender.send(event).expect("测试接收端应保持打开");
        })),
    );
    (context, receiver)
}

#[tokio::test]
async fn test_tool_output_first_chunk_is_immediate_and_preserves_identity() {
    let (context, mut receiver) = make_context();
    let message_id = context.message_id;
    let mut stream = context.stream();
    stream.push(b"first\n");
    assert!(
        matches!(receiver.try_recv().expect("首次输出同步可见"), AgentEvent::ToolOutput {
        message_id: id, tool_call_id, tail, total_lines: 1, total_bytes: 6,
        source_agent_id: Some(source), ..
    } if id == message_id && tool_call_id == "stream-call" && tail == "first\n" && source == "child-instance")
    );
    assert!(receiver.try_recv().is_err(), "无新输出不能重复发布");
}

#[tokio::test]
async fn test_tool_output_throttled_to_one_per_second_and_flushes_quiet_burst() {
    let (context, mut receiver) = make_context();
    let mut stream = context.stream();
    stream.push(b"one\n");
    receiver.try_recv().expect("首次输出立即发布");
    stream.push(b"two\n");
    stream.push(b"three");
    assert!(receiver.try_recv().is_err(), "一秒内只发第一次");
    let event = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .expect("burst 停止后仍应在下一秒发布最新内容")
        .expect("流保持打开");
    assert!(matches!(event, AgentEvent::ToolOutput {
        tail, total_lines: 3, total_bytes: 13, elapsed_ms, ..
    } if tail == "one\ntwo\nthree" && elapsed_ms >= 1000));
    tokio::task::yield_now().await;
    assert!(receiver.try_recv().is_err(), "没有新内容不能持续发心跳");
    assert!(
        !stream.state.lock().expect("读取状态").timer_pending,
        "发布后不遗留待发 timer"
    );
}

#[tokio::test]
async fn test_tool_output_utf8_chunks_and_crlf_count_as_single_newlines() {
    let (context, mut receiver) = make_context();
    let mut stream = context.stream();
    stream.push("开始\r".as_bytes());
    receiver.try_recv().expect("首个完整前缀立即可见");
    stream.push(&[b'\n', 0xe4]);
    {
        let mut state = stream.state.lock().expect("读取尾部");
        assert_eq!(
            decode_tail(state.tail.make_contiguous()),
            "开始\r\n",
            "末端半个 UTF-8 字符不能触发 GBK 兜底"
        );
    }
    stream.push(&[0xbd, 0xa0, b'\r', b'\n']);
    let event = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .expect("尾部应刷新")
        .expect("流保持打开");
    assert!(
        matches!(event, AgentEvent::ToolOutput {
        tail, total_lines: 2, total_bytes: 13, ..
    } if tail == "开始\r\n你\r\n"),
        "分块中文与 CRLF 必须完整还原"
    );
}

#[tokio::test]
async fn test_tool_output_tail_is_bounded_before_and_after_utf8_decode() {
    let (context, mut receiver) = make_context();
    let mut stream = context.stream();
    let output = format!("{}\n", "中文".repeat(MAX_TAIL_BYTES));
    stream.push(output.as_bytes());
    let state = stream.state.lock().expect("读取环形缓存");
    assert!(
        state.tail.len() <= MAX_TAIL_BYTES,
        "原始跨管道缓存上限为 64 KiB"
    );
    let event = receiver.try_recv().expect("首次大块输出立即可见");
    assert!(
        matches!(event, AgentEvent::ToolOutput {
        tail, total_lines: 1, total_bytes, ..
    } if total_bytes == output.len() && tail.len() <= MAX_TAIL_BYTES && tail.ends_with("中文\n") && !tail.contains('\u{fffd}')),
        "截断不能切坏 UTF-8 字符，也不能影响总字节数"
    );
}

#[tokio::test]
async fn test_tool_output_stop_and_drop_cancel_trailing_timer() {
    for explicit_stop in [false, true] {
        let (context, mut receiver) = make_context();
        let mut stream = context.stream();
        stream.push(b"first");
        receiver.try_recv().expect("读取首次输出");
        stream.push(b"suppressed");
        if explicit_stop {
            stream.stop();
            stream.push(b"must-not-publish");
        }
        drop(stream);
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(500), receiver.recv())
                .await
                .expect("停止后应立即释放 timer 与 handler")
                .is_none(),
            "停止后不能补发尾部"
        );
    }
}

#[tokio::test]
async fn test_tool_output_invocation_scope_closes_captured_host_stream() {
    let (context, mut receiver) = make_context();
    let mut stream = ToolOutputContext::scope_invocation(Some(context.clone()), async {
        let mut stream = ToolOutputContext::current()
            .expect("调用内可捕获通道")
            .stream();
        stream.push(b"first");
        stream.push(b"queued");
        stream
    })
    .await;
    receiver.try_recv().expect("首次输出存在");
    assert!(
        context.cancel.is_cancelled(),
        "工具调用结束必须关闭宿主捕获的通道"
    );
    stream.push(b"after-tool-end");
    tokio::task::yield_now().await;
    assert!(
        receiver.try_recv().is_err(),
        "ToolEnd 之后的 reader 不能再发输出"
    );
    assert!(
        ToolOutputContext::current().is_none(),
        "task-local 不能泄漏到下一轮"
    );
}

#[tokio::test]
async fn test_tool_output_cancelled_invocation_closes_detached_host_stream() {
    let (context, mut receiver) = make_context();
    let (sender, stream_receiver) = tokio::sync::oneshot::channel();
    let invocation = tokio::spawn(ToolOutputContext::scope_invocation(
        Some(context.clone()),
        async move {
            let mut stream = ToolOutputContext::current()
                .expect("调用内可捕获通道")
                .stream();
            stream.push(b"first");
            stream.push(b"queued");
            assert!(sender.send(stream).is_ok(), "模拟宿主应持有输出流");
            std::future::pending::<()>().await;
        },
    ));
    let mut stream = stream_receiver.await.expect("宿主应捕获输出流");
    receiver.try_recv().expect("首次输出存在");
    invocation.abort();
    assert!(
        invocation.await.expect_err("工具调用应取消").is_cancelled(),
        "取消必须销毁调用作用域"
    );
    assert!(
        context.cancel.is_cancelled(),
        "取消路径也必须关闭宿主输出通道"
    );
    stream.push(b"after-cancellation");
    tokio::time::timeout(
        Duration::from_millis(500),
        stream.pending.take().expect("第二块安排了延迟发布"),
    )
    .await
    .expect("取消后 timer 应立即退出")
    .expect("timer 正常响应取消");
    assert!(receiver.try_recv().is_err(), "取消后不能发布待发尾部");
    assert!(
        !stream.state.lock().expect("读取状态").timer_pending,
        "取消后 timer 应退出"
    );
}

#[tokio::test]
async fn test_tool_output_concurrent_streams_have_independent_buffers_and_timers() {
    let (first_context, mut first_receiver) = make_context();
    let (mut second_context, mut second_receiver) = make_context();
    second_context.tool_call_id = "other-call".into();
    second_context.source_agent_id = None;
    let mut first_stream = first_context.stream();
    let mut second_stream = second_context.stream();
    first_stream.push(b"first-a\n");
    first_receiver.try_recv().expect("第一个工具立即发布");
    first_stream.push(b"first-b\n");
    second_stream.push(b"second-a\n");
    assert!(
        matches!(second_receiver.try_recv().expect("第二个工具独立立即发布"),
            AgentEvent::ToolOutput { tool_call_id, tail, total_bytes: 9, source_agent_id: None, .. }
            if tool_call_id == "other-call" && tail == "second-a\n"),
        "不能受其他工具节流或尾缓存影响"
    );
    second_stream.push(b"second-b\n");
    let first = tokio::time::timeout(Duration::from_secs(3), first_receiver.recv())
        .await
        .expect("第一个工具独立刷新")
        .expect("第一个输出通道打开");
    let second = tokio::time::timeout(Duration::from_secs(3), second_receiver.recv())
        .await
        .expect("第二个工具独立刷新")
        .expect("第二个输出通道打开");
    assert!(matches!(first, AgentEvent::ToolOutput {
        tail, total_lines: 2, total_bytes: 16, elapsed_ms, source_agent_id: Some(_), ..
    } if tail == "first-a\nfirst-b\n" && elapsed_ms >= 1000));
    assert!(matches!(second, AgentEvent::ToolOutput {
        tail, total_lines: 2, total_bytes: 18, elapsed_ms, source_agent_id: None, ..
    } if tail == "second-a\nsecond-b\n" && elapsed_ms >= 1000));
}

#[tokio::test]
async fn test_tool_output_disabled_scope_hides_and_restores_parent_context() {
    let (context, _receiver) = make_context();
    ToolOutputContext::scope(Some(context), async {
        assert!(ToolOutputContext::current().is_some());
        ToolOutputContext::scope(None, async {
            assert!(
                ToolOutputContext::current().is_none(),
                "关闭开关必须屏蔽父作用域"
            );
        })
        .await;
        assert!(
            ToolOutputContext::current().is_some(),
            "嵌套作用域应恢复父通道"
        );
    })
    .await;
}
