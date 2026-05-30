use std::path::PathBuf;
use std::sync::Arc;

use peri_agent::interaction::ChannelState;
use peri_middlewares::mcp::McpClientPool;
use peri_middlewares::mcp::McpInitStatus;
use peri_middlewares::plugin::PluginLoadResult;
use peri_middlewares::prelude::SharedPermissionMode;

use super::cron_state::CronState;
use super::events::AgentEvent;
use crate::config::PeriConfig;
use crate::thread::ThreadStore;

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
    /// ACP Server 的 PeriConfig Arc（与 ACP Server 共享引用，setup/面板修改后需同步写入）
    pub acp_peri_config: Option<Arc<parking_lot::RwLock<PeriConfig>>>,
    /// ACP Server 的 LlmProvider Arc（与 ACP Server 共享引用，setup/面板修改后需同步写入）
    pub acp_provider: Option<Arc<parking_lot::RwLock<crate::app::agent::LlmProvider>>>,
    /// Channel 共享状态（MCP handler ↔ TUI/broker 桥接）
    pub channel_state: Option<Arc<ChannelState>>,
}

impl ServiceRegistry {
    /// 将当前 TUI 侧的 peri_config / provider 同步写入 ACP Server 持有的 Arc 副本。
    ///
    /// 调用时机：任意修改 peri_config 或切换 provider 后（setup 向导保存、login 面板、
    /// model 面板等路径）。
    pub fn sync_peri_config_to_acp(&mut self) {
        if let Some(ref cfg) = self.peri_config {
            if let Some(ref acp_cfg) = self.acp_peri_config {
                *acp_cfg.write() = cfg.clone();
            }
            if let Some(ref acp_provider) = self.acp_provider {
                if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                    *acp_provider.write() = p;
                }
            }
        }
    }
}
