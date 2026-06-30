//! # Shell 执行抽象
//!
//! 定义 [`ShellExecutor`] trait，将 agent 工具（BashTool）的命令执行委托给
//! 应用层（peri-tui / CLI / 测试），使其能接入 shell 池并支持 Ctrl+B 后台化。
//!
//! 设计对齐 Claude Code 的 BashTool：命令始终经 shell 执行，运行满一定时间
//! 后注册为可后台化任务；Ctrl+B 后台化只切换输出目标（屏幕→磁盘），进程不
//! 中断，`BashTool::invoke` 仍 await 到进程退出拿完整 stdout。退出时旁路注入
//! `<background-task-completed>` 通知给下一轮对话。
//!
//! # 为什么不直接传 `shell_pool`
//!
//! `shell_pool` 在 peri-tui 的 `ChatSession`（同步、非 `Send`、App 渲染循环
//! 独占），而 BashTool 在 peri-middlewares（上游 crate，无法反向依赖 peri-tui）。
//! 照搬项目现有的 [`crate::interaction::UserInteractionBroker`] 模式：trait 定义
//! 在 peri-agent（底层），工具持 `Arc<dyn ShellExecutor>`，应用层实现并经
//! ACP config 透传。
//!
//! # oneshot 单消费者矛盾的解法
//!
//! Claude Code 的 `shellCommand.result` 是 Promise（多消费者），`call` 和
//! `backgroundTask` 各自 `.then` 都能拿结果。peri 用 `tokio::oneshot`（单消费者）。
//! 解法：**invoke 独占 [`AgentShellHandle::result_rx`] 拿完整 stdout**；
//! 退出检测另起一个轻量信号 [`ExitSignal`] 给 UI poll；后台化只切 output 目标
//! + UI 状态，**不抢 result_rx**。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{oneshot, Notify};

// ─── ShellAbortHandle ────────────────────────────────────────────────────────

/// 可复制的 shell kill 句柄。
///
/// ShellExecutor 的具体实现可能由 tokio task、PTY 子进程或宿主侧进程管理器
/// 持有真实进程。上层只依赖 `abort()`，不绑定某一种运行时句柄。
#[derive(Clone)]
pub struct ShellAbortHandle {
    abort_fn: Arc<dyn Fn() + Send + Sync>,
}

impl ShellAbortHandle {
    pub fn new<F>(abort_fn: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            abort_fn: Arc::new(abort_fn),
        }
    }

    pub fn from_tokio_abort(handle: tokio::task::AbortHandle) -> Self {
        Self::new(move || handle.abort())
    }

    pub fn noop() -> Self {
        Self::new(|| {})
    }

    pub fn abort(&self) {
        (self.abort_fn)();
    }
}

// ─── ShellCommandOutput ───────────────────────────────────────────────────────

/// 命令执行结果（stdout/stderr/exit_code）。
///
/// 与 peri-tui 的 `CommandOutput` 结构一致，但定义在 peri-agent 以保持 trait
/// 自包含（peri-middlewares 无法引用 peri-tui 类型）。应用层实现负责转换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ─── ShellRequest ─────────────────────────────────────────────────────────────

/// Shell 执行请求。
#[derive(Debug, Clone)]
pub struct ShellRequest {
    /// 要执行的命令（不含 shell 包装，由实现层决定 `cmd /C` / `bash -c`）。
    pub command: String,
    /// 工作目录。
    pub cwd: String,
    /// 超时毫秒数（默认 120000，上限 600000，由调用方 clamp 后传入）。
    pub timeout_ms: u64,
    /// LLM 显式要求后台执行（`run_in_background=true`）。
    ///
    /// 为 true 时，实现应立即把命令转入后台并让 [`ShellExecutor::execute`]
    /// 返回的 handle 处于"已后台化"状态；invoke 据此立即返回 task_id 占位串，
    /// 真实输出靠后续 `<background-task-completed>` 通知注入。
    pub run_in_background: bool,
}

// ─── ExitSignal ───────────────────────────────────────────────────────────────

/// 进程退出轻量信号：UI poll 用它检测退出，独立于 invoke 持有的 result_rx。
///
/// 设计目的：解决 `tokio::oneshot` 单消费者限制——invoke 长期 await
/// `result_rx` 拿完整 stdout，UI 又需知道"进程是否已退出"以刷新界面。两者
/// 不能共用一个 receiver，故由实现层在进程退出时同时：
/// 1. `result_tx.send(output)` → 唤醒 invoke（拿完整结果）
/// 2. `exit_signal.fire()` → 唤醒 UI poll（仅知退出这一事实）
#[derive(Debug)]
pub struct ExitSignal {
    exited: AtomicBool,
    notify: Notify,
}

impl ExitSignal {
    pub fn new() -> Self {
        Self {
            exited: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    /// 进程退出时调用：标记退出并唤醒所有等待者。
    pub fn fire(&self) {
        self.exited.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    /// 是否已退出（非阻塞）。
    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    /// 异步等待退出（UI poll 也可用此挂起，或直接轮询 [`Self::is_exited`]）。
    pub async fn wait(&self) {
        if self.is_exited() {
            return;
        }
        loop {
            // double-check 避免错过 notify_waiters（fire 在 await 之前发生）
            let notified = self.notify.notified();
            if self.is_exited() {
                return;
            }
            notified.await;
            if self.is_exited() {
                return;
            }
        }
    }
}

impl Default for ExitSignal {
    fn default() -> Self {
        Self::new()
    }
}

// ─── AgentShellHandle ─────────────────────────────────────────────────────────

/// agent shell 执行句柄。
///
/// 由 [`ShellExecutor::execute`] 返回，是 invoke 与 UI 之间的桥梁：
/// - `result_rx`：**invoke 独占**，await 拿完整 stdout（后台化不抢占）
/// - `exit_signal`：UI poll 查退出（独立于 result_rx 的轻量信号）
/// - `background_tx`：UI 发 Ctrl+B 后台化指令；spawn 进程 task 收到后把
///   stdout/stderr 监听从屏幕切到磁盘（进程不中断）
/// - `kill`：UI 详情面板按 `x` 杀进程
/// - `output_path` / `task_id`：磁盘输出路径 + 任务标识（通知 XML 用）
///
/// # 所有者约定
///
/// - **invoke**：持有 `result_rx`，决定返回值（完整 stdout 或 task_id 占位串）
/// - **UI 主循环**：通过 [`crate`] 注册进 `agent_foreground_shells` 槽后，
///   持有 `exit_signal` / `background_tx` / `kill` / `output_path` / `task_id`
///   的副本，poll 检测退出 + 响应 Ctrl+B
/// - **spawn 进程 task**：持有 `result_tx` / `background_rx`，进程退出时发结果
///   + fire exit_signal；收到 background 信号时切换输出目标
pub struct AgentShellHandle {
    /// 任务唯一 ID（uuid7），用于通知 XML、面板展示、匹配。
    pub task_id: String,
    /// 完整输出的磁盘文件路径（DiskOutput）。
    ///
    /// 前台运行时由实现层 spawn DiskOutput writer 持续写盘（与屏幕显示并行，
    /// 便于后台化时无缝接管 + 详情面板查看历史）；后台化时输出继续写此处。
    pub output_path: PathBuf,
    /// invoke await 此 receiver 拿完整 [`ShellCommandOutput`]（独占）。
    pub result_rx: oneshot::Receiver<anyhow::Result<ShellCommandOutput>>,
    /// UI poll 用它检测退出（独立于 result_rx）。
    pub exit_signal: Arc<ExitSignal>,
    /// 发送即请求后台化（UI Ctrl+B 时用）。`None` 表示已后台化或进程结束。
    pub background_tx: Option<oneshot::Sender<()>>,
    /// 杀进程句柄（UI 详情面板 `x` 键）。
    pub kill: ShellAbortHandle,
}

// ─── ShellExecutor ────────────────────────────────────────────────────────────

/// Shell 执行抽象 trait。
///
/// 应用层（peri-tui）实现此 trait，把命令委托给 shell 池执行，使其可被
/// Ctrl+B 后台化。测试 / 非 TUI 场景可实现为直接 spawn（保持原 `cmd.output()`
/// 同步行为，见 peri-middlewares 的 `InlineShellExecutor`）。
///
/// # 使用示例
///
/// ```rust,ignore
/// let executor: Arc<dyn ShellExecutor> = Arc::new(AgentShellExecutor::new(tx, bg_event_tx));
/// let bash_tool = BashTool::new(cwd, executor);
/// // invoke 内：
/// let handle = self.executor.execute(req).await?;
/// let output = handle.result_rx.await??; // 等进程退出拿完整 stdout
/// ```
#[async_trait]
pub trait ShellExecutor: Send + Sync {
    /// 执行命令，返回 [`AgentShellHandle`]。
    ///
    /// 实现应 spawn 进程并通过 handle 把执行控制权交还调用方：
    /// - `req.run_in_background` 为 true 时，handle 的 `background_tx` 应已消费
    ///   （即视为直接后台），invoke 据此立即返回 task_id 占位串。
    /// - 否则 handle 处于前台可后台化状态，invoke await `result_rx` 等进程退出。
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle>;
}
