//! cc-terminal — 跨平台终端底层（终端后端、剪贴板、ConPTY、色彩、标题、键盘模式、shell 执行）。
//!
//! 模块名与 `cc-tui` 保持完全一致，使两侧 `crate::*` 路径在搬迁后无需改动
//! （见拆分计划 §9.4）。

// 这些模块整体从 cc-tui 迁入，沿用其 lint 豁免口径，避免搬迁引入无关重构。
#![allow(
    clippy::if_same_then_else,
    clippy::needless_range_loop,
    clippy::reversed_empty_ranges,
    clippy::let_underscore_future,
    clippy::question_mark,
    clippy::collapsible_else_if,
    clippy::ptr_arg,
    clippy::infallible_try_from,
    clippy::new_without_default
)]

pub mod clipboard;
pub mod conpty;
pub mod keyboard_modes;
pub mod shell_exec;
pub mod terminal_backend;
pub mod terminal_color;
pub mod terminal_title;

/// process-heavy 测试的共享串行锁与等待预算（与 cc-tui 保持一致）。
#[cfg(test)]
pub(crate) mod test_lock {
    use std::time::Duration;

    pub(crate) static PROCESS_TEST_LOCK: tokio::sync::Mutex<()> =
        tokio::sync::Mutex::const_new(());

    pub(crate) const PROCESS_TEST_TIMEOUT: Duration = Duration::from_secs(30);
}
