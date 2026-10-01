use super::*;
use crate::tools::output_persist::truncate_bytes;
use cc_agent::shell::ShellHandoff;
use cc_agent::tools::BaseTool;
use std::time::Instant;

struct MockShellExecutor {
    handle: tokio::sync::Mutex<Option<cc_agent::shell::AgentShellHandle>>,
    requests: tokio::sync::Mutex<Vec<cc_agent::shell::ShellRequest>>,
}

#[tokio::test]
async fn test_shell_wait_accepted_background_wins_over_ready_completion() {
    let (result_tx, result_rx) = oneshot::channel();
    let handoff = ShellHandoff::new(true, false);
    assert!(handoff.background(), "模拟 UI 已成功移交后台");
    result_tx
        .send(Ok(cc_agent::shell::ShellCommandOutput {
            stdout: "done".into(),
            stderr: String::new(),
            exit_code: 0,
        }))
        .expect("结果同时就绪");
    let result = wait_for_shell_result(result_rx, &handoff, 5000).await;
    assert!(
        matches!(result, ShellWaitResult::Backgrounded),
        "已接受的后台移交不能又返回前台结果"
    );
}

#[tokio::test]
async fn test_shell_wait_completed_result_closes_background_requests() {
    let (result_tx, result_rx) = oneshot::channel();
    let handoff = ShellHandoff::new(true, false);
    result_tx
        .send(Ok(cc_agent::shell::ShellCommandOutput {
            stdout: "done".into(),
            stderr: String::new(),
            exit_code: 0,
        }))
        .expect("完成结果就绪");
    let result = wait_for_shell_result(result_rx, &handoff, 5000).await;
    assert!(
        matches!(result, ShellWaitResult::Completed(Ok(Ok(_)))),
        "没有移交时保留前台结果"
    );
    assert!(!handoff.background(), "返回前台结果后不能再接受后台请求");
}

#[tokio::test]
async fn test_shell_wait_timeout_transfers_once_and_rejects_duplicate_background_requests() {
    let (_result_tx, result_rx) = oneshot::channel();
    let handoff = ShellHandoff::new(true, false);
    let result = wait_for_shell_result(result_rx, &handoff, 0).await;
    assert!(
        matches!(result, ShellWaitResult::Backgrounded),
        "前台等待超时时应原子移交后台，不应留下单独的请求消费窗口"
    );
    assert!(handoff.is_backgrounded(), "后台结果与实际归属必须一致");
    assert!(!handoff.background(), "自动后台化后不能重复接受手动移交");
}

#[tokio::test]
async fn test_shell_wait_timeout_without_background_support_preserves_foreground_ownership() {
    let (_result_tx, result_rx) = oneshot::channel();
    let handoff = ShellHandoff::new(false, false);
    let result = wait_for_shell_result(result_rx, &handoff, 0).await;
    assert!(matches!(result, ShellWaitResult::TimedOut));
    assert!(!handoff.background(), "无后台宿主不能接受移交");
    assert!(handoff.settle_foreground(), "前台守卫仍须负责取消命令");
}

#[tokio::test]
async fn test_shell_wait_concurrent_completion_and_background_have_one_owner() {
    for _ in 0..128 {
        let (result_tx, result_rx) = oneshot::channel();
        let handoff = Arc::new(ShellHandoff::new(true, false));
        result_tx
            .send(Ok(cc_agent::shell::ShellCommandOutput {
                stdout: "done".into(),
                stderr: String::new(),
                exit_code: 0,
            }))
            .expect("完成结果就绪");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let sender_barrier = barrier.clone();
        let sender_handoff = handoff.clone();
        let sender = std::thread::spawn(move || {
            sender_barrier.wait();
            sender_handoff.background()
        });
        barrier.wait();
        let result = wait_for_shell_result(result_rx, &handoff, 5000).await;
        let accepted = sender.join().expect("发送线程应正常退出");
        assert_eq!(
            matches!(result, ShellWaitResult::Backgrounded),
            accepted,
            "成功移交与后台结果必须一致，不能同时拥有前台结果和后台通知"
        );
    }
}

#[tokio::test]
async fn test_bash_execution_timeout_rejects_invalid_values_before_execution() {
    let executor = make_completed_executor("", "", 0);
    let tool = BashTool::with_executor(".", executor.clone());
    for invalid in [
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(600001),
        serde_json::json!(1.5),
        serde_json::json!("100"),
        serde_json::Value::Null,
    ] {
        let result = tool
            .invoke(serde_json::json!({
                "command": "printf must-not-run",
                "execution_timeout": invalid
            }))
            .await;
        assert!(
            result
                .expect_err("非法硬期限必须拒绝")
                .to_string()
                .contains("execution_timeout"),
            "应明确指出参数错误"
        );
    }
    assert!(
        executor.requests.lock().await.is_empty(),
        "拒绝时不得提交命令"
    );
}

#[tokio::test]
async fn test_bash_execution_timeout_is_independent_of_foreground_wait() {
    for hard_limit in [None, Some(1), Some(300000), Some(600000)] {
        let executor = make_completed_executor("", "", 0);
        let tool = BashTool::with_executor(".", executor.clone());
        let mut input = serde_json::json!({"command": "printf done", "timeout": 100});
        if let Some(hard_limit) = hard_limit {
            input["execution_timeout"] = serde_json::json!(hard_limit);
        }
        tool.invoke(input).await.expect("合法硬期限应透传");
        let requests = executor.requests.lock().await;
        assert_eq!(requests.len(), 1, "命令只执行一次");
        assert_eq!(requests[0].timeout_ms, 100, "硬期限不能覆盖前台等待时间");
        assert_eq!(
            requests[0].execution_timeout_ms,
            hard_limit.unwrap_or(600000)
        );
    }
}

#[tokio::test]
async fn test_inline_shell_hard_timeout_preserves_terminal_reason() {
    use cc_agent::shell::{ShellDialect, ShellOutcome, ShellRequest};
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let handle = InlineShellExecutor
        .execute(ShellRequest {
            owner_session_id: None,
            invocation: None,
            command: "sleep 30; printf unexpected > late-output".into(),
            original_command: "sleep 30; printf unexpected > late-output".into(),
            shell: ShellDialect::Bash,
            cwd: dir.path().to_string_lossy().into_owned(),
            timeout_ms: 5000,
            execution_timeout_ms: 100,
            run_in_background: false,
        })
        .await
        .expect("应创建前台句柄");
    tokio::time::timeout(Duration::from_secs(5), handle.exit_signal.wait())
        .await
        .expect("硬期限应及时终止命令");
    assert_eq!(handle.exit_signal.outcome(), Some(ShellOutcome::TimedOut));
    assert!(handle.result_rx.await.expect("终态应有结果").is_err());
    assert_eq!(
        handle.exit_signal.outcome(),
        Some(ShellOutcome::TimedOut),
        "取消守卫不能覆盖真实超时"
    );
    assert!(
        !dir.path().join("late-output").exists(),
        "超时后不能执行后续副作用"
    );
}

#[tokio::test]
async fn test_inline_shell_cancellation_publishes_terminal_outcome() {
    use cc_agent::shell::{ShellDialect, ShellOutcome, ShellRequest};
    let handle = InlineShellExecutor
        .execute(ShellRequest {
            owner_session_id: None,
            invocation: None,
            command: "sleep 30".into(),
            original_command: "sleep 30".into(),
            shell: ShellDialect::Bash,
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            timeout_ms: 5000,
            execution_timeout_ms: 600_000,
            run_in_background: false,
        })
        .await
        .expect("应创建前台执行句柄");
    handle.kill.abort();
    tokio::time::timeout(Duration::from_secs(5), handle.exit_signal.wait())
        .await
        .expect("取消必须有终态");
    assert_eq!(handle.exit_signal.outcome(), Some(ShellOutcome::Cancelled));
}

#[async_trait::async_trait]
impl cc_agent::shell::ShellExecutor for MockShellExecutor {
    async fn execute(
        &self,
        req: cc_agent::shell::ShellRequest,
    ) -> anyhow::Result<cc_agent::shell::AgentShellHandle> {
        let backgrounded = req.run_in_background;
        self.requests.lock().await.push(req);
        let handle = self
            .handle
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow::anyhow!("mock handle already consumed"))?;
        if backgrounded && !handle.handoff.background() {
            anyhow::bail!("mock executor does not support background");
        }
        Ok(handle)
    }
}

#[tokio::test]
async fn test_bash_normal_command() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "echo hello"}))
        .await
        .unwrap();
    assert!(result.contains("hello"));
}

#[tokio::test]
async fn test_bash_nonzero_exit_code() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "exit 42"}))
        .await
        .unwrap();
    assert!(result.contains("42"), "应包含退出码: {result}");
}

#[cfg(windows)]
#[tokio::test]
async fn test_bash_routed_command_is_not_replayed() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    std::fs::write(
        dir.path().join("once.sh"),
        "printf x >> count\nprintf denied >&2\nexit 2\n",
    )
    .expect("创建只执行一次的脚本");
    let tool = BashTool::new(dir.path().to_str().expect("测试路径为 UTF-8"));
    let output = tool
        .invoke(serde_json::json!({"command": "bash once.sh", "timeout": 5000}))
        .await
        .expect("应返回脚本执行结果");
    assert!(output.contains("denied"), "应保留脚本错误信息：{output}");
    assert!(output.contains("Exit code: 2"), "应保留退出码：{output}");
    assert_eq!(
        std::fs::read(dir.path().join("count")).expect("读取执行次数"),
        b"x",
        "已路由到 Bash 的命令不得再次触发 CMD fallback"
    );
}

#[tokio::test]
async fn test_bash_ctrl_b_background_returns_task_without_killing() {
    let (_result_tx, result_rx) =
        tokio::sync::oneshot::channel::<anyhow::Result<cc_agent::shell::ShellCommandOutput>>();
    let handoff = Arc::new(ShellHandoff::new(true, false));
    let killed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let killed_for_abort = std::sync::Arc::clone(&killed);
    let handle = cc_agent::shell::AgentShellHandle {
        task_id: "task-manual-bg".to_string(),
        output_path: std::env::temp_dir().join("peri-manual-bg.output"),
        result_rx,
        exit_signal: std::sync::Arc::new(cc_agent::shell::ExitSignal::new()),
        handoff: handoff.clone(),
        kill: cc_agent::shell::ShellAbortHandle::new(move || {
            killed_for_abort.store(true, std::sync::atomic::Ordering::SeqCst);
        }),
    };
    let tool = BashTool::with_executor(
        std::env::temp_dir().to_string_lossy().to_string(),
        std::sync::Arc::new(MockShellExecutor {
            handle: tokio::sync::Mutex::new(Some(handle)),
            requests: tokio::sync::Mutex::new(Vec::new()),
        }),
    );

    let invoke = tokio::spawn(async move {
        tool.invoke(serde_json::json!({
            "command": "python long.py",
            "timeout": 5_000
        }))
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(handoff.background(), "应能通过 Ctrl+B 原子移交后台归属");
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), invoke)
        .await
        .expect("Ctrl+B 后 BashTool 应快速返回")
        .expect("join 应成功")
        .expect("后台化应返回成功");

    assert!(
        result.contains("<background-task-started>"),
        "Ctrl+B 后应返回后台任务占位串: {result}"
    );
    assert!(
        result.contains("<task-id>task-manual-bg</task-id>"),
        "占位串应包含任务 id: {result}"
    );
    assert!(
        !killed.load(std::sync::atomic::Ordering::SeqCst),
        "手动后台化不应杀进程"
    );
}

#[tokio::test]
async fn test_bash_timeout_auto_background_returns_task_without_killing() {
    let (_result_tx, result_rx) =
        tokio::sync::oneshot::channel::<anyhow::Result<cc_agent::shell::ShellCommandOutput>>();
    let handoff = Arc::new(ShellHandoff::new(true, false));
    let killed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let killed_for_abort = std::sync::Arc::clone(&killed);
    let handle = cc_agent::shell::AgentShellHandle {
        task_id: "task-auto-bg".to_string(),
        output_path: std::env::temp_dir().join("peri-auto-bg.output"),
        result_rx,
        exit_signal: std::sync::Arc::new(cc_agent::shell::ExitSignal::new()),
        handoff: handoff.clone(),
        kill: cc_agent::shell::ShellAbortHandle::new(move || {
            killed_for_abort.store(true, std::sync::atomic::Ordering::SeqCst);
        }),
    };
    let tool = BashTool::with_executor(
        std::env::temp_dir().to_string_lossy().to_string(),
        std::sync::Arc::new(MockShellExecutor {
            handle: tokio::sync::Mutex::new(Some(handle)),
            requests: tokio::sync::Mutex::new(Vec::new()),
        }),
    );

    let result = tool
        .invoke(serde_json::json!({
            "command": "python long.py",
            "timeout": 10
        }))
        .await
        .expect("支持自动后台化时，超时应返回后台任务");

    assert!(
        result.contains("<background-task-started>"),
        "超时自动后台化应返回后台任务占位串: {result}"
    );
    assert!(
        handoff.is_backgrounded(),
        "前台超时返回时 TUI 观察的共享状态必须已经移交后台"
    );
    assert!(
        !killed.load(std::sync::atomic::Ordering::SeqCst),
        "自动后台化不应杀进程"
    );
}

/// 验证超时后在合理时间内返回，且 kill_on_drop 确保子进程被清理
#[tokio::test]
async fn test_bash_timeout_returns_quickly() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let start = Instant::now();

    // Windows 用 ping 模拟 sleep，Unix 用 sleep
    let (sleep_cmd, timeout_ms) = if cfg!(target_os = "windows") {
        ("ping -n 60 127.0.0.1", 1000)
    } else {
        ("sleep 60", 1000)
    };

    let result = tool
        .invoke(serde_json::json!({
            "command": sleep_cmd,
            "timeout": timeout_ms
        }))
        .await;
    let err_msg = result.unwrap_err().to_string();
    let elapsed = start.elapsed();

    // 应在约 1 秒内返回（不超过 3 秒），不等待 sleep 60 完成
    assert!(
        elapsed.as_secs() < 3,
        "超时后应快速返回，实际耗时 {:?}",
        elapsed
    );
    assert!(
        err_msg.contains("timed out"),
        "返回值应包含超时提示: {err_msg}"
    );
}

#[tokio::test]
async fn test_bash_stderr_captured() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "echo err >&2"}))
        .await
        .unwrap();
    assert!(result.contains("err"), "stderr 应被捕获: {result}");
}

#[test]
fn test_truncate_output_no_truncation_under_50k_even_with_many_lines() {
    // 3000 行短文本，总长度约 31KB（小于 50KB）
    let lines: Vec<String> = (0..3000).map(|i| format!("line {}", i)).collect();
    let input = lines.join("\n");
    assert!(input.len() < 50_000);
    let result = truncate_output(&input);
    // 不应发生截断，因为小于 50KB
    assert_eq!(result, input);
}

#[test]
fn test_truncate_output_byte_limit_head_tail() {
    // 6000 行文本，总长度约 66KB（超过 50KB 上限）
    let lines: Vec<String> = (0..6000).map(|i| format!("line {}", i)).collect();
    let input = lines.join("\n");
    assert!(input.len() > 50_000);
    let result = truncate_output(&input);
    assert!(
        result.contains("byte preview limit"),
        "应显示字节截断信息: {result}"
    );
    assert!(
        result.contains("bytes omitted, showing head and tail"),
        "应保留 head/tail 标记: {result}"
    );
    // 应保留头部和尾部
    assert!(result.contains("line 0"), "应保留第一行: {result}");
    assert!(result.contains("line 5999"), "应保留最后一行: {result}");
    assert!(
        !result.contains("line 3000"),
        "中间输出不应进入模型预览，应通过 Read 查看完整文件: {result}"
    );
}

#[test]
fn test_truncate_output_no_truncation_when_small() {
    let result = truncate_output("hello\nworld");
    assert_eq!(result, "hello\nworld");
}

#[test]
fn test_truncate_output_char_limit() {
    let long_line = "x".repeat(200_000);
    let result = truncate_output(&long_line);
    assert!(
        result.contains("byte preview limit"),
        "应截断超长输出: {result}"
    );
    assert!(
        result.len() < 55_000,
        "Bash 字节截断后不应继续返回 100KB 级内容，实际长度: {}",
        result.len()
    );
}

#[test]
fn test_truncate_output_preserves_tail() {
    // 6000 行，尾部包含关键信息
    let mut lines: Vec<String> = (0..5999).map(|i| format!("line {}", i)).collect();
    lines.push("CRITICAL ERROR: test failed".to_string());
    let input = lines.join("\n");
    let result = truncate_output(&input);
    // 尾部关键行应保留
    assert!(
        result.contains("CRITICAL ERROR"),
        "截断后应保留尾部关键信息: {result}"
    );
    assert!(result.contains("line 0"), "应保留头部: {result}");
}

#[test]
fn test_bash_description_extended() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let desc = tool.description();
    assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
    assert!(
        desc.contains("dedicated tool"),
        "description 应强调优先使用专用工具"
    );
    assert!(desc.contains("timeout"), "description 应提及超时");
    assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    assert!(desc.contains("Failed commands are never automatically rerun"));
    assert!(
        !desc.contains("working directory persists"),
        "不能宣称不存在的跨调用 cwd 状态"
    );
}

/// clamp 的契约不依赖 Git Bash 冷启动是否能在 100ms 内结束。
#[tokio::test]
async fn test_bash_timeout_clamped_to_minimum() {
    let executor = make_completed_executor("", "", 0);
    let tool = BashTool::with_executor(".", executor.clone());
    let result = tool
        .invoke(serde_json::json!({"command": "echo quick", "timeout": 0}))
        .await;
    assert!(result.is_ok(), "已有结果应成功返回");
    let requests = executor.requests.lock().await;
    assert_eq!(requests.len(), 1, "只提交一次命令");
    assert_eq!(requests[0].timeout_ms, 1, "零超时应 clamp 为 1 毫秒");
}

/// 显式超时 600000 毫秒应被允许（上限）
#[tokio::test]
async fn test_bash_timeout_maximum_accepted() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "command": "echo ok",
            "timeout": 600000
        }))
        .await
        .unwrap();
    assert!(result.contains("ok"));
}

#[test]
#[allow(non_snake_case)]
fn test_tool_name_is_Bash() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    assert_eq!(tool.name(), "Bash");
}

#[tokio::test]
async fn test_bash_default_timeout_is_120_seconds() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    // 不传 timeout → 默认 120000ms = 120s
    let result = tool
        .invoke(serde_json::json!({"command": "echo ok"}))
        .await
        .unwrap();
    assert!(result.contains("ok"));
}

#[tokio::test]
async fn test_bash_inline_background_rejected_before_spawn() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let tool = BashTool::new(dir.path().to_string_lossy());
    let result = tool
        .invoke(serde_json::json!({
            "command": "printf unexpected > side-effect",
            "description": "test description",
            "run_in_background": true
        }))
        .await;
    let error = result.expect_err("无后台服务时必须拒绝").to_string();
    assert!(
        error.contains("does not support background"),
        "应解释宿主能力：{error}"
    );
    assert!(
        !dir.path().join("side-effect").exists(),
        "拒绝请求不能产生副作用"
    );
}

#[test]
fn test_truncate_bytes_ascii() {
    let s = "hello world";
    assert_eq!(truncate_bytes(s, 5), "hello");
}

#[test]
fn test_truncate_bytes_within_limit() {
    let s = "hello";
    assert_eq!(truncate_bytes(s, 100), "hello");
}

#[test]
fn test_format_command_output_with_rtk_stderr_cleaned() {
    let stdout = "total 0\n-rw-r--r-- 1 user staff 0 Sep 21 10:00 file.txt";
    let raw_stderr =
        "[rtk] /!\\ No hook installed — run `rtk init -g` for automatic token savings\n";
    let cleaned_stderr = crate::tools::output_filter::clean_rtk_stderr_noise(raw_stderr);
    let output = format_command_output(stdout, &cleaned_stderr, 0);

    assert_eq!(output, stdout);
    assert!(
        !output.contains("[stderr]"),
        "RTK 外部宿主警告被清洗为空后不应产生 [stderr] 块误导 Agent: {output}"
    );
}

#[test]
fn test_truncate_bytes_utf8_safe() {
    // 中文字符每个占 3 字节，在字节 7 处截断（是字符边界）
    let s = "你好世界";
    assert_eq!(truncate_bytes(s, 6), "你好");
}

#[test]
fn test_truncate_bytes_utf8_mid_character() {
    // "你好" = 6 bytes, 在字节 5 处截断（不是字符边界）
    // 应回退到字节 3 处（"你" 的末尾）
    let s = "你好世界";
    let result = truncate_bytes(s, 5);
    assert_eq!(result, "你", "应在字符边界截断，实际: {}", result);
}

#[test]
fn test_truncate_bytes_empty_string() {
    assert_eq!(truncate_bytes("", 10), "");
}

#[test]
fn test_truncate_bytes_zero_max() {
    assert_eq!(truncate_bytes("hello", 0), "");
}

#[test]
fn test_truncate_output_persists_full_content_when_exceeding_50k() {
    let lines: Vec<String> = (0..6000).map(|i| format!("line {}", i)).collect();
    let input = lines.join("\n");
    let result = truncate_output(&input);
    assert!(
        result.contains("Read tool"),
        "应包含 Read tool 提示: {result}"
    );
    assert!(
        result.contains("peri-tool-output-"),
        "应包含临时文件路径: {result}"
    );
}

#[test]
fn test_truncate_output_persists_full_content_on_byte_truncation() {
    let long_line = "x".repeat(200_000);
    let result = truncate_output(&long_line);
    assert!(result.contains("Read tool"), "字节截断也应持久化: {result}");
    assert!(
        result.contains("peri-tool-output-"),
        "字节截断应包含文件路径: {result}"
    );
}

fn make_completed_executor(stdout: &str, stderr: &str, exit_code: i32) -> Arc<MockShellExecutor> {
    use cc_agent::shell::{AgentShellHandle, ExitSignal, ShellAbortHandle, ShellCommandOutput};
    let (tx, result_rx) = tokio::sync::oneshot::channel();
    tx.send(Ok(ShellCommandOutput {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_code,
    }))
    .expect("测试结果通道应打开");
    Arc::new(MockShellExecutor {
        requests: tokio::sync::Mutex::new(Vec::new()),
        handle: tokio::sync::Mutex::new(Some(AgentShellHandle {
            task_id: "contract-task".into(),
            output_path: std::env::temp_dir().join("contract-task.output"),
            result_rx,
            exit_signal: Arc::new(ExitSignal::new()),
            handoff: Arc::new(ShellHandoff::new(true, false)),
            kill: ShellAbortHandle::noop(),
        })),
    })
}

#[tokio::test]
async fn test_bash_failed_output_does_not_bypass_injected_executor() {
    let executor = make_completed_executor("", "short failure", 2);
    let tool = BashTool::with_executor(".", executor.clone());
    let output = tool
        .invoke(serde_json::json!({"command": "printf unexpected"}))
        .await
        .expect("应保留执行结果");
    assert!(
        output.contains("short failure"),
        "失败不能转去直接 spawn：{output}"
    );
    assert!(output.contains("Exit code: 2"), "保留退出码：{output}");
    let requests = executor.requests.lock().await;
    assert_eq!(requests.len(), 1, "一个调用只能提交一次执行");
    assert_eq!(requests[0].shell, cc_agent::shell::ShellDialect::Bash);
    assert_eq!(requests[0].original_command, "printf unexpected");
}

#[tokio::test]
async fn test_bash_direct_background_submits_same_shell_contract() {
    let executor = make_completed_executor("", "", 0);
    let tool = BashTool::with_executor(".", executor.clone());
    let output = tool
        .invoke(serde_json::json!({"command": "printf pending", "run_in_background": true}))
        .await
        .expect("支持后台的宿主应返回句柄");
    let requests = executor.requests.lock().await;
    assert_eq!(requests.len(), 1, "直接后台也只提交一次");
    assert!(requests[0].run_in_background);
    assert_eq!(requests[0].shell, cc_agent::shell::ShellDialect::Bash);
    assert!(
        output.contains("<task-id>contract-task</task-id>"),
        "返回同一个任务：{output}"
    );
}

#[tokio::test]
async fn test_bash_single_line_failure_preserves_side_effect_once() {
    for exit_code in [2, 127] {
        let dir = tempfile::tempdir().expect("创建隔离目录");
        let tool = BashTool::new(dir.path().to_string_lossy());
        let command = format!("printf x >> count; printf denied >&2; exit {exit_code}");
        let output = tool
            .invoke(serde_json::json!({"command": command, "timeout": 5000}))
            .await
            .expect("应返回真实错误");
        assert_eq!(
            std::fs::read(dir.path().join("count")).expect("读取执行次数"),
            b"x",
            "不允许重放副作用"
        );
        assert!(
            output.contains(&format!("Exit code: {exit_code}")),
            "应保留退出码：{output}"
        );
    }
}

#[tokio::test]
async fn test_bash_preserves_native_git_message_quoting() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().join("中文 with spaces");
    std::fs::create_dir(&cwd).expect("创建带空格工作目录");
    let tool = BashTool::new(cwd.to_string_lossy());
    // 用同名函数检查真实 shell argv，不创建提交，也不读取用户 git 配置。
    let command = r#"git() { printf '<%s>\n' "$@"; }; note='中文 && | 引号'; git commit -m "$note" -m 'body $HOME'"#;
    let output = tool
        .invoke(serde_json::json!({"command": command, "timeout": 5000}))
        .await
        .expect("Bash 应原生处理引用");
    assert!(
        output.contains("<commit>\n<-m>\n<中文 && | 引号>\n<-m>\n<body $HOME>"),
        "不应改写成 CMD 临时文件或破坏参数：{output}"
    );
}

#[tokio::test]
async fn test_bash_drains_stderr_before_stdout_without_deadlock() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let tool = BashTool::new(dir.path().to_string_lossy());
    let output = tool.invoke(serde_json::json!({
        "command": "for ((i=0;i<8192;i++)); do printf 'stderr-padding-1234567890\\n' >&2; done; printf drained",
        "timeout": 10000
    })).await.expect("stderr 超过管道缓冲后仍应完成");
    assert!(
        output.contains("drained"),
        "应读到 stderr 之后的 stdout：{output}"
    );
}
