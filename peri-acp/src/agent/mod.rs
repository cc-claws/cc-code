//! Agent construction and lifecycle.
//!
//! Builds ReActAgent instances with the full middleware chain.
//! Shared by TUI and ACP paths via [`build_agent`].
//!
//! Migrated from peri-tui/src/app/agent.rs:build_bare_agent().

pub mod builder;
pub use builder::*;
