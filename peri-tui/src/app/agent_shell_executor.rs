//! Agent shell 执行器：peri-tui 的 [`ShellExecutor`] 实现。
//!
//! 把 agent 的 Bash 工具命令接入流式执行 + 磁盘输出，使其可被 Ctrl+B 后台化。
//! 设计对齐 Claude Code（参见 docs/ctrl-b-background-shell.html）：
//! - 命令始终经 shell 执行，stdout/stderr 写入磁盘（`DiskOutput`）
//! - [`BashTool::invoke`] 通过 `result_rx` await 到进程退出拿完整 stdout（后台化不抢占）
//! - UI 主循环通过 [`AgentShellRegistration`] channel 接收注册事件，把命令登记到
//!   `agent_foreground_shells` 槽位以响应 Ctrl+B（后台化只切 UI 状态，进程不中断）
//! - 退出检测走独立的 [`ExitSignal`]（invoke 独占 result_rx，UI poll 查 exit_signal）
//!
//! # oneshot 单消费者矛盾的解法
//!
//! [`BashTool::invoke`] 与 UI poll 都需"进程退出"信号，但 `tokio::oneshot` 单消费者。
//! 解法：invoke 独占 `result_rx`（拿完整 [`ShellCommandOutput`]）；一个 wrapper task
//! `await` 真正的 `execution.result`，解析后同时：
//! 1. `result_tx.send(output)` → 唤醒 invoke
//! 2. `exit_signal.fire()` → 唤醒 UI poll
//!
//! UI 不碰 `result_rx`，后台化时也不 take 它，避免与 invoke 的 await 冲突。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::shell::{
    AgentShellHandle, ExitSignal, ShellCommandOutput, ShellExecutor, ShellRequest,
};
use tokio::sync::{mpsc, oneshot};

use crate::shell_exec::execute_shell_command_streaming;

/// agent 前台 shell 注册信息：由 [`AgentShellExecutor::execute`] 通过 channel
/// 发送给 App 主循环，用于登记到 `agent_foreground_shells` 槽位以响应 Ctrl+B。
///
/// 注意：`result_rx` 不在此处（它由 invoke 独占 await）；这里只含 UI 控制所需的
/// 副本（exit_signal 是 Arc 可共享，background_tx 是 oneshot 仅 UI 持有）。
pub struct AgentShellRegistration {
    pub task_id: String,
    pub command: String,
    pub cwd: String,
    pub output_path: PathBuf,
    /// UI poll 查退出（独立于 invoke 的 result_rx）
    pub exit_signal: Arc<ExitSignal>,
    /// Ctrl+B 时 UI 发送此信号请求后台化；spawn 进程 task 收到后无需动作
    /// （输出已全程写磁盘，后台化只切 UI 状态 + 启动 stall watchdog）
    /// None 表示该任务直接以后台模式启动（run_in_background=true），无需 Ctrl+B。
    pub background_tx: Option<oneshot::Sender<()>>,
    /// 杀进程（UI 详情面板 `x` 键）
    pub kill: tokio::task::AbortHandle,
    pub started_instant: std::time::Instant,
    /// true = 直接后台启动（LLM 传 run_in_background）；登记到 background_shells。
    /// false = 前台启动（可 Ctrl+B）；登记到 agent_foreground_shells。
    pub direct_background: bool,
}

/// App 侧持有的 agent shell 跟踪槽位（前台 + 后台共用）。
///
/// 由 [`AgentShellRegistration`] 转换而来。区别于用户 `!command` 路径的
/// [`super::ShellCommandPool`] / [`super::BackgroundShell`]：agent 路径下
/// `result_rx` 由 `BashTool::invoke` 独占 await（拿完整 stdout），UI 只用
/// [`ExitSignal`] 检测退出 + [`Self::background_tx`] 响应 Ctrl+B。
///
/// 生命周期：
/// 1. 前台注册（direct_background=false）→ push 到 `agent_shells`，is_backgrounded=false
/// 2. 用户按 Ctrl+B → is_backgrounded=true，启动 stall watchdog（输出已全程写磁盘）
/// 3. 进程退出（exit_signal 触发）→ mark_ended + 注入完成通知
pub struct AgentShellSlot {
    pub task_id: String,
    pub command: String,
    pub cwd: PathBuf,
    pub output_path: PathBuf,
    /// 退出检测信号（poll 用，独立于 invoke 的 result_rx）。
    pub exit_signal: Arc<ExitSignal>,
    /// Ctrl+B 后台化信号（前台时存在；后台化或退出后 None）。
    pub background_tx: Option<oneshot::Sender<()>>,
    /// 杀进程句柄（详情面板 `x` 键）。
    pub kill: tokio::task::AbortHandle,
    pub started_instant: std::time::Instant,
    /// 是否已后台化（true = 不再占前台、显示在后台面板；false = 前台运行中可被 Ctrl+B）。
    pub is_backgrounded: bool,
    /// 是否已退出完成（避免重复通知）。
    pub ended: bool,
    /// 退出码（退出后设置）。
    pub exit_code: Option<i32>,
    /// stall watchdog task（后台化时启动）。
    pub stall_watchdog: Option<tokio::task::JoinHandle<()>>,
}

impl AgentShellSlot {
    /// 由注册信息构造前台槽位。
    pub fn from_registration(reg: AgentShellRegistration) -> Self {
        let is_backgrounded = reg.direct_background;
        Self {
            task_id: reg.task_id,
            command: reg.command,
            cwd: PathBuf::from(reg.cwd),
            output_path: reg.output_path,
            exit_signal: reg.exit_signal,
            background_tx: reg.background_tx,
            kill: reg.kill,
            started_instant: reg.started_instant,
            is_backgrounded,
            ended: false,
            exit_code: None,
            stall_watchdog: None,
        }
    }

    /// 是否仍在前台运行（可被 Ctrl+B 后台化）。
    pub fn is_foreground_running(&self) -> bool {
        !self.ended && !self.is_backgrounded
    }

    /// 已运行时长。
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_instant.elapsed()
    }

    /// 标记后台化：发送 background 信号 + 置标志。
    /// 返回是否成功（已后台化或已退出返回 false）。
    pub fn mark_backgrounded(&mut self) -> bool {
        if self.is_backgrounded || self.ended {
            return false;
        }
        self.is_backgrounded = true;
        // 发送后台信号（进程 task 收到后无动作；输出已全程写磁盘）
        if let Some(tx) = self.background_tx.take() {
            let _ = tx.send(());
        }
        true
    }

    /// 标记结束（poll 检测到 exit_signal 后调用）。
    pub fn mark_ended(&mut self, exit_code: Option<i32>) {
        self.ended = true;
        self.exit_code = exit_code;
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
        let ShellRequest {
            command,
            cwd,
            run_in_background,
            ..
        } = req;

        let task_id = uuid::Uuid::now_v7().to_string();
        let cwd_path = PathBuf::from(&cwd);
        let output_path =
            peri_agent::task_output::task_output_path(&task_id, &cwd_path, &self.session_id);

        // 流式执行：stdout/stderr 合并推送 output_rx，进程退出 result 在 execution.result。
        let execution = execute_shell_command_streaming(&command, &cwd, None);

        // output_rx 全程写磁盘（agent 路径不显示在 UI 输出流，仅写磁盘供详情面板 / 通知读取）。
        // 与 !command 路径不同：那条路径 output_rx 由 App drain 丢弃；本路径交给 DiskOutput。
        peri_agent::task_output::DiskOutput::spawn_writer(
            output_path.clone(),
            execution.output_rx,
        );

        // 真正的进程退出信号在 execution.result（peri-tui 的 oneshot）。
        // 我们包一层：await 它 → 转换为 ShellCommandOutput → 同时发给 invoke 的
        // result_rx 和触发 exit_signal（解决 oneshot 单消费者矛盾）。
        let (result_tx, result_rx) = oneshot::channel::<anyhow::Result<ShellCommandOutput>>();
        let exit_signal = Arc::new(ExitSignal::new());
        let exit_signal_clone = Arc::clone(&exit_signal);
        let (background_tx, background_rx) = oneshot::channel::<()>();

        let mut real_result = execution.result;
        let join = tokio::spawn(async move {
            // background_rx：后台化请求信号。agent 路径下输出已全程写磁盘，
            // 后台化只是 UI 状态切换，进程 task 无需动作。这里仅消费以避免泄漏。
            drop(background_rx);

            let real = (&mut real_result).await;
            let converted = match real {
                Ok(Ok(out)) => Ok(ShellCommandOutput {
                    stdout: out.stdout,
                    stderr: out.stderr,
                    exit_code: out.exit_code,
                }),
                Ok(Err(e)) => Err(e), // anyhow::Error
                Err(_) => Err(anyhow::anyhow!(
                    "Command executor closed unexpectedly (process task dropped)"
                )),
            };
            // 进程已退出：唤醒 invoke（拿完整输出）+ UI poll（知退出）。
            let _ = result_tx.send(converted);
            exit_signal_clone.fire();
        });

        // 注册到 App 主循环。前台命令登记到 agent_foreground_shells（可 Ctrl+B）；
        // run_in_background=true 直接登记到 background_shells（invoke 会立即返回占位串）。
        let registration = AgentShellRegistration {
            task_id: task_id.clone(),
            command: command.clone(),
            cwd: cwd.clone(),
            output_path: output_path.clone(),
            exit_signal: Arc::clone(&exit_signal),
            background_tx: if run_in_background { None } else { Some(background_tx) },
            kill: execution.abort,
            started_instant: std::time::Instant::now(),
            direct_background: run_in_background,
        };
        // channel 发送失败（App 已退出）不影响命令执行本身——仅 UI 不显示。
        let _ = self.registration_tx.send(registration);

        Ok(AgentShellHandle {
            task_id,
            output_path,
            result_rx,
            exit_signal,
            background_tx: None, // 已在 registration 里交给了 UI；handle 不再持有
            kill: join.abort_handle(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个用于测试的 AgentShellRegistration（前台、含 background_tx）。
    fn make_reg(direct_background: bool) -> (AgentShellRegistration, oneshot::Receiver<()>) {
        let (bg_tx, bg_rx) = oneshot::channel();
        let reg = AgentShellRegistration {
            task_id: "test-task".to_string(),
            command: "echo hi".to_string(),
            cwd: "/tmp".to_string(),
            output_path: PathBuf::from("/tmp/out.log"),
            exit_signal: Arc::new(ExitSignal::new()),
            background_tx: if direct_background { None } else { Some(bg_tx) },
            kill: tokio::spawn(async {}).abort_handle(),
            started_instant: std::time::Instant::now(),
            direct_background,
        };
        (reg, bg_rx)
    }

    #[tokio::test]
    async fn test_slot_foreground_running_initially() {
        let (reg, _rx) = make_reg(false);
        let slot = AgentShellSlot::from_registration(reg);
        assert!(slot.is_foreground_running(), "前台注册后应处于前台运行中");
        assert!(!slot.is_backgrounded);
        assert!(!slot.ended);
    }

    #[tokio::test]
    async fn test_slot_direct_background_not_foreground() {
        let (reg, _rx) = make_reg(true);
        let slot = AgentShellSlot::from_registration(reg);
        assert!(
            !slot.is_foreground_running(),
            "direct_background 的槽位不应处于前台"
        );
        assert!(slot.is_backgrounded, "direct_background 应标记为已后台化");
    }

    #[tokio::test]
    async fn test_slot_mark_backgrounded_sends_signal() {
        let (reg, mut bg_rx) = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        assert!(slot.mark_backgrounded(), "首次后台化应成功");
        assert!(slot.is_backgrounded);
        // background_tx 应被消费，bg_rx 收到信号
        assert!(bg_rx.try_recv().is_ok(), "后台化应发送 background 信号");
        // 重复后台化返回 false
        assert!(!slot.mark_backgrounded(), "已后台化的重复调用应返回 false");
    }

    #[tokio::test]
    async fn test_slot_mark_backgrounded_after_ended_fails() {
        let (reg, _rx) = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        slot.mark_ended(Some(0));
        assert!(
            !slot.mark_backgrounded(),
            "已退出的槽位后台化应返回 false"
        );
    }

    #[tokio::test]
    async fn test_slot_mark_ended_sets_exit_code() {
        let (reg, _rx) = make_reg(false);
        let mut slot = AgentShellSlot::from_registration(reg);
        slot.mark_ended(Some(42));
        assert!(slot.ended);
        assert_eq!(slot.exit_code, Some(42));
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
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            signal.wait(),
        )
        .await;
        assert!(result.is_ok(), "fire 后 wait 不应超时挂起");
    }
}

