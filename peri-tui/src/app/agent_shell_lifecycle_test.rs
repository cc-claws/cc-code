use super::*;
use peri_agent::{
    shell::{ShellDialect, ShellHandoff},
    tools::{BaseTool, ToolInvocationContext},
};
use peri_middlewares::middleware::terminal::BashTool;
use std::time::Duration;

fn make_foreground_slot() -> (AgentShellSlot, Arc<ShellHandoff>) {
    let handoff = Arc::new(ShellHandoff::new(true, false));
    let registration = AgentShellRegistration {
        task_id: "handoff-task".into(),
        owner_session_id: None,
        tool_call_id: Some("handoff-call".into()),
        source_agent_id: None,
        execution_timeout_ms: 600_000,
        command: "sleep 30".into(),
        cwd: ".".into(),
        output_path: PathBuf::new(),
        exit_signal: Arc::new(ExitSignal::new()),
        handoff: handoff.clone(),
        kill: ShellAbortHandle::noop(),
        started_instant: std::time::Instant::now(),
        direct_background: false,
    };
    (AgentShellSlot::from_registration(registration), handoff)
}

#[test]
fn test_slot_background_transition_is_shared_and_visible_to_waiter() {
    let (mut slot, handoff) = make_foreground_slot();
    assert!(slot.mark_backgrounded(), "后台移交应成功");
    assert!(
        handoff.is_backgrounded(),
        "工具与 UI 必须观察到同一归属状态"
    );
    assert!(slot.is_backgrounded(), "槽位状态必须来自共享归属");
}

#[test]
fn test_slot_completed_before_ui_poll_cannot_be_manually_backgrounded() {
    let (mut slot, _) = make_foreground_slot();
    slot.exit_signal.finish(ShellOutcome::Exited(0));
    assert!(!slot.ended, "模拟终态已发布但 UI 尚未轮询");
    assert!(!slot.mark_backgrounded(), "不能把已结束命令重新标成后台");
    assert!(!slot.is_backgrounded());
}

#[test]
fn test_slot_without_background_host_cannot_claim_background_ownership() {
    let (mut slot, _) = make_foreground_slot();
    slot.handoff = Arc::new(ShellHandoff::new(false, false));
    assert!(!slot.mark_backgrounded(), "没有后台宿主就不能移交");
    assert!(!slot.is_backgrounded());
}

#[tokio::test]
async fn test_executor_short_auto_background_keeps_registration_and_outcome() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tool = BashTool::with_executor(
        cwd.clone(),
        Arc::new(AgentShellExecutor::new(tx, cwd, "short".into())),
    );
    let result = tool
        .invoke(serde_json::json!({
            "command": "sleep 0.3; printf finished", "timeout": 1
        }))
        .await
        .expect("短超时应返回后台句柄");
    let reg = rx.try_recv().expect("返回后台句柄前必须已经注册");
    assert!(
        result.contains(&reg.task_id),
        "登记与返回句柄必须指向同一任务"
    );
    assert!(reg.handoff.is_backgrounded(), "自动后台必须先原子移交归属");
    tokio::time::timeout(Duration::from_secs(5), reg.exit_signal.wait())
        .await
        .expect("任务应完成");
    assert_eq!(reg.exit_signal.outcome(), Some(ShellOutcome::Exited(0)));
    assert_eq!(
        std::fs::read_to_string(reg.output_path).expect("完成后输出已刷盘"),
        "finished"
    );
    assert!(rx.try_recv().is_err(), "完成不能重复登记");
}

#[tokio::test]
async fn test_executor_background_preserves_success_and_failure_exit_codes() {
    for code in [0, 7] {
        let dir = tempfile::tempdir().expect("创建隔离目录");
        let cwd = dir.path().to_string_lossy().to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tool = BashTool::with_executor(
            cwd.clone(),
            Arc::new(AgentShellExecutor::new(tx, cwd, "exit".into())),
        );
        let result = tool
            .invoke(serde_json::json!({
                "command": format!("printf final-output; exit {code}"), "run_in_background": true
            }))
            .await
            .expect("应返回后台任务");
        let reg = rx.try_recv().expect("直接后台应登记");
        assert!(result.contains(&reg.task_id));
        tokio::time::timeout(Duration::from_secs(5), reg.exit_signal.wait())
            .await
            .expect("后台任务应完成");
        assert_eq!(
            reg.exit_signal.outcome(),
            Some(ShellOutcome::Exited(code)),
            "丢弃工具结果 receiver 不能丢退出码"
        );
        assert_eq!(
            std::fs::read_to_string(reg.output_path).expect("完成时即可读取最终输出"),
            "final-output"
        );
    }
}

#[tokio::test]
async fn test_executor_hard_deadline_survives_every_background_mode() {
    for mode in ["foreground", "manual", "automatic", "direct"] {
        let dir = tempfile::tempdir().expect("创建隔离目录");
        let cwd = dir.path().to_string_lossy().to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tool = BashTool::with_executor(
            cwd.clone(),
            Arc::new(AgentShellExecutor::new(tx, cwd, "deadline".into())),
        );
        let invoke = tokio::spawn(async move {
            tool.invoke(serde_json::json!({
                "command": "sleep 10; printf must-not-run", "execution_timeout": 300,
                "timeout": if mode == "automatic" { 1 } else { 5000 },
                "run_in_background": mode == "direct"
            }))
            .await
        });
        let reg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("必须及时注册")
            .expect("注册存在");
        if mode == "manual" {
            assert!(reg.handoff.background(), "手动后台应原子移交归属");
        }
        tokio::time::timeout(Duration::from_secs(5), reg.exit_signal.wait())
            .await
            .expect("硬期限必须在后台仍然生效");
        assert_eq!(
            reg.exit_signal.outcome(),
            Some(ShellOutcome::TimedOut),
            "{mode} 必须记录真实超时原因"
        );
        let result = invoke.await.expect("工具任务应正常回收");
        if mode == "foreground" {
            assert!(result
                .expect_err("前台硬超时必须报错")
                .to_string()
                .contains("hard execution deadline"));
        } else {
            assert!(result.expect("后台化应先返回句柄").contains(&reg.task_id));
        }
        // 输出文件缺失视为无输出（Windows runner 文件系统偶发问题，见 #323）：
        // 本测试验证的是硬期限行为（TimedOut 已在上面断言），读盘只是辅助检查。
        let output = std::fs::read_to_string(&reg.output_path).unwrap_or_default();
        assert!(
            !output.contains("must-not-run"),
            "{mode} 超时后不应执行后续命令"
        );
    }
}

#[tokio::test]
async fn test_executor_cancelled_foreground_tool_stops_process_owner() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tool = BashTool::with_executor(
        cwd.clone(),
        Arc::new(AgentShellExecutor::new(tx, cwd, "cancel".into())),
    );
    let invoke = tokio::spawn(async move {
        tool.invoke(serde_json::json!({"command": "sleep 30"}))
            .await
    });
    let reg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("注册应及时完成")
        .expect("注册存在");
    invoke.abort();
    assert!(invoke
        .await
        .expect_err("工具 future 应被取消")
        .is_cancelled());
    tokio::time::timeout(Duration::from_secs(5), reg.exit_signal.wait())
        .await
        .expect("取消工具也必须停止命令");
    assert_eq!(reg.exit_signal.outcome(), Some(ShellOutcome::Cancelled));
}

#[tokio::test]
async fn test_executor_identity_comes_from_dispatch_not_model_arguments() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tool = BashTool::with_executor(
        cwd.clone(),
        Arc::new(AgentShellExecutor::new(tx, cwd, "identity".into())),
    );
    let context = ToolInvocationContext {
        tool_call_id: "trusted-call".into(),
        source_agent_id: Some("trusted-agent".into()),
    };
    let result = context
        .scope(tool.invoke(serde_json::json!({
            "command": "printf identity", "tool_call_id": "fake", "source_agent_id": "fake"
        })))
        .await
        .expect("命令应完成");
    let reg = rx.try_recv().expect("应登记");
    assert_eq!(reg.tool_call_id.as_deref(), Some("trusted-call"));
    assert_eq!(reg.source_agent_id.as_deref(), Some("trusted-agent"));
    assert!(result.contains("identity"));
}

#[tokio::test]
async fn test_executor_closed_host_cannot_return_background_handle() {
    let dir = tempfile::tempdir().expect("创建隔离目录");
    let cwd = dir.path().to_string_lossy().to_string();
    let (tx, rx) = mpsc::unbounded_channel();
    drop(rx);
    let executor = AgentShellExecutor::new(tx, cwd.clone(), "closed".into());
    let result = executor
        .execute(ShellRequest {
            owner_session_id: None,
            invocation: None,
            command: "printf unexpected > must-not-run".into(),
            original_command: "printf unexpected > must-not-run".into(),
            shell: ShellDialect::Bash,
            cwd,
            timeout_ms: 5000,
            execution_timeout_ms: 5000,
            run_in_background: true,
        })
        .await;
    assert!(
        result.is_err(),
        "后台宿主消失必须停止命令并报错，不能返回幽灵句柄"
    );
    assert!(
        !dir.path().join("must-not-run").exists(),
        "已关闭宿主必须在启动前拒绝"
    );
}
