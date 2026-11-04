//! Terminal 中间件垫片。
//!
//! 实现已下沉到 `cc-tools::bash`（见拆分计划 §4.1 / §9）。此处保留重导出，
//! 使 `cc_middlewares::middleware::terminal::*` 与 crate 内 `crate::middleware::terminal::*`
//! 路径在下游（含 cc-tui 的 agent shell 契约测试）保持零改动。

pub use cc_tools::bash::{
    format_command_output, get_default_bash_timeout_ms, get_max_bash_timeout_ms, BashTool,
    InlineShellExecutor, TerminalMiddleware,
};
