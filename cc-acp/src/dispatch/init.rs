//! Build ACP `initialize` response with full session capabilities.

use agent_client_protocol_schema::{
    AgentCapabilities, AuthMethod, AuthMethodAgent, AuthMethodId, InitializeResponse,
    PromptCapabilities, ProtocolVersion, SessionCapabilities, SessionCloseCapabilities,
    SessionForkCapabilities, SessionListCapabilities, SessionResumeCapabilities,
};

/// Construct the full [`InitializeResponse`] with all session lifecycle
/// capabilities declared (load, list, close, resume, fork).
///
/// `promptCapabilities.image` 必须与实现一致：stdio 路径实际处理
/// `ContentBlock::Image`，不声明会让遵守能力位的客户端永不发送图片。
/// `embeddedContext` 暂不声明——`ContentBlock::Resource` 目前降级为文本。
///
/// Auth: 声明 `AuthMethod::Agent`（agent 自行处理认证，即 cc-code 自有
/// 的 API key / OAuth 配置）。客户端可用 `authenticate` / v2 `auth/login`
/// 完成握手；`logout` / v2 `auth/logout` 清理状态。
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
    InitializeResponse::new(ProtocolVersion::V1)
        .agent_capabilities(caps)
        .auth_methods(vec![AuthMethod::Agent(
            AuthMethodAgent::new(AuthMethodId::new("cc-code"), "cc-code")
                .description("cc-code 自有认证（API key / OAuth，经配置文件或环境变量）"),
        )])
}
