//! cc-acp — ACP Agent Service Layer
//!
//! Provides session management, agent construction, middleware chain assembly,
//! transport abstraction (mpsc/stdio), event mapping, HITL/AskUser broker, and
//! Langfuse tracing. Serves both TUI (via in-memory transport) and IDE (via stdio
//! transport) frontends.

// acp_server 从 cc-tui 迁入后仍需以 `cc_acp::` 绝对路径引用本 crate 的模块。
extern crate self as cc_acp;

pub mod agent;
pub mod broker;
pub mod dispatch;
pub mod event;
pub mod hooks;
pub mod langfuse;
pub mod lsp;
pub mod prompt;
pub mod provider;
pub mod session;
pub mod transport;

#[cfg(test)]
#[path = "kernel_acceptance_test.rs"]
mod kernel_acceptance_test;
