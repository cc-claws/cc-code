//! cc-tui-core — 会话引擎与协议服务层的纯逻辑部分（Phase 2 第一批）。
//!
//! 当前承载零 UI / App 耦合的通用基建：国际化、会话导出、配置类型重导出。
//! 后续批次按拆分计划 §9.8 规程逐步下沉纯数据面板状态与 ViewModel。

pub mod config;
pub mod display_flags;
pub mod export;
pub mod i18n;
pub mod markdown;
pub mod message_view;
pub mod shell_history;
pub mod shell_notification;
pub mod terminal_theme;
pub mod text_width;
pub mod theme;
pub mod tool_display;
pub mod ui_state;

// terminal_theme 内部以 `crate::terminal_color` 引用色深探测（与 cc-tui 保持同名）。
pub use cc_terminal::terminal_color;
