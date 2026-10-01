//! # Shell 执行抽象
//!
//! 定义 [`ShellExecutor`] trait，将 agent 工具（BashTool）的命令执行委托给
//! 应用层（cc-tui / CLI / 测试），使其能接入 shell 池并支持 Ctrl+B 后台化。
//!
//! 命令启动前选定解释器，每次调用只提交一次。TUI 启动即登记，延迟的只有
//! 运行提示；输出全程写盘，Ctrl+B 返回同一任务句柄，硬执行期限保持不变。
//! 退出后由宿主发送完成通知；无后台服务的宿主在启动前拒绝直接后台请求。
//!
//! # 为什么不直接传 `shell_pool`
//!
//! `shell_pool` 在 cc-tui 的 `ChatSession`（同步、非 `Send`、App 渲染循环
//! 独占），而 BashTool 在 cc-middlewares（上游 crate，无法反向依赖 cc-tui）。
//! 照搬项目现有的 [`crate::interaction::UserInteractionBroker`] 模式：trait 定义
//! 在 cc-agent（底层），工具持 `Arc<dyn ShellExecutor>`，应用层实现并经
//! ACP config 透传。
//!
//! # oneshot 单消费者矛盾的解法
//!
//! Claude Code 的 `shellCommand.result` 是 Promise（多消费者），`call` 和
//! `backgroundTask` 各自 `.then` 都能拿结果。peri 用 `tokio::oneshot`（单消费者）。
//! 解法：**invoke 独占 [`AgentShellHandle::result_rx`] 拿完整 stdout**；
//! 退出检测另起一个轻量信号 [`ExitSignal`] 给 UI poll；手动/自动后台化与取消
//! 通过共享的 [`ShellHandoff`] 仲裁归属，避免通道消息入队与消费之间的竞态。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::{oneshot, Notify};

mod handoff;
pub use handoff::ShellHandoff;

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
/// 与 cc-tui 的 `CommandOutput` 结构一致，但定义在 cc-agent 以保持 trait
/// 自包含（cc-middlewares 无法引用 cc-tui 类型）。应用层实现负责转换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ─── ShellRequest ─────────────────────────────────────────────────────────────

/// 请求在启动前选定的解释器语义；执行失败不能切换解释器重跑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellDialect {
    /// 兼容用户本机命令及既有宿主的默认 shell。
    PlatformDefault,
    /// Bash 工具的确定性语义；Windows 必须使用 Git Bash。
    Bash,
}

/// Shell 执行请求。
#[derive(Debug, Clone)]
pub struct ShellRequest {
    /// ACP 宿主绑定的会话归属，不从模型参数读取；子 Agent 继承同一宿主。
    pub owner_session_id: Option<String>,
    /// 调度器提供的身份；非 Agent 调用可以没有身份，不回退到命令文本匹配。
    pub invocation: Option<crate::tools::ToolInvocationContext>,
    /// 实际执行的命令（可能经过 RTK 改写，不含 shell 包装）。
    pub command: String,
    /// 调用原文，用于展示；不得用改写后的文本关联原始工具行。
    /// 关联使用 invocation 中可信的 tool_call_id，而非命令内容。
    pub original_command: String,
    /// 启动前确定的解释器。
    pub shell: ShellDialect,
    /// 工作目录。
    pub cwd: String,
    /// 超时毫秒数（默认 120000，上限 600000，由调用方 clamp 后传入）。
    pub timeout_ms: u64,
    /// 从实际启动起计时的硬上限；前台、手动后台、直接后台均生效。
    pub execution_timeout_ms: u64,
    /// LLM 显式要求后台执行（`run_in_background=true`）。
    ///
    /// 为 true 时，实现应立即把命令转入后台并让 [`ShellExecutor::execute`]
    /// 返回的 handle 处于"已后台化"状态；invoke 据此立即返回 task_id 占位串，
    /// 真实输出靠后续 `<background-task-completed>` 通知注入。
    pub run_in_background: bool,
}

// ─── ExitSignal ───────────────────────────────────────────────────────────────

/// 与工具结果接收器无关的终态，后台观察者也能取得真实退出原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellOutcome {
    Exited(i32),
    TimedOut,
    Cancelled,
    Failed(String),
}

/// 进程退出轻量信号：UI poll 用它检测退出，独立于 invoke 持有的 result_rx。
///
/// 设计目的：解决 `tokio::oneshot` 单消费者限制——invoke 长期 await
/// `result_rx` 拿完整 stdout，UI 又需知道"进程是否已退出"以刷新界面。两者
/// 不能共用一个 receiver，故由实现层在进程退出时同时：
/// 1. 输出落盘后 `exit_signal.finish(outcome)` → 唤醒 UI（真实终态）
/// 2. `result_tx.send(output)` → 唤醒 invoke（拿完整结果）
#[derive(Debug)]
pub struct ExitSignal {
    exited: AtomicBool,
    notify: Notify,
    outcome: Mutex<Option<ShellOutcome>>,
}

impl ExitSignal {
    pub fn new() -> Self {
        Self {
            exited: AtomicBool::new(false),
            notify: Notify::new(),
            outcome: Mutex::new(None),
        }
    }

    /// 进程退出时调用：标记退出并唤醒所有等待者。
    pub fn fire(&self) {
        self.finish(ShellOutcome::Cancelled);
    }

    /// 终态只写一次，后台观察者不会因工具结果 receiver 被释放而丢失退出码。
    pub fn finish(&self, outcome: ShellOutcome) {
        let mut slot = self.outcome.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return;
        }
        *slot = Some(outcome);
        self.exited.store(true, Ordering::Release);
        drop(slot);
        self.notify.notify_waiters();
    }

    pub fn outcome(&self) -> Option<ShellOutcome> {
        self.outcome
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 是否已退出（非阻塞）。
    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    /// 异步等待退出（UI poll 也可用此挂起，或直接轮询 [`Self::is_exited`]）。
    pub async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // notify_waiters 不保留 permit；先登记 waiter，再检查状态，避免
            // finish 恰好发生在状态检查与首次 poll 之间时永久丢失唤醒。
            notified.as_mut().enable();
            if self.is_exited() {
                return;
            }
            notified.await;
        }
    }
}

impl Default for ExitSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "exit_signal_test.rs"]
mod exit_signal_tests;

// ─── AgentShellHandle ─────────────────────────────────────────────────────────

/// agent shell 执行句柄。
///
/// 由 [`ShellExecutor::execute`] 返回，是 invoke 与 UI 之间的桥梁：
/// - `result_rx`：**invoke 独占**，await 拿完整 stdout（后台化不抢占）
/// - `exit_signal`：UI poll 查退出（独立于 result_rx 的轻量信号）
/// - `handoff`：前台等待、取消与 UI 共享的原子归属
/// - `kill`：UI 详情面板按 `x` 杀进程
/// - `output_path` / `task_id`：磁盘输出路径 + 任务标识（通知 XML 用）
///
/// # 所有者约定
///
/// - **invoke**：持有 `result_rx` / `handoff`，
///   决定返回值（完整 stdout 或 task_id 占位串）
/// - **UI 主循环**：注册进 `agent_shells` 槽后，
///   持有 `exit_signal` / `handoff` / `kill` / `output_path` / `task_id`
///   的副本，poll 检测退出 + 响应 Ctrl+B
/// - **spawn 进程 task**：持有 `result_tx`，输出刷盘后发布终态和结果
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
    pub handoff: Arc<ShellHandoff>,
    /// 杀进程句柄（UI 详情面板 `x` 键）。
    pub kill: ShellAbortHandle,
}

// ─── ShellExecutor ────────────────────────────────────────────────────────────

/// Shell 执行抽象 trait。
///
/// 应用层（cc-tui）实现此 trait，把命令委托给 shell 池执行，使其可被
/// Ctrl+B 后台化。测试 / 非 TUI 场景可实现为直接 spawn（保持原 `cmd.output()`
/// 同步行为，见 cc-middlewares 的 `InlineShellExecutor`）。
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
    /// - `req.run_in_background` 为 true 时，handoff 已处于后台归属。
    /// - 否则 handle 处于前台可后台化状态，invoke await `result_rx` 等进程退出。
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle>;
}

/// 按 ACP 会话绑定执行器；共享给子 Agent 的工具也会保留最初归属。
pub struct SessionShellExecutor {
    inner: Arc<dyn ShellExecutor>,
    session_id: String,
}

impl SessionShellExecutor {
    pub fn new(inner: Arc<dyn ShellExecutor>, session_id: String) -> Self {
        Self { inner, session_id }
    }
}

#[async_trait]
impl ShellExecutor for SessionShellExecutor {
    async fn execute(&self, mut req: ShellRequest) -> anyhow::Result<AgentShellHandle> {
        req.owner_session_id = Some(self.session_id.clone());
        self.inner.execute(req).await
    }
}
