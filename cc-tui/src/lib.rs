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
pub mod acp_server;
pub mod alloc_config;
pub mod app;
pub mod clipboard;
pub mod command;
pub mod config;
pub mod conpty;
pub mod event;
pub mod export;
pub mod i18n;
pub mod keyboard_modes;
pub mod shell_exec;
pub mod shell_history;
pub mod terminal_backend;
pub mod terminal_theme;
pub mod terminal_title;
pub mod thread;
pub mod ui;
pub mod update;

#[cfg(test)]
mod alloc_config_test;

/// process-heavy 测试的共享串行锁与等待预算。
///
/// 这些用例会真起 `cmd` / git-bash / powershell 子进程，或以固定墙钟预算等待
/// 后台 IO 就绪。libtest 默认多线程并发执行，多个用例同时抢进程与 IO，在慢机器
/// （CI 的 4 核 windows runner）上会把启动与调度延迟放大到超出等待窗口，造成偶发
/// 超时（见 issue #451）。这把全局异步锁只做序列化，不改变任何用例语义。
#[cfg(test)]
pub(crate) mod test_lock {
    use std::time::Duration;

    /// process-heavy 用例串行锁。
    pub(crate) static PROCESS_TEST_LOCK: tokio::sync::Mutex<()> =
        tokio::sync::Mutex::const_new(());

    /// process-heavy 用例的等待预算：CI windows runner 在并发下，子进程启动与
    /// 输出到达可能比本机慢一个数量级，原先 5s/10s 的窗口不足以覆盖最坏情况。
    pub(crate) const PROCESS_TEST_TIMEOUT: Duration = Duration::from_secs(30);
}
