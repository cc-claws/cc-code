use serde::{Deserialize, Serialize};

/// 消息唯一标识符 — UUID v7（时间有序，跨进程安全）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(uuid::Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    pub fn as_uuid(&self) -> uuid::Uuid {
        self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

use super::content::{ContentBlock, MessageContent};

// ─── ToolCallRequest ──────────────────────────────────────────────────────────

/// 工具调用请求（对应 OpenAI tool_calls / Anthropic tool_use blocks）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ToolCallRequest {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

// ─── BaseMessage ──────────────────────────────────────────────────────────────

/// BaseMessage - 统一消息类型，对齐 LangChain BaseMessage
///
/// `content` 字段为 `MessageContent`，支持：
/// - 纯文本字符串
/// - 标准 ContentBlock 列表（多模态、推理内容等）
/// - Provider 原生格式（透传）
///
/// `content_blocks()` 方法懒解析，对齐 LangChain JS 的 `contentBlocks` 属性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum BaseMessage {
    #[serde(rename = "user")]
    Human {
        id: MessageId,
        content: MessageContent,
    },

    #[serde(rename = "assistant")]
    Ai {
        id: MessageId,
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCallRequest>,
    },

    #[serde(rename = "system")]
    System {
        id: MessageId,
        content: MessageContent,
    },

    #[serde(rename = "tool")]
    Tool {
        id: MessageId,
        tool_call_id: String,
        content: MessageContent,
        #[serde(default)]
        is_error: bool,
        /// 实际产生该结果的工具名。自动改道（schema 错配救援）后与 AI 消息的声明名
        /// 不同，展示层以本字段为准，避免「声明名 + 改道后输出」杂交体；与声明一致
        /// 或旧记录为 `None` 时，展示层回退 AI 消息 `tool_calls` 按 `tool_call_id` 回查。
        /// 仅内部展示用：LLM 请求序列化手工构造 provider 形状，不携带本字段。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
    },
}

impl BaseMessage {
    // ── 构造器 ────────────────────────────────────────────────────────────────

    pub fn human(content: impl Into<MessageContent>) -> Self {
        Self::Human {
            id: MessageId::new(),
            content: content.into(),
        }
    }

    pub fn ai(content: impl Into<MessageContent>) -> Self {
        Self::Ai {
            id: MessageId::new(),
            content: content.into(),
            tool_calls: Vec::new(),
        }
    }

    pub fn ai_with_tool_calls(
        content: impl Into<MessageContent>,
        tool_calls: Vec<ToolCallRequest>,
    ) -> Self {
        Self::Ai {
            id: MessageId::new(),
            content: content.into(),
            tool_calls,
        }
    }

    /// 构造带 ContentBlock 列表的 AI 消息（含工具调用 block）
    ///
    /// `blocks` 中的 `ToolUse` block 会被同步提取到 `tool_calls`，保持一致性。
    pub fn ai_from_blocks(blocks: Vec<ContentBlock>) -> Self {
        let tool_calls: Vec<ToolCallRequest> = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, name, input } = b {
                    Some(ToolCallRequest::new(
                        id.clone(),
                        name.clone(),
                        input.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect();
        Self::Ai {
            id: MessageId::new(),
            content: MessageContent::Blocks(blocks),
            tool_calls,
        }
    }

    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self::System {
            id: MessageId::new(),
            content: content.into(),
        }
    }

    pub fn tool_result(id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
        Self::Tool {
            id: MessageId::new(),
            tool_call_id: id.into(),
            content: content.into(),
            is_error: false,
            tool_name: None,
        }
    }

    /// 对 AI 消息的**双份名字存储**应用同一个改写闭包。
    ///
    /// `BaseMessage::Ai` 同时把工具名存在 `content` 的 `ContentBlock::ToolUse`
    /// 与 `tool_calls: Vec<ToolCallRequest>` 里，`ai_from_blocks` 只在构造时同步一次，
    /// 之后不会自动同步。任何一处的改写都必须用本方法同时覆盖两处，否则
    /// API payload 与 state / compact 比对会看到不一致的名字。
    ///
    /// 闭包签名 `(旧名字, 入参) -> 新名字`；入参可就地改写（内核入站归一化用），
    /// 也可原样不动（出站历史回译只改名字）。
    pub fn map_ai_tool_names(&mut self, mut f: impl FnMut(&str, &mut serde_json::Value) -> String) {
        let Self::Ai {
            content,
            tool_calls,
            ..
        } = self
        else {
            return;
        };
        match content {
            MessageContent::Blocks(blocks) => {
                for block in blocks.iter_mut() {
                    if let ContentBlock::ToolUse { name, input, .. } = block {
                        let new_name = f(name, input);
                        *name = new_name;
                    }
                }
            }
            MessageContent::Raw(values) => {
                for value in values.iter_mut() {
                    if value["type"].as_str() != Some("tool_use") {
                        continue;
                    }
                    let Some(old_name) = value["name"].as_str().map(str::to_string) else {
                        continue;
                    };
                    let mut input = value
                        .get("input")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let new_name = f(&old_name, &mut input);
                    value["name"] = serde_json::Value::String(new_name);
                    value["input"] = input;
                }
            }
            MessageContent::Text(_) => {}
        }
        for tc in tool_calls.iter_mut() {
            let new_name = f(&tc.name, &mut tc.arguments);
            tc.name = new_name;
        }
    }

    pub fn tool_error(id: impl Into<String>, error: impl Into<MessageContent>) -> Self {
        Self::Tool {
            id: MessageId::new(),
            tool_call_id: id.into(),
            content: error.into(),
            is_error: true,
            tool_name: None,
        }
    }

    // ── 访问器 ────────────────────────────────────────────────────────────────

    /// 获取消息 ID
    pub fn id(&self) -> MessageId {
        match self {
            Self::Human { id, .. } => *id,
            Self::Ai { id, .. } => *id,
            Self::System { id, .. } => *id,
            Self::Tool { id, .. } => *id,
        }
    }

    /// 获取消息 `MessageContent` 引用
    pub fn message_content(&self) -> &MessageContent {
        match self {
            Self::Human { content, .. } => content,
            Self::Ai { content, .. } => content,
            Self::System { content, .. } => content,
            Self::Tool { content, .. } => content,
        }
    }

    /// 获取纯文本内容（拼接所有 text block）
    pub fn content(&self) -> String {
        self.message_content().text_content()
    }

    /// 懒解析为标准 ContentBlock 列表
    ///
    /// 对齐 LangChain JS 的 `message.contentBlocks` 属性。
    pub fn content_blocks(&self) -> Vec<ContentBlock> {
        self.message_content().content_blocks()
    }

    /// 是否包含工具调用
    pub fn has_tool_calls(&self) -> bool {
        match self {
            Self::Ai { tool_calls, .. } => !tool_calls.is_empty(),
            _ => false,
        }
    }

    /// 获取工具调用列表（仅 Ai 变体有效）
    pub fn tool_calls(&self) -> &[ToolCallRequest] {
        match self {
            Self::Ai { tool_calls, .. } => tool_calls,
            _ => &[],
        }
    }

    /// 是否为系统消息
    pub fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    include!("message_test.rs");
}
