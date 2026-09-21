//! Tracing 日志模块
//!
//! ## 控制开关
//!
//! 通过环境变量控制：
//!
//! | 环境变量 | 说明 |
//! |---|---|
//! | `RUST_LOG` | 日志级别，默认 `info` |
//! | `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出 |
//! | `RUST_LOG_FILE` | 日志文件路径（最高优先级，未设置时用默认路径） |
//!
//! ## 默认日志路径
//!
//! 未设置 `RUST_LOG_FILE` 时，日志统一写入 `~/.cc-code/logs/{service_name}.log`
//! （目录不存在时自动创建）：
//!
//! | 运行模式 | service_name | 默认文件 |
//! |---|---|---|
//! | TUI | `agent-tui` | `~/.cc-code/logs/agent-tui.log` |
//! | `-p` 非交互 | `peri-print` | `~/.cc-code/logs/peri-print.log` |
//! | `acp` 子命令 | `peri-acp` | `~/.cc-code/logs/peri-acp.log` |
//!
//! ## 使用方式
//!
//! 调用一次 [`init_tracing`]，其余自动处理：
//!
//! ```rust,no_run
//! #[tokio::main]
//! async fn main() {
//!     let _guard = peri_agent::telemetry::init_tracing("my-agent");
//! }
//! ```

mod subscriber;

pub use subscriber::TracingGuard;

/// 初始化 tracing
///
/// 返回的 `TracingGuard` 必须保持存活直到程序退出（通常绑定到 `main` 的局部变量）。
pub fn init_tracing(service_name: &str) -> TracingGuard {
    subscriber::init_tracing(service_name)
}
