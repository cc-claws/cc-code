use std::{path::PathBuf, sync::Arc};

use cc_agent::interaction::ChannelState;
use cc_middlewares::{
    mcp::{McpClientPool, McpInitStatus},
    plugin::PluginLoadResult,
    prelude::SharedPermissionMode,
};

use super::{cron_state::CronState, events::AgentEvent};
use crate::{config::PeriConfig, shell_history::ShellCommandStore, thread::ThreadStore};

/// 进程资源采样器：每 2 秒采样一次当前进程的 CPU 和内存
pub struct ProcessResourceMonitor {
    sys: sysinfo::System,
    pid: sysinfo::Pid,
    /// 上次采样时间
    last_sample: std::time::Instant,
    /// 缓存的内存使用量（MB）
    memory_mb: u64,
    /// 缓存的 CPU 占用百分比（0.0-100.0，单核；可超过 100 表示多核）
    cpu_percent: f32,
}

impl ProcessResourceMonitor {
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new();
        let pid = sysinfo::get_current_pid().expect("failed to get current PID");
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        Self {
            sys,
            pid,
            last_sample: std::time::Instant::now() - std::time::Duration::from_secs(3), // 确保首次调用立即采样
            memory_mb: 0,
            cpu_percent: 0.0,
        }
    }

    /// 刷新缓存（仅当距上次采样 ≥ 2 秒时才执行系统调用）
    pub fn refresh_if_needed(&mut self) {
        if self.last_sample.elapsed() >= std::time::Duration::from_secs(2) {
            self.sys
                .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.pid]), true);
            if let Some(proc) = self.sys.process(self.pid) {
                self.memory_mb = proc.memory() / 1024 / 1024;
                self.cpu_percent = proc.cpu_usage();
            }
            self.last_sample = std::time::Instant::now();
        }
    }

    pub fn memory_mb(&self) -> u64 {
        self.memory_mb
    }

    pub fn cpu_percent(&self) -> f32 {
        self.cpu_percent
    }
}

/// 全局服务/状态聚合：跨 session 共享的服务字段。
pub struct ServiceRegistry {
    pub peri_config: Option<PeriConfig>,
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub shell_command_store: Arc<ShellCommandStore>,
    pub mcp_pool: Option<Arc<McpClientPool>>,
    pub mcp_init_rx: Option<tokio::sync::watch::Receiver<McpInitStatus>>,
    pub cron: CronState,
    pub plugin_data: Option<PluginLoadResult>,
    pub bg_event_tx: tokio::sync::mpsc::Sender<AgentEvent>,
    pub bg_event_rx: Option<tokio::sync::mpsc::Receiver<AgentEvent>>,
    pub config_path_override: Option<PathBuf>,
    pub claude_settings_override: Option<PathBuf>,
    /// 进程内存监控（2s 刷新）
    pub resource_monitor: parking_lot::Mutex<ProcessResourceMonitor>,
    /// i18n 语言注册表（跨 session 共享）
    pub lc: crate::i18n::LcRegistry,
    /// Channel 共享状态（MCP handler ↔ TUI/broker 桥接）
    pub channel_state: Option<Arc<ChannelState>>,
    /// Git 分支缓存（渲染只读；异步刷新见 `git_branch_poller`）
    pub git_branch_cache: parking_lot::Mutex<GitBranchCache>,
    /// Git 分支异步刷新器（子进程不占用渲染线程，issue #277）
    pub git_branch_poller: parking_lot::Mutex<GitBranchPoller>,
    /// panic hook 通知 receiver（TUI 模式专用，由 main.rs init_panic_notify 初始化）
    pub panic_notify_rx: Option<tokio::sync::mpsc::UnboundedReceiver<String>>,
}

/// Git 分支状态缓存，避免每帧都 spawn 子进程
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitBranchStatus {
    pub branch: String,
    pub dirty: bool,
}

/// Git 分支状态缓存容器。
///
/// **渲染路径只读**（`get_cached`），写入由后台任务完成（`set_status`）——
/// 因此渲染线程**永远不会**因 `git` 子进程而阻塞。
///
/// 对齐 Codex 的做法（`codex-rs/tui/src/branch_summary.rs` 模块文档）：
/// 分支查询只走异步 executor，「status line can render whichever pieces are
/// available **without blocking the rest of the UI**」。
#[derive(Default)]
pub struct GitBranchCache {
    status: Option<GitBranchStatus>,
}

impl GitBranchCache {
    pub fn new() -> Self {
        Self { status: None }
    }

    /// 读取缓存。渲染路径专用，绝不 spawn 子进程。
    pub fn get_cached(&self) -> Option<&GitBranchStatus> {
        self.status.as_ref()
    }

    /// 由后台刷新任务写回。
    pub fn set_status(&mut self, status: Option<GitBranchStatus>) {
        self.status = status;
    }

    /// 同步探测分支状态（**阻塞**：会 spawn `git` 子进程）。
    ///
    /// 只能在后台线程/任务中调用（见 `GitBranchPoller`），
    /// **禁止**从渲染路径调用。
    pub(crate) fn detect_status(cwd: &str) -> Option<GitBranchStatus> {
        use std::process::Command;
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return None;
        }
        let dirty = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
            .unwrap_or(false);
        Some(GitBranchStatus { branch, dirty })
    }
}

/// Git 分支异步刷新器。
///
/// 把 `detect_status` 的子进程开销移出渲染线程：事件循环按 TTL 触发异步探测，
/// 结果经 channel 回传并写回 [`GitBranchCache`]。
///
/// 为什么需要它（原实现的缺陷）：旧逻辑为「避免子进程阻塞渲染」而在
/// `ui.loading` 期间**完全冻结**缓存，导致 Agent 工作期间状态栏分支名长期陈旧
/// （issue #277）。异步化后阻塞问题从根上消失，loading 期间亦可正常刷新。
pub struct GitBranchPoller {
    tx: tokio::sync::mpsc::UnboundedSender<Option<GitBranchStatus>>,
    rx: Option<tokio::sync::mpsc::UnboundedReceiver<Option<GitBranchStatus>>>,
    /// 是否有探测在途（避免重复 spawn）
    pending: bool,
    last_request: Option<std::time::Instant>,
}

impl GitBranchPoller {
    /// 刷新间隔。沿用旧值 5s；区别在于**loading 期间同样生效**。
    const TTL: std::time::Duration = std::time::Duration::from_secs(5);

    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            tx,
            rx: Some(rx),
            pending: false,
            last_request: None,
        }
    }

    /// 是否该发起一次探测（未在途中、且距上次超过 TTL）。
    pub fn due(&self) -> bool {
        self.rx.is_some()
            && !self.pending
            && self
                .last_request
                .map(|t| t.elapsed() >= Self::TTL)
                .unwrap_or(true)
    }

    /// 标记已发起探测，返回用于 spawn 的 sender 副本。
    pub fn begin(&mut self) -> tokio::sync::mpsc::UnboundedSender<Option<GitBranchStatus>> {
        self.pending = true;
        self.last_request = Some(std::time::Instant::now());
        self.tx.clone()
    }

    /// 强制下次 [`due`] 返回 true（用于轮末等需要立即刷新的时机）。
    pub fn invalidate(&mut self) {
        self.last_request = None;
    }

    /// 收取所有已完成探测的结果，返回最后一次结果（None 表示探测失败）。
    pub fn drain(&mut self) -> Option<Option<GitBranchStatus>> {
        let rx = self.rx.as_mut()?;
        let mut last: Option<Option<GitBranchStatus>> = None;
        loop {
            match rx.try_recv() {
                Ok(status) => {
                    self.pending = false;
                    last = Some(status);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.rx = None;
                    break;
                }
            }
        }
        last
    }
}

impl Default for GitBranchPoller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "service_registry_test.rs"]
mod tests;
