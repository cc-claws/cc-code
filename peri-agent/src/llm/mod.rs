pub mod anthropic;
pub mod openai;
pub mod retry;
pub mod sse;
pub mod types;

mod adapter;
mod react_adapter;

use crate::{
    error::AgentResult,
    llm::types::{LlmRequest, LlmResponse, StreamingContext},
};
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
pub use react_adapter::BaseModelReactLLM; // BaseModel → ReactLLM 适配器（当前推荐的适配路径）
pub use retry::{RetryConfig, RetryableLLM};

/// Build a reqwest client with connection pool limits to prevent TLS session
/// accumulation. Default pool is unbounded — each idle connection holds
/// ~50-100 KB of TLS state that is never released.
pub(crate) fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
