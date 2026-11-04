//! TUI interface for Rust Agent - interactive terminal playground

#![allow(
    clippy::if_same_then_else,
    clippy::needless_range_loop,
    clippy::reversed_empty_ranges,
    clippy::let_underscore_future,
    clippy::question_mark,
    clippy::collapsible_else_if,
    clippy::ptr_arg,
    clippy::infallible_try_from
)]

pub mod acp_client;
pub mod alloc_config;
pub mod app;
pub mod command;
pub mod event;
pub use cc_tui_core::shell_history;
pub use cc_tui_core::terminal_theme;
pub mod thread;
pub mod ui;
pub mod update;

// 内嵌 ACP 服务运行时已收敛至 cc-acp，此处重导出保持 main.rs 的 cc_tui::acp_server 路径。
pub use cc_acp::session::acp_server;

// 零耦合通用基建已下沉到 cc-tui-core，此处重导出保持 crate 内路径零改动。
pub use cc_tui_core::{config, export, i18n};

// 终端底层已下沉到 cc-terminal，此处以同名重导出保持 crate 内路径零改动。
pub use cc_terminal::clipboard;
pub use cc_terminal::conpty;
pub use cc_terminal::keyboard_modes;
pub use cc_terminal::shell_exec;
pub use cc_terminal::terminal_backend;
pub use cc_terminal::terminal_color;
pub mod terminal_title;

#[cfg(test)]
mod alloc_config_test;

/// process-heavy 测试的等待预算。
///
/// 这些用例会真起 `cmd` / git-bash / powershell 子进程，或以固定墙钟预算等待
/// 后台 IO 就绪。CI 的 4 核 windows runner 上启动与调度延迟可能比本机慢一个
/// 数量级，原先 5s/10s 的窗口不足以覆盖最坏情况（见 issue #451）。
/// 串行锁随 shell_exec 相关用例一同迁至 cc-terminal::test_lock。
#[cfg(test)]
pub(crate) mod test_lock {
    use std::time::Duration;

    pub(crate) const PROCESS_TEST_TIMEOUT: Duration = Duration::from_secs(30);
}
