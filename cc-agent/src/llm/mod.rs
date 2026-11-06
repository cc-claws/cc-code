pub mod anthropic;
pub mod openai;
pub mod repetition;
pub mod retry;
pub mod sse;
pub mod types;
pub mod url_util;

#[cfg(test)]
mod url_util_test;

mod adapter;
mod react_adapter;

// Re-export types for external crate usage (e.g. BaseModel trait impls, tests)
pub use self::react_adapter::BaseModelReactLLM;
pub use self::retry::{RetryConfig, RetryableLLM};
pub use self::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};

use crate::error::AgentResult;
use async_trait::async_trait;

/// BaseModel trait - 统一 LLM 接口，对齐 LangChain Python BaseModel
#[async_trait]
pub trait BaseModel: Send + Sync {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse>;
    fn provider_name(&self) -> &str;
    fn model_id(&self) -> &str;

    /// 模型的上下文窗口大小（token 数）
    ///
    /// 用于 token 用量追踪和上下文压缩决策。
    /// 默认返回 200_000（适用于大多数 modern LLM）。
    fn context_window(&self) -> u32 {
        200_000
    }

    /// 流式调用。默认实现回退到非流式 invoke()。
    /// 仅 ChatOpenAI 和 ChatAnthropic override 此方法实现 SSE 流式。
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        _ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        self.invoke(request).await
    }
}

pub use adapter::MockLLM;
pub use anthropic::ChatAnthropic;
pub use openai::ChatOpenAI;
pub use url_util::{format_reqwest_error, is_opencode_endpoint, normalize_base_url};

/// Build a reqwest client with connection pool limits to prevent TLS session
/// accumulation. Default pool is unbounded — each idle connection holds
/// ~50-100 KB of TLS state that is never released.
///
/// User-Agent 显式声明为本客户端（而非留空或 HTTP 库默认值）：OpenCode Go 等网关
/// 要求客户端自报身份，通用 SDK/HTTP 库名称会被判定为滥用流量。
pub(crate) fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("cc-code/", env!("CARGO_PKG_VERSION")))
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 为请求附加 `x-opencode-session` 会话头
///
/// OpenCode Console/Go 用该头识别会话：缺失时直接返回 400
/// （`Request is missing x-opencode-session and cannot be routed efficiently`），
/// 因为路由与 prompt cache 都按会话聚合。`session_id` 由 ACP 会话 ID 透传，跨轮次稳定。
///
/// 不按 host 过滤：base URL 可能经自建中转指向 OpenCode，过滤会漏发；
/// 对不识别该头的 provider 它只是被忽略的额外请求头，与该仓库既有
/// `x-session-id`、`metadata.session_id` 的透传策略一致。
pub(crate) fn apply_opencode_session_header(
    req: reqwest::RequestBuilder,
    session_id: Option<&str>,
) -> reqwest::RequestBuilder {
    match session_id {
        Some(sid) => req.header("x-opencode-session", sid),
        None => req,
    }
}
