//! Build ACP `initialize` response with full session capabilities.

use agent_client_protocol_schema::{
    AgentCapabilities, InitializeResponse, PromptCapabilities, ProtocolVersion,
    SessionCapabilities, SessionCloseCapabilities, SessionForkCapabilities,
    SessionListCapabilities, SessionResumeCapabilities,
};

/// Construct the full [`InitializeResponse`] with all session lifecycle
/// capabilities declared (load, list, close, resume, fork).
///
/// Used by both TUI (MpscTransport) and stdio transport implementations.
pub fn build_initialize_response() -> InitializeResponse {
    let caps = AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new())
        .session_capabilities(
            SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .close(SessionCloseCapabilities::new())
                .resume(SessionResumeCapabilities::new())
                .fork(SessionForkCapabilities::new()),
        );
    InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(caps)
}
