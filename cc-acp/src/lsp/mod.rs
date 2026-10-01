//! LSP middleware integration.
//!
//! Re-exports `cc_lsp` types and provides integration with
//! `cc_middlewares::LspMiddleware` for the agent builder.
//!
//! LSP servers are configured in `AcpAgentConfig::lsp_servers`
//! and automatically registered when non-empty.

pub use cc_lsp::config::LspServerConfig;
