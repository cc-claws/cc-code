//! App 级 UI 状态：跨 session 共享的全局 UI 临时状态

use std::{
    cell::{Cell, RefCell},
    time::Instant,
};

use crate::clipboard::ClipboardLease;

use super::{oauth_prompt::OAuthPrompt, setup_wizard::SetupWizardPanel};

/// App 级 UI 状态：跨 session 共享的全局 UI 临时状态。
///
/// 与 `ServiceRegistry` 中的"服务"字段（config、MCP pool、cron 等）不同，
/// 这里的字段纯粹是 UI 层面的临时状态（高亮计时、弹窗、鼠标探测等）。
pub struct GlobalUiState {
    pub viewport: crate::terminal_backend::viewport::ViewportState,
    /// 布局准备可能跨多个循环，宿主 reflow 的清理请求保留到实际输出成功。
    pub terminal_reflow_pending: bool,
    pub(crate) agent_center: Option<super::agent_center::AgentCenter>,
    pub(crate) agent_catalog: Vec<crate::thread::ThreadMeta>,
    pub(crate) agent_count_checked: Option<Instant>,
    pub(crate) agent_count_rx:
        Option<tokio::sync::oneshot::Receiver<Result<Vec<crate::thread::ThreadMeta>, String>>>,
    /// 标题任务跨会话异步返回；手工命名优先于本进程的自动标题。
    pub(crate) manual_thread_titles:
        std::sync::Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    pub(crate) thread_title_write_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    pub setup_wizard: Option<SetupWizardPanel>,
    pub oauth_prompt: Option<OAuthPrompt>,
    pub mode_highlight_until: Option<Instant>,
    pub model_highlight_until: Option<Instant>,
    pub mcp_ready_shown_until: Cell<Option<Instant>>,
    pub mcp_failed_shown: RefCell<Option<(String, Instant)>>,
    pub quit_pending_since: Option<Instant>,
    pub quit_requested: bool,
    /// Linux X11/Wayland 剪贴板所有权持有。剪贴板内容必须由写入它的进程持有，
    /// 否则 TUI 退出后内容就消失。每次复制都更新这个 lease，让最新内容存活到
    /// TUI 退出。其他平台（macOS/Windows）lease 为 None。
    pub clipboard_lease: Option<ClipboardLease>,
    /// 上次写出到终端标题栏的已清洗字符串（用于 OSC 0 写出去重）
    pub last_terminal_title: Option<String>,
}

impl Default for GlobalUiState {
    fn default() -> Self {
        Self::new()
    }
}
impl GlobalUiState {
    pub fn new() -> Self {
        Self {
            viewport: Default::default(),
            terminal_reflow_pending: false,
            agent_center: None,
            agent_catalog: Vec::new(),
            agent_count_checked: None,
            agent_count_rx: None,
            manual_thread_titles: Default::default(),
            thread_title_write_lock: Default::default(),
            setup_wizard: None,
            oauth_prompt: None,
            mode_highlight_until: None,
            model_highlight_until: None,
            mcp_ready_shown_until: Cell::new(None),
            mcp_failed_shown: RefCell::new(None),
            quit_pending_since: None,
            quit_requested: false,
            clipboard_lease: None,
            last_terminal_title: None,
        }
    }
}
