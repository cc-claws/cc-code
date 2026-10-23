//! Event sink abstraction for ACP session event routing.
//!
//! Different frontends (TUI via MpscTransport, IDE via stdio SDK) route agent
//! execution events differently. [`EventSink`] abstracts this so the core
//! prompt execution logic can live in `cc-acp`.

use async_trait::async_trait;
use cc_agent::agent::events::AgentEvent as ExecutorEvent;
use serde_json::json;
use tracing::{debug, error};

use super::subagent_protocol::{
    enrich_lifecycle, lifecycle_payload, session_update_payload, PeriAgentNotification,
    RoutedSessionNotification, SubagentLifecycleNotification,
};
use crate::{event::map_event, transport::AcpTransport};

// Re-export SDK types used by StdioEventSink.
pub use agent_client_protocol::{
    schema::{SessionId as SdkSessionId, SessionNotification, SessionUpdate},
    Client, ConnectionTo,
};

/// Receives [`ExecutorEvent`]s produced during agent execution and routes them
/// to the appropriate transport.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Push a single executor event. Called from the background pump task.
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32);

    /// Signal that the agent execution stream has ended (no more events).
    async fn push_done(&self, session_id: &str);
}

// ── TUI transport-backed EventSink ──────────────────────────────────────────

/// [`EventSink`] backed by an [`AcpTransport`]. Sends two notification types:
/// - `session/update` — standard ACP SessionUpdate (with `_peri` metadata for TUI)
/// - `peri/agent_event` — raw serialized ExecutorEvent (for TUI-only events, categories ②③)
pub struct TransportEventSink {
    transport: std::sync::Arc<dyn AcpTransport>,
    background_registry: Option<std::sync::Arc<cc_middlewares::BackgroundTaskRegistry>>,
}

impl TransportEventSink {
    pub fn new(transport: std::sync::Arc<dyn AcpTransport>) -> Self {
        Self {
            transport,
            background_registry: None,
        }
    }

    pub fn with_background_registry(
        mut self,
        registry: std::sync::Arc<cc_middlewares::BackgroundTaskRegistry>,
    ) -> Self {
        self.background_registry = Some(registry);
        self
    }
}

#[async_trait]
impl EventSink for TransportEventSink {
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        if let Some(mut payload) = lifecycle_payload(session_id, event) {
            enrich_lifecycle(&mut payload, self.background_registry.as_deref());
            let _ = self
                .transport
                .send_notification("peri/subagent_lifecycle", payload)
                .await;
            return;
        }
        let mapped = map_event(event, context_window);

        for m in mapped {
            // 1. session/update — standard ACP notifications
            for update in m.updates {
                let payload =
                    match session_update_payload(session_id, &update, m.source_agent_id.as_deref())
                    {
                        Ok(p) => p,
                        Err(e) => {
                            error!(error = %e, "EventSink: serialize SessionUpdate failed");
                            continue;
                        }
                    };
                let _ = self
                    .transport
                    .send_notification("session/update", payload)
                    .await;
            }

            // 2. peri/agent_event — TUI-specific events (categories ②③)
            if m.forward_to_tui {
                let event_json = match serde_json::to_string(event) {
                    Ok(s) => s,
                    Err(e) => {
                        error!(error = %e, "EventSink: serialize ExecutorEvent failed");
                        continue;
                    }
                };
                let agent_event_params = json!({
                    "sessionId": session_id,
                    "event_json": event_json,
                });
                if let Err(e) = self
                    .transport
                    .send_notification("peri/agent_event", agent_event_params)
                    .await
                {
                    error!(error = %e, "EventSink: send peri/agent_event failed");
                }
            }
        }
    }

    async fn push_done(&self, session_id: &str) {
        debug!(session_id = %session_id, "EventSink: sending agent_event_done");
        if let Err(e) = self
            .transport
            .send_notification("peri/agent_event_done", json!({ "sessionId": session_id }))
            .await
        {
            error!(session_id = %session_id, error = %e, "EventSink: agent_event_done send failed")
        }
    }
}

// ── SDK-backed EventSink for stdio path ─────────────────────────────────────

/// [`EventSink`] backed by the SDK's [`ConnectionTo<Client>`].
///
/// Sends the same routed `session/update` and lifecycle extensions as TUI.
/// Used by the stdio `peri acp` mode
/// which communicates with external IDE clients via the agent-client-protocol SDK.
pub struct StdioEventSink {
    cx: ConnectionTo<Client>,
    session_id: SdkSessionId,
    background_results:
        Option<tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::BackgroundTaskResult>>,
    background_registry: Option<std::sync::Arc<cc_middlewares::BackgroundTaskRegistry>>,
}

impl StdioEventSink {
    pub fn new(cx: ConnectionTo<Client>, session_id: SdkSessionId) -> Self {
        Self {
            cx,
            session_id,
            background_results: None,
            background_registry: None,
        }
    }

    pub fn with_background_results(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::BackgroundTaskResult>,
    ) -> Self {
        self.background_results = Some(sender);
        self
    }

    pub fn with_background_registry(
        mut self,
        registry: std::sync::Arc<cc_middlewares::BackgroundTaskRegistry>,
    ) -> Self {
        self.background_registry = Some(registry);
        self
    }

    /// Send an arbitrary `SessionUpdate` notification through the SDK connection.
    pub fn send_update(&self, update: SessionUpdate) {
        let notif = SessionNotification::new(self.session_id.clone(), update);
        if let Err(e) = self.cx.send_notification(notif) {
            error!(error = %e, "StdioEventSink: failed to send SessionUpdate");
        }
    }
}

/// stdio 仅传输既有开始/结果协议，TUI 瞬时状态不写 stdout。
fn map_stdio_event(event: &ExecutorEvent, context_window: u32) -> Vec<crate::event::MappedEvent> {
    fn is_ui_tool_progress(event: &ExecutorEvent) -> bool {
        match event {
            ExecutorEvent::SubagentEvent { event, .. } => is_ui_tool_progress(event),
            ExecutorEvent::ToolPending { .. } => true,
            _ => false,
        }
    }
    if is_ui_tool_progress(event) {
        Vec::new()
    } else {
        map_event(event, context_window)
    }
}

#[async_trait]
impl EventSink for StdioEventSink {
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        if let Some(mut payload) = lifecycle_payload(session_id, event) {
            enrich_lifecycle(&mut payload, self.background_registry.as_deref());
            if let Err(e) = self
                .cx
                .send_notification(SubagentLifecycleNotification(payload))
            {
                error!(error = %e, "StdioEventSink: failed to send lifecycle");
            }
            return;
        }
        if let (Some(sender), ExecutorEvent::BackgroundTaskCompleted(result)) =
            (&self.background_results, event)
        {
            let _ = sender.send(result.clone());
        }
        let mapped = map_stdio_event(event, context_window);
        for m in mapped {
            for update in m.updates {
                let payload =
                    match session_update_payload(session_id, &update, m.source_agent_id.as_deref())
                    {
                        Ok(payload) => payload,
                        Err(e) => {
                            error!(error = %e, "StdioEventSink: serialize update failed");
                            continue;
                        }
                    };
                if let Err(e) = self
                    .cx
                    .send_notification(RoutedSessionNotification(payload))
                {
                    error!(error = %e, "StdioEventSink: failed to send SessionNotification");
                    break;
                }
            }
            if m.forward_to_tui {
                if let Ok(event_json) = serde_json::to_string(event) {
                    let _ = self.cx.send_notification(PeriAgentNotification(json!({
                        "sessionId": session_id,
                        "event_json": event_json,
                    })));
                }
            }
        }
    }

    async fn push_done(&self, _session_id: &str) {
        // No explicit done signal in standard ACP protocol.
    }
}

#[cfg(test)]
#[path = "event_sink_test.rs"]
mod tests;
