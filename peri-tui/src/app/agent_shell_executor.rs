//! Agent shell 执行器：peri-tui 的 [`ShellExecutor`] 实现。
//!
//! 把 agent 的 Bash 工具命令接入流式执行 + 磁盘输出，使其可被 Ctrl+B 后台化。
//! 设计对齐 Claude Code（参见 docs/ctrl-b-background-shell.html）：
//! - 命令始终经 shell 执行，stdout/stderr 写入磁盘（`DiskOutput`）
//! - `BashTool::invoke` 前台等待结果；后台移交后返回同一任务句柄，进程继续运行
//! - UI 主循环通过 [`AgentShellRegistration`] channel 接收注册事件，把命令登记到
//!   `agent_shells` 槽位以响应 Ctrl+B（后台化不重新启动进程，也不重置硬期限）
//! - 退出检测走独立的 [`ExitSignal`]（invoke 独占 result_rx，UI poll 查 exit_signal）
//!
//! # oneshot 单消费者矛盾的解法
//!
//! `BashTool::invoke` 与 UI poll 都需"进程退出"信号，但 `tokio::oneshot` 单消费者。
//! 解法：invoke 独占 `result_rx`（拿完整 [`ShellCommandOutput`]）；一个 wrapper task
//! `await` 真正的 `execution.result`，解析后同时：
//! 1. 等输出写盘，`exit_signal.finish(outcome)` → 向 UI 发布真实终态
//! 2. `result_tx.send(output)` → 唤醒仍在等待的 invoke
//!
//! UI 不碰 `result_rx`，后台化时也不 take 它，避免与 invoke 的 await 冲突。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::shell::{
    AgentShellHandle, ExitSignal, ShellAbortHandle, ShellCommandOutput, ShellExecutor,
    ShellHandoff, ShellOutcome, ShellRequest,
};
use tokio::sync::{mpsc, oneshot};

use crate::shell_exec::{execute_shell_command_streaming_with_timeout, ShellExecutionTimedOut};

/// agent 前台 shell 注册信息：由 [`AgentShellExecutor::execute`] 通过 channel
/// 发送给 App 主循环，用于登记到 `agent_shells` 槽位以响应 Ctrl+B。
///
/// 注意：`result_rx` 不在此处（它由 invoke 独占 await）；这里只含 UI 控制所需的
/// 副本（exit_signal 与 handoff 均为 Arc 共享状态）。
pub struct AgentShellRegistration {
    pub task_id: String,
    pub owner_session_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub source_agent_id: Option<String>,
    pub execution_timeout_ms: u64,
    pub command: String,
    pub cwd: String,
    pub output_path: PathBuf,
    /// UI poll 查退出（独立于 invoke 的 result_rx）
    pub exit_signal: Arc<ExitSignal>,
    pub handoff: Arc<ShellHandoff>,
    /// 杀进程（UI 详情面板 `x` 键）
    pub kill: ShellAbortHandle,
    pub started_instant: std::time::Instant,
    /// true = 直接后台启动；false = 前台启动（可 Ctrl+B），都登记到 agent_shells。
    pub direct_background: bool,
}

/// App 侧持有的 agent shell 跟踪槽位（前台 + 后台共用）。
///
/// 由 [`AgentShellRegistration`] 转换而来。区别于用户 `!command` 路径的
/// [`super::ShellCommandPool`] / [`super::BackgroundShell`]：agent 路径下
/// `result_rx` 由 `BashTool::invoke` 独占 await（拿完整 stdout），UI 只用
/// [`ExitSignal`] 检测退出 + [`ShellHandoff`] 仲裁 Ctrl+B 与取消。
///
/// 生命周期：
/// 1. 前台注册（direct_background=false）→ push 到 `agent_shells`，is_backgrounded=false
/// 2. 用户按 Ctrl+B → is_backgrounded=true，启动 stall watchdog（输出已全程写磁盘）
/// 3. 进程退出（exit_signal 触发）→ mark_ended + 注入完成通知
pub struct AgentShellSlot {
    pub task_id: String,
    pub owner_session_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub source_agent_id: Option<String>,
    pub execution_timeout_ms: u64,
    pub command: String,
    pub cwd: PathBuf,
    pub output_path: PathBuf,
    /// 退出检测信号（poll 用，独立于 invoke 的 result_rx）。
    pub exit_signal: Arc<ExitSignal>,
    pub handoff: Arc<ShellHandoff>,
    /// 杀进程句柄（详情面板 `x` 键）。
    pub kill: ShellAbortHandle,
    pub started_instant: std::time::Instant,
    /// 是否已退出完成（避免重复通知）。
    pub ended: bool,
    /// 完成通知只交付给原会话一次；切换会话不丢弃进程归属。
    pub completion_notified: bool,
    /// 完整终态持久保留，即使工具 receiver 已关闭也可供 UI/通知消费。
    pub outcome: Option<ShellOutcome>,
    /// 退出码（退出后设置）。
    pub exit_code: Option<i32>,
    /// 结束时间点（结束后冻结 elapsed）。
    pub ended_at: Option<std::time::Instant>,
    /// stall watchdog task（后台化时启动）。
    pub stall_watchdog: Option<tokio::task::JoinHandle<()>>,
}

impl AgentShellSlot {
    /// 由注册信息构造前台槽位。
    pub fn from_registration(reg: AgentShellRegistration) -> Self {
        Self {
            task_id: reg.task_id,
            owner_session_id: reg.owner_session_id,
            tool_call_id: reg.tool_call_id,
            source_agent_id: reg.source_agent_id,
            execution_timeout_ms: reg.execution_timeout_ms,
            command: reg.command,
            cwd: PathBuf::from(reg.cwd),
            output_path: reg.output_path,
            exit_signal: reg.exit_signal,
            handoff: reg.handoff,
            kill: reg.kill,
            started_instant: reg.started_instant,
            ended: false,
            completion_notified: false,
            outcome: None,
            exit_code: None,
            ended_at: None,
            stall_watchdog: None,
        }
    }

    /// 是否仍在前台运行（可被 Ctrl+B 后台化）。
    pub fn is_foreground_running(&self) -> bool {
        !self.ended && self.handoff.is_foreground_pending() && !self.exit_signal.is_exited()
    }

    pub fn is_backgrounded(&self) -> bool {
        self.handoff.is_backgrounded()
    }

    pub fn belongs_to(&self, session_id: Option<&str>) -> bool {
        self.owner_session_id
            .as_deref()
            .is_none_or(|owner| Some(owner) == session_id)
    }

    /// 已运行时长（结束后冻结为 ended_at - started_instant）。
    pub fn elapsed(&self) -> std::time::Duration {
        match self.ended_at {
            Some(end) => end.saturating_duration_since(self.started_instant),
            None => self.started_instant.elapsed(),
        }
    }

    /// 共享归属先完成移交，再唤醒等待者；取消与移交只能一个获得前台权。
    pub fn mark_backgrounded(&mut self) -> bool {
        if !self.is_foreground_running() {
            return false;
        }
        self.handoff.background()
    }

    /// 标记结束（poll 检测到 exit_signal 后调用）。
    pub fn mark_ended(&mut self, outcome: ShellOutcome) {
        self.ended = true;
        self.exit_code = match &outcome {
            ShellOutcome::Exited(code) => Some(*code),
            _ => None,
        };
        self.outcome = Some(outcome);
        self.ended_at = Some(std::time::Instant::now());
        // 终止 stall watchdog
        if let Some(w) = self.stall_watchdog.take() {
            w.abort();
        }
    }
}

/// peri-tui 的 [`ShellExecutor`] 实现。
///
/// 持有一个 mpsc sender，把每个命令的 [`AgentShellRegistration`] 发给 App 主循环。
/// App 在 `poll_agent_foreground_shells` 之外的某处 `recv` 这些注册（见接入点）。
pub struct AgentShellExecutor {
    /// 注册事件 channel：executor → App 主循环。
    registration_tx: mpsc::UnboundedSender<AgentShellRegistration>,
    /// 当前会话的 session_id（用于构造 DiskOutput 路径）。
    /// 因 ACP server 在独立 task 运行，构造 executor 时快照。
    session_id: String,
}

impl AgentShellExecutor {
    /// 创建执行器。`cwd` 参数保留以备未来按会话区分（当前 DiskOutput 路径用 session_id）。
    pub fn new(
        registration_tx: mpsc::UnboundedSender<AgentShellRegistration>,
        _cwd: String,
        session_id: String,
    ) -> Self {
        Self {
            registration_tx,
            session_id,
        }
    }
}

#[async_trait]
impl ShellExecutor for AgentShellExecutor {
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle> {
        anyhow::ensure!(
            !self.registration_tx.is_closed(),
            "Background command host is no longer available; no command was executed"
        );
        let ShellRequest {
            owner_session_id,
            invocation,
            command,
            original_command,
            shell,
            cwd,
            run_in_background,
            execution_timeout_ms,
            ..
        } = req;

        let task_id = uuid::Uuid::now_v7().to_string();
        let cwd_path = PathBuf::from(&cwd);
        let output_path = peri_agent::task_output::task_output_path(
            &task_id,
            &cwd_path,
            owner_session_id.as_deref().unwrap_or(&self.session_id),
        );

        // 流式执行：stdout/stderr 合并推送 output_rx，进程退出 result 在 execution.result。
        let execution = execute_shell_command_streaming_with_timeout(
            &command,
            &cwd,
            None,
            shell,
            std::time::Duration::from_millis(execution_timeout_ms),
        )?;

        // output_rx 全程写磁盘（agent 路径不显示在 UI 输出流，仅写磁盘供详情面板 / 通知读取）。
        // 与 !command 路径不同：那条路径 output_rx 由 App drain 丢弃；本路径交给 DiskOutput。
        let output_writer = peri_agent::task_output::DiskOutput::spawn_writer(
            output_path.clone(),
            execution.output_rx,
        );
        let started_instant = execution.started_instant;
        let process_abort = execution.abort.clone();

        // 真正的进程退出信号在 execution.result（peri-tui 的 oneshot）。
        // 我们包一层：await 它 → 转换为 ShellCommandOutput → 同时发给 invoke 的
        // result_rx 和触发 exit_signal（解决 oneshot 单消费者矛盾）。
        let (result_tx, result_rx) = oneshot::channel::<anyhow::Result<ShellCommandOutput>>();
        let exit_signal = Arc::new(ExitSignal::new());
        let exit_signal_clone = Arc::clone(&exit_signal);
        let handoff = Arc::new(ShellHandoff::new(true, run_in_background));

        let real_result = execution.result;
        tokio::spawn(async move {
            let real = real_result.await;
            let (converted, outcome) = match real {
                Ok(Ok(out)) => {
                    let outcome = ShellOutcome::Exited(out.exit_code);
                    (
                        Ok(ShellCommandOutput {
                            stdout: out.stdout,
                            stderr: out.stderr,
                            exit_code: out.exit_code,
                        }),
                        outcome,
                    )
                }
                Ok(Err(e)) => {
                    let outcome = if e.downcast_ref::<ShellExecutionTimedOut>().is_some() {
                        ShellOutcome::TimedOut
                    } else {
                        ShellOutcome::Failed(e.to_string())
                    };
                    (Err(e), outcome)
                }
                Err(_) => (
                    Err(anyhow::anyhow!("Command was cancelled")),
                    ShellOutcome::Cancelled,
                ),
            };
            // 完成通知可立即读盘；必须先等输出写入结束。
            let _ = output_writer.await;
            exit_signal_clone.finish(outcome);
            let _ = result_tx.send(converted);
        });

        // 注册到 App 主循环。
        // - run_in_background=true：立即注册为后台任务。
        // - 普通前台命令同样立即注册，2 秒仅控制 UI 提示，不控制任务所有权。
        let registration = AgentShellRegistration {
            task_id: task_id.clone(),
            owner_session_id,
            tool_call_id: invocation.as_ref().map(|ctx| ctx.tool_call_id.clone()),
            source_agent_id: invocation.and_then(|ctx| ctx.source_agent_id),
            execution_timeout_ms,
            command: original_command,
            cwd: cwd.clone(),
            output_path: output_path.clone(),
            exit_signal: Arc::clone(&exit_signal),
            handoff: Arc::clone(&handoff),
            kill: process_abort.clone(),
            started_instant,
            direct_background: run_in_background,
        };
        if self.registration_tx.send(registration).is_err() {
            process_abort.abort();
            exit_signal.wait().await;
            anyhow::bail!("Background command host is no longer available; command stopped");
        }

        Ok(AgentShellHandle {
            task_id,
            output_path,
            result_rx,
            exit_signal,
            handoff,
            kill: process_abort,
        })
    }
}

#[cfg(test)]
#[path = "agent_shell_contract_test.rs"]
mod contract_tests;

#[cfg(test)]
#[path = "agent_shell_lifecycle_test.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个用于测试的 AgentShellRegistration。
    fn make_reg(direct_background: bool) -> AgentShellRegistration {
        let handoff = Arc::new(ShellHandoff::new(true, direct_background));
        AgentShellRegistration {
            task_id: "test-task".to_string(),
            owner_session_id: None,
            tool_call_id: None,
            source_agent_id: None,
            execution_timeout_ms: 600_000,
            command: "echo hi".to_string(),
            cwd: "/tmp".to_string(),
            output_path: PathBuf::from("/tmp/out.log"),
            exit_signal: Arc::new(ExitSignal::new()),
            handoff,
            kill: ShellAbortHandle::noop(),
            started_instant: std::time::Instant::now(),
            direct_background,
        }
    }

    #[tokio::test]
    async fn test_slot_foreground_running_initially() {
        let reg = make_reg(false);
        let slot = AgentShellSlot::from_registration(reg);
        assert!(slot.is_foreground_running(), "前台注册后应处于前台运行中");
        assert!(!slot.is_backgrounded());
        assert!(!slot.ended);
    }

    #[tokio::test]
    async fn test_slot_direct_background_not_foreground() {
        let reg = make_reg(true);
        let slot = AgentShellSlot::from_registration(reg);
        assert!(
            !slot.is_foreground_running(),
            "direct_background 的槽位不应处于前台"
        );
        assert!(slot.is_backgrounded(), "direct_background 应标记为已后台化");
    }

    #[tokio::test]
    async fn test_slot_mark_backgrounded_updates_shared_ownership() {
        let reg = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        assert!(slot.mark_backgrounded(), "首次后台化应成功");
        assert!(slot.is_backgrounded());
        // 重复后台化返回 false
        assert!(!slot.mark_backgrounded(), "已后台化的重复调用应返回 false");
    }

    #[tokio::test]
    async fn test_slot_mark_backgrounded_after_ended_fails() {
        let reg = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        slot.mark_ended(ShellOutcome::Exited(0));
        assert!(!slot.mark_backgrounded(), "已退出的槽位后台化应返回 false");
    }

    #[tokio::test]
    async fn test_slot_mark_ended_sets_exit_code() {
        let reg = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        slot.mark_ended(ShellOutcome::Exited(42));
        assert!(slot.ended);
        assert_eq!(slot.exit_code, Some(42));
    }

    #[tokio::test]
    async fn test_slot_elapsed_freezes_after_ended() {
        let reg = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        slot.mark_ended(ShellOutcome::Exited(0));
        let elapsed_after_end = slot.elapsed();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let elapsed_later = slot.elapsed();
        assert_eq!(
            elapsed_after_end, elapsed_later,
            "mark_ended 后 elapsed 应冻结，不再增长"
        );
    }

    #[test]
    fn test_exit_signal_fire_and_check() {
        let signal = ExitSignal::new();
        assert!(!signal.is_exited(), "新建信号不应为 exited");
        signal.fire();
        assert!(signal.is_exited(), "fire 后应为 exited");
    }

    #[tokio::test]
    async fn test_exit_signal_wait_after_fire() {
        let signal = ExitSignal::new();
        signal.fire();
        // fire 之后 wait 应立即返回（不挂起）
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), signal.wait()).await;
        assert!(result.is_ok(), "fire 后 wait 不应超时挂起");
    }

    fn test_cwd() -> String {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    fn quick_command() -> &'static str {
        if cfg!(windows) {
            "echo quick"
        } else {
            "printf quick"
        }
    }

    fn slow_command() -> &'static str {
        if cfg!(windows) {
            // 用 ping 模拟 sleep：powershell 冷启动在 CI 机器上可能耗时数秒，
            // 导致「长前台命令」测试的结果等待超时（Elapsed）
            "ping -n 2 127.0.0.1 >nul & echo done"
        } else {
            "sleep 0.5; printf done"
        }
    }

    #[tokio::test]
    async fn test_executor_short_lived_foreground_command_registers_immediately() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let executor = AgentShellExecutor::new(tx, test_cwd(), "test-session".to_string());
        let handle = executor
            .execute(ShellRequest {
                owner_session_id: None,
                invocation: None,
                command: quick_command().to_string(),
                original_command: quick_command().to_string(),
                shell: peri_agent::shell::ShellDialect::PlatformDefault,
                cwd: test_cwd(),
                timeout_ms: 5_000,
                execution_timeout_ms: 600_000,
                run_in_background: false,
            })
            .await
            .unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), handle.result_rx)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result.exit_code, 0, "短命命令应正常退出");
        let registration = rx.try_recv().expect("短命令也必须登记，显示门槛由 UI 决定");
        assert_eq!(
            registration.exit_signal.outcome(),
            Some(ShellOutcome::Exited(0))
        );
    }

    #[tokio::test]
    async fn test_executor_long_foreground_command_registers_before_completion() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let executor = AgentShellExecutor::new(tx, test_cwd(), "test-session".to_string());
        let handle = executor
            .execute(ShellRequest {
                owner_session_id: None,
                invocation: None,
                command: slow_command().to_string(),
                original_command: slow_command().to_string(),
                shell: peri_agent::shell::ShellDialect::PlatformDefault,
                cwd: test_cwd(),
                timeout_ms: 5_000,
                execution_timeout_ms: 600_000,
                run_in_background: false,
            })
            .await
            .unwrap();
        let registration = rx.try_recv().expect("返回句柄前必须登记长命令");
        assert!(
            !registration.direct_background,
            "前台命令不应标记为直接后台"
        );
        // CI 慢机器（尤其 Windows）上命令总耗时可能超过 5 秒，放宽等待
        let result = tokio::time::timeout(std::time::Duration::from_secs(15), handle.result_rx)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result.exit_code, 0, "长命令应正常退出");
    }
}
