use async_trait::async_trait;
use serde_json::{json, Value};

use super::{
    super::BaseModel,
    cache::{self, SystemPromptBlock, SYSTEM_PROMPT_DYNAMIC_BOUNDARY},
};
use crate::{
    error::{AgentError, AgentResult},
    llm::{
        sse::SseParser,
        types::{LlmRequest, LlmResponse, StopReason, StreamingContext},
    },
    messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest},
};

// ─── ContentBlock → Anthropic content part ────────────────────────────────

fn block_to_anthropic(block: &ContentBlock) -> Option<Value> {
    match block {
        ContentBlock::Text { text } => Some(json!({ "type": "text", "text": text })),
        ContentBlock::Image { source } => match source {
            ImageSource::Base64 { media_type, data } => Some(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": media_type,
                    "data": data
                }
            })),
            ImageSource::Url { url } => Some(json!({
                "type": "image",
                "source": { "type": "url", "url": url }
            })),
        },
        ContentBlock::Document { source, title } => {
            let src = serde_json::to_value(source).unwrap_or_default();
            let mut obj = json!({ "type": "document", "source": src });
            if let Some(t) = title {
                obj["title"] = json!(t);
            }
            Some(obj)
        }
        ContentBlock::ToolUse { id, name, input } => Some(json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input
        })),
        ContentBlock::ToolResult {
            id,
            tool_use_id,
            content,
            is_error,
        } => {
            let content_val: Vec<Value> = content.iter().filter_map(block_to_anthropic).collect();
            // 部分 provider（如 GLM Anthropic 兼容端口）要求 tool_result block 含 id 字段。
            // 无显式 id 时，生成一个以保证兼容性。
            let block_id = id
                .clone()
                .unwrap_or_else(|| format!("toolu_{}", uuid::Uuid::now_v7()));
            Some(json!({
                "type": "tool_result",
                "id": block_id,
                "tool_use_id": tool_use_id,
                "content": content_val,
                "is_error": is_error
            }))
        }
        // thinking block 在 assistant 消息中由 Anthropic 生成，发送时透传
        ContentBlock::Reasoning { text, signature } => {
            let mut obj = json!({ "type": "thinking", "thinking": text });
            if let Some(sig) = signature {
                obj["signature"] = json!(sig);
            }
            Some(obj)
        }
        ContentBlock::Unknown(v) => Some(v.clone()),
    }
}

fn content_to_anthropic(content: &MessageContent) -> Value {
    match content {
        MessageContent::Text(s) => json!([{"type": "text", "text": s}]),
        MessageContent::Blocks(blocks) => {
            let parts: Vec<Value> = blocks.iter().filter_map(block_to_anthropic).collect();
            Value::Array(parts)
        }
        MessageContent::Raw(values) => Value::Array(values.clone()),
    }
}

/// 将 BaseMessage 列表转为 Anthropic messages 格式
///
/// - System 消息提取到顶层 system 字段
/// - Tool 消息合并为 user content blocks
///
/// **缓存前缀稳定性**：含边界标记的 System 消息（来自 build_system_prompt）
/// 排在最前面，不含边界标记的 middleware 注入内容排在边界之后，
/// 确保 middleware 内容变化不会破坏 Anthropic prompt cache 前缀。
pub(super) fn messages_to_anthropic(
    messages: &[BaseMessage],
) -> (Vec<Value>, Vec<SystemPromptBlock>) {
    let mut system_parts_with_boundary: Vec<String> = Vec::new();
    let mut system_parts_no_boundary: Vec<String> = Vec::new();
    let mut result: Vec<Value> = Vec::new();

    for msg in messages {
        match msg {
            BaseMessage::System { content, .. } => {
                let text = content.text_content();
                if !text.trim().is_empty() {
                    // 含边界标记的 system prompt 排在前面（可缓存前缀），
                    // middleware 注入的内容排在后面（动态段，不影响缓存）
                    if text.contains(cache::SYSTEM_PROMPT_DYNAMIC_BOUNDARY) {
                        system_parts_with_boundary.push(text);
                    } else {
                        system_parts_no_boundary.push(text);
                    }
                }
            }
            BaseMessage::Human { content, .. } => {
                result.push(json!({
                    "role": "user",
                    "content": content_to_anthropic(content)
                }));
            }
            BaseMessage::Ai {
                content,
                tool_calls,
                ..
            } => {
                if tool_calls.is_empty() {
                    result.push(json!({
                        "role": "assistant",
                        "content": content_to_anthropic(content)
                    }));
                } else {
                    // 若 content 已经是 Blocks（含 ToolUse），直接序列化
                    // 否则构造 text + tool_use blocks
                    let content_val = match content {
                        MessageContent::Blocks(_) | MessageContent::Raw(_) => {
                            content_to_anthropic(content)
                        }
                        MessageContent::Text(t) => {
                            let mut blocks: Vec<Value> = Vec::new();
                            if !t.is_empty() {
                                blocks.push(json!({ "type": "text", "text": t }));
                            }
                            for tc in tool_calls {
                                blocks.push(json!({
                                    "type": "tool_use",
                                    "id": tc.id,
                                    "name": tc.name,
                                    "input": tc.arguments
                                }));
                            }
                            Value::Array(blocks)
                        }
                    };
                    result.push(json!({ "role": "assistant", "content": content_val }));
                }
            }
            BaseMessage::Tool {
                id: msg_id,
                tool_call_id,
                content,
                is_error,
            } => {
                // GLM 等第三方 Anthropic 兼容端口要求 tool_result block 含 id 字段。
                // 使用 Tool 消息自身的 MessageId（UUID v7）作为 block id。
                let block_id = msg_id.as_uuid().to_string();
                let tool_result_block = json!({
                    "type": "tool_result",
                    "id": block_id,
                    "tool_use_id": tool_call_id,
                    "content": content_to_anthropic(content),
                    "is_error": is_error
                });

                // Anthropic 要求 tool_result blocks 必须在 user content 数组开头
                let appended = if let Some(last) = result.last_mut() {
                    if last["role"] == "user" {
                        if let Some(arr) = last["content"].as_array_mut() {
                            arr.insert(0, tool_result_block.clone());
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !appended {
                    result.push(json!({
                        "role": "user",
                        "content": [tool_result_block]
                    }));
                }
            }
        }
    }

    // 拼接：含边界的 system prompt 在前，middleware 注入内容追加到边界之后
    let mut system_text = system_parts_with_boundary.join("\n\n");
    if !system_parts_no_boundary.is_empty() {
        let middleware_text = system_parts_no_boundary.join("\n\n");
        if system_text.contains(cache::SYSTEM_PROMPT_DYNAMIC_BOUNDARY) {
            // 在边界标记位置之后插入 middleware 内容
            // split_system_blocks 会在边界处拆分，middleware 内容归入动态段
            system_text = system_text.replacen(
                cache::SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                &format!(
                    "{}\n\n{}",
                    cache::SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                    middleware_text
                ),
                1,
            );
        } else {
            // 无边界标记：全部作为动态段
            system_text = format!("{system_text}\n\n{middleware_text}");
        }
    }
    let system_blocks = cache::split_system_blocks(&system_text);
    (result, system_blocks)
}

// ─── 响应 content blocks → BaseMessage ───────────────────────────────────

pub(super) fn parse_content_blocks(
    raw_blocks: &[Value],
) -> (Vec<ContentBlock>, Vec<ToolCallRequest>) {
    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut tool_calls: Vec<ToolCallRequest> = Vec::new();

    for b in raw_blocks {
        match b["type"].as_str() {
            Some("text") => {
                if let Some(text) = b["text"].as_str() {
                    blocks.push(ContentBlock::text(text));
                }
            }
            Some("thinking") => {
                let text = b["thinking"].as_str().unwrap_or("").to_string();
                let signature = b["signature"].as_str().map(|s| s.to_string());
                if let Some(sig) = signature {
                    blocks.push(ContentBlock::reasoning_with_signature(text, sig));
                } else {
                    blocks.push(ContentBlock::reasoning(text));
                }
            }
            Some("tool_use") => {
                if let (Some(id), Some(name)) = (b["id"].as_str(), b["name"].as_str()) {
                    let input = b["input"].clone();
                    blocks.push(ContentBlock::tool_use(id, name, input.clone()));
                    tool_calls.push(ToolCallRequest::new(id, name, input));
                }
            }
            // Anthropic extended thinking 可能返回 redacted_thinking block，
            // 必须保留原始数据以便在后续请求中回传，否则 API 会拒绝
            Some("redacted_thinking") => {
                blocks.push(ContentBlock::Unknown(b.clone()));
            }
            _ => {
                blocks.push(ContentBlock::Unknown(b.clone()));
            }
        }
    }

    (blocks, tool_calls)
}

/// Build system blocks JSON for Anthropic API request.
///
/// Cache control rules:
/// - Blocks with cache_control=true keep their flag (e.g., static block from split_system_blocks)
/// - The last block gets cache_control as FALLBACK only when no preceding block already has it
///   (handles the 1-block / no-boundary edge case)
pub(super) fn build_system_blocks_json(blocks: &[SystemPromptBlock]) -> Vec<Value> {
    let has_cached = blocks.iter().any(|b| b.cache_control);
    let last_idx = blocks.len().saturating_sub(1);
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let mut block = json!({"type": "text", "text": &b.text});
            if b.cache_control || (i == last_idx && !has_cached) {
                block["cache_control"] = json!({"type": "ephemeral"});
            }
            block
        })
        .collect()
}

/// 构建 Anthropic API 请求体（invoke 和 invoke_streaming 共用）
pub(super) fn build_request_body(
    adapter: &super::ChatAnthropic,
    request: &LlmRequest,
    streaming: bool,
) -> Value {
    let tools_json: Vec<Value> = request
        .tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })
        })
        .collect();

    let (mut messages, system_from_msgs) = messages_to_anthropic(&request.messages);

    // 确保所有 assistant 消息都包含 thinking block。
    //
    // DeepSeek 等 Anthropic 兼容端口在 thinking 模式下要求所有 assistant 消息
    // 都包含 thinking block，即使客户端未显式启用 extended thinking。
    // 中间件（如 SkillPreloadMiddleware、AtMentionMiddleware）注入的伪 assistant
    // 消息不含 thinking block 会导致 400 错误
    // ("The content[].thinking in the thinking mode must be passed back to the API")。
    // 始终调用 ensure_thinking_blocks，它为缺少 thinking 的 assistant 消息补充
    // 空占位 thinking block（thinking: "", signature: ""）——语义上等于"无思考"，
    // 对真实 Anthropic API 也无害（未启用 extended thinking 时 API 会忽略）。
    cache::ensure_thinking_blocks(&mut messages);

    // 合并 system blocks：消息列表中的 System（中间件注入）+ request.system
    let mut system_blocks = system_from_msgs;
    if let Some(ref base) = request.system {
        if !base.is_empty() {
            system_blocks.push(SystemPromptBlock {
                text: base.clone(),
                cache_control: false,
            });
        }
    }
    let max_tokens = request.max_tokens.unwrap_or(adapter.max_tokens);

    // 开启缓存时：对最后一条消息的最后一个 block 加 cache_control
    if adapter.enable_cache {
        cache::apply_cache_to_messages(&mut messages);
    }

    let mut body = json!({
        "model": adapter.model,
        "max_tokens": max_tokens,
        "messages": messages
    });

    if streaming {
        body["stream"] = json!(true);
    } else {
        body["stream"] = json!(false);
    }

    if adapter.enable_cache {
        // system 多块格式：静态块已有 cache_control → 动态块不重复标记
        if !system_blocks.is_empty() {
            body["system"] = Value::Array(build_system_blocks_json(&system_blocks));
        }
    } else if !system_blocks.is_empty() {
        // 不启用缓存：合并为单个字符串，移除边界标记
        let text = system_blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
            .replace(SYSTEM_PROMPT_DYNAMIC_BOUNDARY, "");
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            body["system"] = json!(trimmed);
        }
    }

    if !tools_json.is_empty() {
        body["tools"] = Value::Array(tools_json);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    // Extended Thinking 配置
    if adapter.extended_thinking {
        body["thinking"] = json!({
            "type": "enabled",
            "budget_tokens": adapter.thinking_budget
        });
        body["output_config"] = json!({ "effort": adapter.thinking_effort });
    }

    body
}

/// 处理 Anthropic HTTP 响应：读取、解析、错误处理、LlmResponse 构建
///
/// 从 `invoke` 提取以保持 ~130 行 → ~45 行。
async fn handle_anthropic_response(
    resp: reqwest::Response,
    model: &str,
    msg_count: usize,
    start: std::time::Instant,
    body: &Value,
) -> AgentResult<LlmResponse> {
    let status = resp.status();
    let header_request_id = resp
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let resp_text = resp.text().await.map_err(|e| {
        tracing::error!(
            provider = "anthropic",
            model = %model,
            status = %status,
            elapsed_ms = start.elapsed().as_millis() as u64,
            error = %e,
            "LLM 读取响应体失败"
        );
        AgentError::LlmError(format!("读取响应体失败: {e}"))
    })?;
    let resp_json: Value = match serde_json::from_str(&resp_text) {
        Ok(v) => v,
        Err(e) => {
            // 自适应容错：部分代理网关（如反向代理或格式转换器）在非流式请求下仍强制返回 SSE 流
            if resp_text.contains("event:") || resp_text.contains("data:") {
                match parse_anthropic_sse_to_json(&resp_text) {
                    Ok(sse_val) => {
                        tracing::info!(
                            provider = "anthropic",
                            model = %model,
                            status = %status,
                            elapsed_ms = start.elapsed().as_millis() as u64,
                            "非流式请求收到 SSE 流式响应，已自动聚合还原为标准 JSON"
                        );
                        sse_val
                    }
                    Err(sse_err) => {
                        tracing::error!(
                            provider = "anthropic",
                            model = %model,
                            status = %status,
                            elapsed_ms = start.elapsed().as_millis() as u64,
                            json_err = %e,
                            sse_err = %sse_err,
                            "LLM 响应解析失败（JSON 与 SSE 解析均失败）"
                        );
                        return Err(AgentError::LlmError(format!(
                            "解析响应失败: {e} (SSE fallback: {sse_err})\n原始响应({status}): {resp_text}"
                        )));
                    }
                }
            } else {
                tracing::error!(
                    provider = "anthropic",
                    model = %model,
                    status = %status,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    error = %e,
                    "LLM 响应解析失败"
                );
                return Err(AgentError::LlmError(format!(
                    "解析响应失败: {e}\n原始响应({status}): {resp_text}"
                )));
            }
        }
    };

    let request_id = header_request_id.or_else(|| resp_json["id"].as_str().map(|s| s.to_string()));

    if !status.is_success() {
        let msg = resp_json["error"]["message"]
            .as_str()
            .unwrap_or("未知错误")
            .to_string();
        let error_type = resp_json["error"]["type"].as_str().unwrap_or("unknown");

        if status.as_u16() == 500 {
            tracing::error!(
                provider = "anthropic",
                model = %model,
                status = %status,
                error_type,
                error_message = %msg,
                elapsed_ms = start.elapsed().as_millis() as u64,
                request_messages = %serde_json::to_string(&body["messages"]).unwrap_or_else(|_| "serialize failed".into()),
                "LLM API 500 错误（服务端 bug），已记录请求体"
            );
        } else {
            tracing::error!(
                provider = "anthropic",
                model = %model,
                status = %status,
                error_type,
                error_message = %msg,
                elapsed_ms = start.elapsed().as_millis() as u64,
                msg_count,
                "LLM API 错误"
            );
        }
        return Err(AgentError::LlmHttpError {
            status: status.as_u16(),
            message: format!("API 错误 {status}: {msg}"),
        });
    }

    tracing::info!(
        provider = "anthropic",
        model = %model,
        status = %status,
        elapsed_ms = start.elapsed().as_millis() as u64,
        msg_count,
        input_tokens = resp_json["usage"]["input_tokens"].as_u64().unwrap_or(0),
        output_tokens = resp_json["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cache_read = resp_json["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_creation = resp_json["usage"]["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        "LLM invoke completed"
    );

    parse_anthropic_json_response(&resp_json, model, status, request_id)
}

/// 解析 Anthropic JSON 响应体（支持原生 Anthropic 与反向代理 OpenAI 兼容格式）
pub(crate) fn parse_anthropic_json_response(
    resp_json: &Value,
    model: &str,
    status: reqwest::StatusCode,
    request_id: Option<String>,
) -> AgentResult<LlmResponse> {
    let (raw_blocks, fallback_stop_reason, fallback_usage): (
        Vec<Value>,
        Option<StopReason>,
        Option<crate::llm::types::TokenUsage>,
    ) = match resp_json["content"].as_array() {
        Some(arr) => (arr.clone(), None, None),
        None => {
            // 自适应容错：部分代理网关（如反向代理或格式转换层）在非流式请求下返回 OpenAI 格式
            if let Some(choices) = resp_json["choices"].as_array() {
                if let Some(choice) = choices.first() {
                    let msg = &choice["message"];
                    let mut synth_blocks = Vec::new();
                    if let Some(reasoning) = msg["reasoning_content"]
                        .as_str()
                        .or_else(|| msg["reasoning"].as_str())
                    {
                        if !reasoning.is_empty() {
                            synth_blocks.push(json!({
                                "type": "thinking",
                                "thinking": reasoning,
                            }));
                        }
                    }
                    if let Some(text) = msg["content"].as_str() {
                        if !text.is_empty() {
                            synth_blocks.push(json!({
                                "type": "text",
                                "text": text,
                            }));
                        }
                    }
                    if let Some(tool_calls) = msg["tool_calls"].as_array() {
                        for tc in tool_calls {
                            let id = tc["id"].as_str().unwrap_or("");
                            let name = tc["function"]["name"].as_str().unwrap_or("");
                            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                            synth_blocks.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                    }
                    tracing::info!(
                        provider = "anthropic",
                        model = %model,
                        status = %status,
                        "非流式请求收到 OpenAI 格式响应，已自动兼容解析"
                    );
                    let stop_reason = choice["finish_reason"]
                        .as_str()
                        .map(StopReason::from_openai);
                    let usage = resp_json.get("usage").and_then(|u| {
                        let prompt_tokens = u["prompt_tokens"].as_u64()? as u32;
                        let completion_tokens = u["completion_tokens"].as_u64()? as u32;
                        Some(crate::llm::types::TokenUsage {
                            input_tokens: prompt_tokens,
                            output_tokens: completion_tokens,
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens: None,
                            request_id: request_id.clone(),
                        })
                    });
                    (synth_blocks, stop_reason, usage)
                } else {
                    return Err(AgentError::LlmError("响应 choices 为空".to_string()));
                }
            } else {
                return Err(AgentError::LlmError("响应缺少 content 字段".to_string()));
            }
        }
    };

    let stop_reason = resp_json["stop_reason"]
        .as_str()
        .map(StopReason::from_display)
        .or(fallback_stop_reason)
        .unwrap_or(StopReason::EndTurn);

    let (blocks, tool_calls) = parse_content_blocks(&raw_blocks);

    let message = if !tool_calls.is_empty() {
        let content = if let [single] = blocks.as_slice() {
            if let Some(text) = single.as_text() {
                MessageContent::text(text)
            } else {
                MessageContent::Blocks(blocks)
            }
        } else {
            MessageContent::Blocks(blocks)
        };
        BaseMessage::ai_with_tool_calls(content, tool_calls)
    } else if let [single] = blocks.as_slice() {
        if let Some(text) = single.as_text() {
            BaseMessage::ai(text)
        } else {
            BaseMessage::ai(MessageContent::Blocks(blocks))
        }
    } else if blocks.is_empty() {
        BaseMessage::ai("")
    } else {
        BaseMessage::ai(MessageContent::Blocks(blocks))
    };

    let usage = {
        let raw_input = resp_json["usage"]["input_tokens"]
            .as_u64()
            .map(|v| v as u32)
            .unwrap_or(0);
        let output = resp_json["usage"]["output_tokens"]
            .as_u64()
            .map(|v| v as u32);
        let cache_creation = resp_json["usage"]["cache_creation_input_tokens"]
            .as_u64()
            .map(|v| v as u32)
            .unwrap_or(0);
        let cache_read = resp_json["usage"]["cache_read_input_tokens"]
            .as_u64()
            .map(|v| v as u32)
            .unwrap_or(0);
        match (resp_json["usage"]["input_tokens"].as_u64(), output) {
            (Some(_), Some(o)) => Some(crate::llm::types::TokenUsage {
                input_tokens: raw_input + cache_creation + cache_read,
                output_tokens: o,
                cache_creation_input_tokens: Some(cache_creation),
                cache_read_input_tokens: Some(cache_read),
                request_id: request_id.clone(),
            }),
            _ => None,
        }
    }
    .or(fallback_usage);
    Ok(LlmResponse {
        message,
        stop_reason,
        usage,
        request_id,
    })
}

#[async_trait]
impl BaseModel for super::ChatAnthropic {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let msg_count = request.messages.len();
        let start = std::time::Instant::now();

        let body = build_request_body(self, &request, false);

        let chat_url = match &self.base_url {
            Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
            None => "https://api.anthropic.com/v1/messages".to_string(),
        };

        let mut req = self
            .client
            .post(chat_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if self.enable_cache {
            req = req.header("anthropic-beta", "prompt-caching-2024-07-31");
        }
        if let Some(ref sid) = request.session_id {
            req = req.header("x-session-id", sid.as_str());
        }

        tracing::debug!(
            provider = "anthropic",
            model = %self.model,
            messages_count = body["messages"].as_array().map(|a| a.len()).unwrap_or(0),
            "LLM 请求发送"
        );

        let resp = req.json(&body).send().await.map_err(|e| {
            tracing::error!(
                provider = "anthropic",
                model = %self.model,
                elapsed_ms = start.elapsed().as_millis() as u64,
                error = %e,
                "LLM 网络请求失败"
            );
            AgentError::LlmError(e.to_string())
        })?;

        handle_anthropic_response(resp, &self.model, msg_count, start, &body).await
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn context_window(&self) -> u32 {
        200_000
    }

    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        super::stream::do_invoke_streaming(self, request, ctx).await
    }
}

/// 解析可能被中转代理强制返回为 SSE 流的响应体，聚合还原为标准的 Anthropic 消息 JSON
pub(crate) fn parse_anthropic_sse_to_json(sse_text: &str) -> Result<Value, String> {
    let mut parser = SseParser::new();
    let mut input_bytes = sse_text.as_bytes().to_vec();
    if !input_bytes.ends_with(b"\n\n") {
        input_bytes.extend_from_slice(b"\n\n");
    }

    let mut message_id: Option<String> = None;
    let mut model_name: Option<String> = None;
    let mut stop_reason_str = "end_turn".to_string();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut cache_creation_input_tokens: u32 = 0;
    let mut cache_read_input_tokens: u32 = 0;

    let mut accumulated_blocks: Vec<Value> = Vec::new();
    let mut current_block_type: Option<String> = None;
    let mut text_content = String::new();
    let mut reasoning_content = String::new();
    let mut thinking_signature: Option<String> = None;
    let mut tool_use_id: Option<String> = None;
    let mut tool_use_name: Option<String> = None;
    let mut tool_input_fragments = String::new();

    let events = parser.push(&input_bytes);
    if events.is_empty() {
        return Err("未解析到有效的 SSE 事件".to_string());
    }

    let mut has_recognized_event = false;

    for (event_type, data) in events {
        let event = event_type.as_deref().unwrap_or("");
        let parsed: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match event {
            "message_start" => {
                has_recognized_event = true;
                if let Some(msg) = parsed.get("message") {
                    message_id = msg["id"].as_str().map(|s| s.to_string());
                    model_name = msg["model"].as_str().map(|s| s.to_string());
                    let usage_obj = if msg["usage"].is_object() {
                        &msg["usage"]
                    } else if parsed["usage"].is_object() {
                        &parsed["usage"]
                    } else {
                        &serde_json::Value::Null
                    };
                    input_tokens = usage_obj["input_tokens"].as_u64().unwrap_or(0) as u32;
                    cache_creation_input_tokens = usage_obj["cache_creation_input_tokens"]
                        .as_u64()
                        .unwrap_or(0) as u32;
                    cache_read_input_tokens =
                        usage_obj["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32;
                }
            }
            "content_block_start" => {
                has_recognized_event = true;
                let cb = &parsed["content_block"];
                let cb_type = cb["type"].as_str().unwrap_or("");
                current_block_type = Some(cb_type.to_string());

                match cb_type {
                    "thinking" => {
                        reasoning_content.clear();
                        thinking_signature = cb["signature"].as_str().map(|s| s.to_string());
                    }
                    "text" => {
                        text_content.clear();
                        if let Some(t) = cb["text"].as_str() {
                            text_content.push_str(t);
                        }
                    }
                    "tool_use" => {
                        tool_use_id = cb["id"].as_str().map(|s| s.to_string());
                        tool_use_name = cb["name"].as_str().map(|s| s.to_string());
                        tool_input_fragments.clear();
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                has_recognized_event = true;
                let delta = &parsed["delta"];
                match delta["type"].as_str().unwrap_or("") {
                    "thinking_delta" => {
                        if let Some(t) = delta["thinking"].as_str() {
                            reasoning_content.push_str(t);
                        }
                    }
                    "text_delta" => {
                        if let Some(t) = delta["text"].as_str() {
                            text_content.push_str(t);
                        }
                    }
                    "input_json_delta" => {
                        if let Some(json_part) = delta["partial_json"].as_str() {
                            tool_input_fragments.push_str(json_part);
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                has_recognized_event = true;
                match current_block_type.as_deref() {
                    Some("thinking") => {
                        let mut block = json!({
                            "type": "thinking",
                            "thinking": &reasoning_content
                        });
                        if let Some(ref sig) = thinking_signature {
                            block["signature"] = json!(sig);
                        }
                        accumulated_blocks.push(block);
                    }
                    Some("text") => {
                        accumulated_blocks.push(json!({
                            "type": "text",
                            "text": &text_content
                        }));
                    }
                    Some("tool_use") => {
                        let input: Value = serde_json::from_str(&tool_input_fragments)
                            .unwrap_or_else(|_| {
                                if tool_input_fragments.is_empty() {
                                    json!({})
                                } else {
                                    Value::Null
                                }
                            });
                        accumulated_blocks.push(json!({
                            "type": "tool_use",
                            "id": tool_use_id,
                            "name": tool_use_name,
                            "input": input
                        }));
                    }
                    _ => {}
                }
                current_block_type = None;
            }
            "message_delta" => {
                has_recognized_event = true;
                if let Some(stop) = parsed["delta"]["stop_reason"].as_str() {
                    stop_reason_str = stop.to_string();
                }
                if let Some(tokens) = parsed["usage"]["output_tokens"].as_u64() {
                    output_tokens = tokens as u32;
                }
                if input_tokens == 0 {
                    if let Some(tokens) = parsed["usage"]["input_tokens"].as_u64() {
                        input_tokens = tokens as u32;
                    }
                }
                let delta_cache_creation = parsed["usage"]["cache_creation_input_tokens"]
                    .as_u64()
                    .unwrap_or(0) as u32;
                let delta_cache_read = parsed["usage"]["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or(0) as u32;
                if delta_cache_creation > 0 {
                    cache_creation_input_tokens = delta_cache_creation;
                }
                if delta_cache_read > 0 {
                    cache_read_input_tokens = delta_cache_read;
                }
            }
            "message_stop" => {
                has_recognized_event = true;
                if input_tokens == 0 {
                    if let Some(tokens) = parsed["usage"]["input_tokens"].as_u64() {
                        input_tokens = tokens as u32;
                    }
                    cache_creation_input_tokens = parsed["usage"]["cache_creation_input_tokens"]
                        .as_u64()
                        .unwrap_or(0) as u32;
                    cache_read_input_tokens = parsed["usage"]["cache_read_input_tokens"]
                        .as_u64()
                        .unwrap_or(0) as u32;
                }
            }
            "error" => {
                has_recognized_event = true;
                if let Some(err_obj) = parsed.get("error") {
                    return Ok(json!({
                        "type": "error",
                        "error": err_obj
                    }));
                }
            }
            _ => {}
        }
    }

    if !has_recognized_event {
        return Err("未包含可识别的 Anthropic SSE 事件".to_string());
    }

    if let Some(ref cb_type) = current_block_type {
        match cb_type.as_str() {
            "thinking" => {
                let mut block = json!({
                    "type": "thinking",
                    "thinking": &reasoning_content
                });
                if let Some(ref sig) = thinking_signature {
                    block["signature"] = json!(sig);
                }
                accumulated_blocks.push(block);
            }
            "text" => {
                accumulated_blocks.push(json!({
                    "type": "text",
                    "text": &text_content
                }));
            }
            "tool_use" => {
                let input: Value =
                    serde_json::from_str(&tool_input_fragments).unwrap_or_else(|_| {
                        if tool_input_fragments.is_empty() {
                            json!({})
                        } else {
                            Value::Null
                        }
                    });
                accumulated_blocks.push(json!({
                    "type": "tool_use",
                    "id": tool_use_id,
                    "name": tool_use_name,
                    "input": input
                }));
            }
            _ => {}
        }
    }

    Ok(json!({
        "id": message_id,
        "type": "message",
        "role": "assistant",
        "model": model_name,
        "content": accumulated_blocks,
        "stop_reason": stop_reason_str,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "cache_creation_input_tokens": cache_creation_input_tokens,
            "cache_read_input_tokens": cache_read_input_tokens
        }
    }))
}
