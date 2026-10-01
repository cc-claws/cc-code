//! Hook system integration.
//!
//! Hook middleware is configured through `AcpAgentConfig::hook_groups`.
//! Each group becomes a separate `HookMiddleware` instance in the agent pipeline.
//!
//! Hooks are event-driven callbacks (Command/Prompt/Http/Agent) for 14 event types,
//! provided by `cc_middlewares::hooks`.

pub use cc_middlewares::hooks::types::RegisteredHook;
