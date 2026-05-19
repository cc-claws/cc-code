//! Event sink abstraction for ACP session event routing.
//!
//! Different frontends (TUI via MpscTransport, IDE via stdio SDK) route agent
//! execution events differently. [`EventSink`] abstracts this so the core
//! prompt execution logic can live in `peri-acp`.

use async_trait::async_trait;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;

/// Receives [`ExecutorEvent`]s produced during agent execution and routes them
/// to the appropriate transport.
///
/// Two implementations exist:
/// - **TUI**: Serializes events to JSON and sends `peri/agent_event`,
///   `session/update`, and `peri/*` notifications via [`AcpTransport`].
/// - **Stdio SDK**: Converts events to `SessionUpdate` and sends via the
///   SDK's `ConnectionTo<Client>` notification channel.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Push a single executor event. Called from the background pump task.
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32);

    /// Signal that the agent execution stream has ended (no more events).
    async fn push_done(&self, session_id: &str);
}
