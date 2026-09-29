//! Build ACP `initialize` response with full session capabilities.

use agent_client_protocol_schema::{
    AgentCapabilities, InitializeResponse, PromptCapabilities, ProtocolVersion,
    SessionCapabilities, SessionCloseCapabilities, SessionForkCapabilities,
    SessionListCapabilities, SessionResumeCapabilities,
};

/// Construct the full [`InitializeResponse`] with all session lifecycle
/// capabilities declared (load, list, close, resume, fork).
///
/// `promptCapabilities.image` 必须与实现一致：stdio 路径实际处理
/// `ContentBlock::Image`，不声明会让遵守能力位的客户端永不发送图片。
/// `embeddedContext` 暂不声明——`ContentBlock::Resource` 目前降级为文本。
///
/// Used by both TUI (MpscTransport) and stdio transport implementations.
pub fn build_initialize_response() -> InitializeResponse {
    let caps = AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new().image(true))
        .session_capabilities(
            SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .close(SessionCloseCapabilities::new())
                .resume(SessionResumeCapabilities::new())
                .fork(SessionForkCapabilities::new()),
        );
    InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(caps)
}
