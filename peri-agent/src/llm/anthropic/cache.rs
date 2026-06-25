use serde_json::{json, Value};

/// system prompt 边界标记：之前的内容可被 Anthropic prompt cache 命中，
/// 之后的内容变化不会破坏前缀缓存。
pub(super) const SYSTEM_PROMPT_DYNAMIC_BOUNDARY: &str = "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__";

/// system prompt 的独立缓存块
pub(super) struct SystemPromptBlock {
    pub(super) text: String,
    pub(super) cache_control: bool,
}

/// 将 system prompt 文本按边界标记拆分为缓存块。
///
/// 边界标记 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之前的内容标记为可缓存，
/// 之后的内容不标记缓存（动态内容变化不会破坏前缀缓存）。
pub(super) fn split_system_blocks(text: &str) -> Vec<SystemPromptBlock> {
    if text.is_empty() {
        return Vec::new();
    }
    if let Some(idx) = text.find(SYSTEM_PROMPT_DYNAMIC_BOUNDARY) {
        let mut blocks = Vec::new();
        let static_text = text[..idx].trim().to_string();
        let dynamic_text = text[idx + SYSTEM_PROMPT_DYNAMIC_BOUNDARY.len()..]
            .trim()
            .to_string();
        if !static_text.is_empty() {
            blocks.push(SystemPromptBlock {
                text: static_text,
                cache_control: true,
            });
        }
        if !dynamic_text.is_empty() {
            blocks.push(SystemPromptBlock {
                text: dynamic_text,
                cache_control: false,
            });
        }
        blocks
    } else {
        // 无边界标记 → 单块，不缓存
        vec![SystemPromptBlock {
            text: text.to_string(),
            cache_control: false,
        }]
    }
}

/// 对 messages 列表中的 user 消息追加 cache_control 断点
///
/// Anthropic Prompt Caching 要求在需要缓存的边界位置加 `cache_control: { type: "ephemeral" }`。
/// 最多允许 4 个断点（system 占 1-2 个，messages 中占剩余名额）。
///
/// **缓存策略**（最多 3 断点）：
/// 1. **第一条 user 消息**：system + 首条 user 构成稳定缓存段，后续轮次不会失效。
/// 2. **倒数第二条 user 消息**：多轮对话中，上一轮的 user+assistant+tool 整段可被缓存。
///    当目标消息仅含 tool_result 无 text block 时，沿 user_indices 向前回退搜索。
/// 3. **最后一条 user 消息**：当前轮次的完整前缀可被缓存（同一轮内多次工具调用间复用）。
///    同样支持回退搜索。
///
/// 当 user 消息不足 3 条时，按实际数量设置断点（不会重复）。
pub(super) fn apply_cache_to_messages(messages: &mut [Value]) {
    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "user")
        .map(|(i, _)| i)
        .collect();

    if user_indices.is_empty() {
        return;
    }

    // 检查 user 消息是否包含可附加 cache_control 的 text block
    let has_text_block = |msg: &Value| -> bool {
        match msg.get("content") {
            Some(Value::Array(blocks)) => blocks.iter().any(|b| {
                b["type"].as_str() == Some("text")
                    && !b["text"]
                        .as_str()
                        .map(|t| t.trim().is_empty())
                        .unwrap_or(true)
            }),
            Some(Value::String(s)) => !s.trim().is_empty(),
            _ => false,
        }
    };

    // 确定要加断点的位置：第一条 + 倒数第二条 + 最后一条（去重）
    let mut target_indices: Vec<usize> = Vec::new();
    target_indices.push(user_indices[0]);
    if let Some(&last) = user_indices.last() {
        if last != user_indices[0] {
            target_indices.push(last);
        }
    }
    if user_indices.len() >= 3 {
        let second_to_last = user_indices[user_indices.len() - 2];
        if !target_indices.contains(&second_to_last) {
            if second_to_last < target_indices[0] {
                target_indices.insert(0, second_to_last);
            } else if second_to_last > target_indices[target_indices.len() - 1] {
                target_indices.push(second_to_last);
            } else {
                target_indices.insert(1, second_to_last);
            }
        }
    }

    for idx in &target_indices {
        // 如果目标消息无 text block，沿 user_indices 向前回退搜索
        let effective_idx = if has_text_block(&messages[*idx]) {
            Some(*idx)
        } else {
            user_indices
                .iter()
                .rev()
                .find(|&&ui| {
                    ui < *idx && has_text_block(&messages[ui]) && !target_indices.contains(&ui)
                })
                .copied()
        };

        if let Some(ei) = effective_idx {
            let msg = &mut messages[ei];
            if let Some(content) = msg.get_mut("content") {
                match content {
                    Value::Array(blocks) => {
                        let target = blocks.iter_mut().rfind(|b| {
                            let btype = b["type"].as_str().unwrap_or("");
                            btype == "text"
                                && !b["text"]
                                    .as_str()
                                    .map(|t| t.trim().is_empty())
                                    .unwrap_or(true)
                        });
                        if let Some(block) = target {
                            block["cache_control"] = json!({ "type": "ephemeral" });
                        }
                    }
                    Value::String(s) if !s.trim().is_empty() => {
                        let text = s.clone();
                        *content = json!([{
                            "type": "text",
                            "text": text,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 为不含 thinking 的 assistant 消息注入占位 thinking block。
///
/// DeepSeek Anthropic 兼容端口在 thinking 模式下要求所有 assistant 消息都包含 thinking block。
/// 中间件（如 SkillPreloadMiddleware）注入的伪 assistant 消息不含 thinking，会导致 400 错误。
/// 注入带占位文本和空 signature 的 thinking block 以通过 API 验证。
pub(super) fn ensure_thinking_blocks(messages: &mut [Value]) {
    for msg in messages.iter_mut() {
        if msg["role"] != "assistant" {
            continue;
        }
        let has_thinking = match msg.get("content") {
            Some(Value::Array(blocks)) => blocks.iter().any(|b| {
                let btype = b["type"].as_str().unwrap_or("");
                btype == "thinking" || btype == "redacted_thinking"
            }),
            _ => false,
        };
        if !has_thinking {
            let placeholder = json!({
                "type": "thinking",
                "thinking": "",
                "signature": ""
            });
            match msg.get_mut("content") {
                Some(Value::Array(blocks)) => {
                    blocks.insert(0, placeholder);
                }
                Some(content) => {
                    let old = content.clone();
                    *content = Value::Array(vec![placeholder, old]);
                }
                None => {
                    msg["content"] = Value::Array(vec![placeholder]);
                }
            }
        }
    }
}
