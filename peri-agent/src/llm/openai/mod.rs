mod invoke;
mod stream;

#[cfg(test)]
use serde_json::Value;

/// ChatOpenAI - OpenAI 兼容 API 的 LLM 实现
pub struct ChatOpenAI {
    pub api_key: String,
    /// OpenAI Base URL，需要 `/v1` 后缀。
    pub base_url: String,
    pub model: String,
    /// o1/o3 系列推理强度："low" | "medium" | "high"
    /// 设置后请求体加 `reasoning_effort` 字段，同时移除 temperature
    pub reasoning_effort: Option<String>,
    /// 是否在请求体中发送 `thinking: { type: "enabled" }`（deepseek-v4-pro 等）
    pub thinking_enabled: bool,
    /// 是否在 content 中回传 `thinking` 类型的 Reasoning 块。
    /// 仅 deepseek-v4-pro 等明确支持的模型开启，其他 provider 不支持会报 400。
    pub supports_thinking_content: bool,
    /// 最大输出 token 数，默认 32000
    pub max_tokens: u32,
    client: reqwest::Client,
}

impl ChatOpenAI {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".to_string(),
            reasoning_effort: None,
            thinking_enabled: false,
            supports_thinking_content: Self::detect_thinking_content_support(&model),
            max_tokens: 32000,
            model,
            client: super::build_reqwest_client(),
        }
    }

    /// 设置 API Base URL。OpenAI Base URL 需要 `/v1` 后缀。
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// 开启 reasoning effort（o1/o3 系列）
    /// `effort`: "low" | "medium" | "high"
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// 开启 DeepSeek thinking 模式（deepseek-v4-pro 等）
    ///
    /// 请求体中添加 `"thinking": {"type": "enabled"}`，API 会返回 `reasoning_content` 字段。
    /// 注意：`supports_thinking_content` 由构造函数根据模型名自动检测，此方法不修改它。
    /// 只有 deepseek-v4 系列支持 content 数组中的 `thinking` 块，其他模型只支持
    /// 顶层 `reasoning_content` 字段回传。
    pub fn with_thinking_enabled(mut self) -> Self {
        self.thinking_enabled = true;
        self
    }

    /// 手动控制是否在 content 中回传 `thinking` 类型的 Reasoning 块
    pub fn with_thinking_content(mut self, enabled: bool) -> Self {
        self.supports_thinking_content = enabled;
        self
    }

    /// 设置最大输出 token 数
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// 根据模型名检测是否支持 content 中的 `thinking` 类型
    ///
    /// DeepSeek V4 的 OpenAI API 格式不支持 content 数组中的 `{"type": "thinking"}` 块，
    /// reasoning 内容应通过顶层 `reasoning_content` 字段回传（在 messages_to_json 中处理）。
    /// Anthropic 兼容端点支持 thinking 块，但那走 AnthropicAdapter 路径。
    /// 目前没有已知的 OpenAI 兼容 API 支持 content 数组中的 thinking 块作为输入。
    fn detect_thinking_content_support(model: &str) -> bool {
        let _ = model;
        false
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        let base_url = std::env::var("OPENAI_API_BASE")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("OPENAI_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "gpt-4o".to_string());
        Some(Self::new(api_key, model).with_base_url(base_url))
    }

    /// 模型的上下文窗口大小（token 数），作为固有方法提供给 BaseModel 和 ReactLLM trait
    fn context_window_inner(&self) -> u32 {
        200_000
    }

    // ─── Thin wrappers（兼容测试文件中的关联函数/方法调用语法）───

    #[cfg(test)]
    pub(crate) fn content_to_openai(
        content: &crate::messages::MessageContent,
        supports_thinking_content: bool,
    ) -> Value {
        invoke::content_to_openai(content, supports_thinking_content)
    }

    #[cfg(test)]
    pub(crate) fn messages_to_json(&self, messages: &[crate::messages::BaseMessage]) -> Vec<Value> {
        invoke::messages_to_json(self, messages)
    }

    #[cfg(test)]
    pub(crate) fn parse_assistant_message(
        assistant_msg: &Value,
        stop_reason: &crate::llm::types::StopReason,
    ) -> crate::messages::BaseMessage {
        invoke::parse_assistant_message(assistant_msg, stop_reason)
    }
}

#[cfg(test)]
#[path = "../openai_test.rs"]
mod tests;
