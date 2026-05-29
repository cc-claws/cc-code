use async_trait::async_trait;

use super::BaseModel;
use crate::{
    agent::react::{ReactLLM, Reasoning, ToolCall},
    error::AgentResult,
    llm::types::{LlmRequest, StopReason, StreamingContext},
    messages::{BaseMessage, ContentBlock},
    tools::BaseTool,
};

/// BaseModelReactLLM - 将 BaseModel 适配为 ReactLLM
pub struct BaseModelReactLLM {
    pub model: Box<dyn BaseModel>,
    pub system: Option<String>,
    /// 会话级 ID，透传到 LlmRequest，供代理（如 LiteLLM）按 session 聚合请求
    pub session_id: Option<String>,
}

impl BaseModelReactLLM {
    pub fn new(model: Box<dyn BaseModel>) -> Self {
        Self {
            model,
            system: None,
            session_id: None,
        }
    }

    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

#[async_trait]
impl ReactLLM for BaseModelReactLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        let tool_defs = tools.iter().map(|t| t.definition()).collect();

        let mut request = LlmRequest::new(messages.to_vec()).with_tools(tool_defs);

        if let Some(system) = &self.system {
            request = request.with_system(system.clone());
        }

        if let Some(ref sid) = self.session_id {
            request = request.with_session_id(sid.clone());
        }

        let model_name = self.model.model_id().to_string();
        let provider = self.model.provider_name();
        let msg_count = messages.len();
        let tool_count = tools.len();
        let start = std::time::Instant::now();

        let streamed = streaming.is_some();
        let response = if let Some(ctx) = streaming {
            self.model.invoke_streaming(request, ctx).await
        } else {
            self.model.invoke(request).await
        }
        .map_err(|e| {
            tracing::error!(
                provider = provider,
                model = %model_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                msg_count,
                tool_count,
                streamed,
                error = %e,
                "generate_reasoning 失败"
            );
            e
        })?;

        let usage = response.usage.clone();
        tracing::debug!(
            provider = provider,
            model = %model_name,
            elapsed_ms = start.elapsed().as_millis() as u64,
            msg_count,
            streamed,
            stop_reason = ?response.stop_reason,
            input_tokens = usage.as_ref().map(|u| u.input_tokens),
            output_tokens = usage.as_ref().map(|u| u.output_tokens),
            cache_creation = usage.as_ref().and_then(|u| u.cache_creation_input_tokens),
            cache_read = usage.as_ref().and_then(|u| u.cache_read_input_tokens),
            "generate_reasoning 完成"
        );

        let usage = response.usage.clone();

        if response.stop_reason == StopReason::ToolUse {
            // 从 content_blocks() 提取 ToolUse blocks（跨 provider 兼容）
            let blocks = response.message.content_blocks();
            let thought = blocks
                .iter()
                .filter_map(|b| b.as_text())
                .collect::<Vec<_>>()
                .join("");

            let calls: Vec<ToolCall> = blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolUse { id, name, input } = b {
                        Some(ToolCall::new(id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            if !calls.is_empty() {
                let mut r = Reasoning::with_tools(thought, calls);
                r.stop_reason = response.stop_reason.clone();
                r.source_message = Some(response.message);
                r.usage = usage;
                r.model = model_name;
                r.streamed = streamed;
                return Ok(r);
            }

            // fallback：从 tool_calls() 读（兼容旧路径）
            let calls: Vec<ToolCall> = response
                .message
                .tool_calls()
                .iter()
                .map(|tc| ToolCall::new(tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                .collect();
            if calls.is_empty() {
                tracing::warn!("LLM 返回 ToolUse stop_reason 但无 tool_calls，降级为最终回答");
                let text = if thought.is_empty() {
                    "(empty response)".to_string()
                } else {
                    thought
                };
                let mut r = Reasoning::with_answer("", text);
                r.stop_reason = response.stop_reason.clone();
                r.source_message = Some(response.message);
                r.usage = usage;
                r.model = model_name;
                r.streamed = streamed;
                return Ok(r);
            }
            let mut r = Reasoning::with_tools(thought, calls);
            r.stop_reason = response.stop_reason.clone();
            r.source_message = Some(response.message);
            r.usage = usage;
            r.model = model_name;
            r.streamed = streamed;
            Ok(r)
        } else if response.message.has_tool_calls() {
            // 防御：某些 provider（如 DeepSeek）可能返回 stop_reason != ToolUse
            // 但响应内容含 tool_use blocks。此时必须按工具调用处理，
            // 否则 source_message（含 tool_use）会通过 handle_final_answer 写入 state
            // 而无配对 tool_result，导致下次 API 调用 400。
            let tc_reqs = response.message.tool_calls();
            let calls: Vec<ToolCall> = tc_reqs
                .iter()
                .map(|tc| ToolCall::new(tc.id.clone(), tc.name.clone(), tc.arguments.clone()))
                .collect();
            tracing::warn!(
                stop_reason = ?response.stop_reason,
                tool_count = calls.len(),
                "stop_reason 与内容不一致：响应含 tool_use 但 stop_reason 非 ToolUse，按工具调用处理"
            );
            let text = response.message.content();
            let mut r = Reasoning::with_tools(text, calls);
            r.stop_reason = response.stop_reason.clone();
            r.source_message = Some(response.message);
            r.usage = usage;
            r.model = model_name;
            r.streamed = streamed;
            Ok(r)
        } else {
            // 最终答案：text_content() 提取所有文字（跳过 reasoning block）
            let mut text = response.message.content();
            if response.stop_reason == StopReason::MaxTokens {
                tracing::warn!("LLM 输出因 max_tokens 截断，回答可能不完整");
                text.push_str("\n\n[⚠ 回答因输出长度限制被截断]");
            }
            let mut r = Reasoning::with_answer("", text);
            r.stop_reason = response.stop_reason.clone();
            r.source_message = Some(response.message);
            r.usage = usage;
            r.model = model_name;
            r.streamed = streamed;
            Ok(r)
        }
    }

    fn model_name(&self) -> String {
        self.model.model_id().to_string()
    }

    fn context_window(&self) -> u32 {
        // 委托给 BaseModel 实现，每个模型提供自己的准确上下文窗口
        self.model.context_window()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("react_adapter_test.rs");
}
