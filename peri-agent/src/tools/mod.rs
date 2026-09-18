use serde::{Deserialize, Serialize};

use crate::messages::{ContentBlock, MessageContent};

/// 工具定义（JSON Schema 格式参数描述）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// 工具执行结果——支持纯文本和多模态（如图片）返回
#[derive(Debug, Clone)]
pub struct ToolContent {
    /// 纯文本输出（用于 TUI 事件展示、日志、遥测）
    pub output: String,
    /// 结构化内容（用于写入 state 发送给 LLM），None 时回退到 output 纯文本
    pub content: Option<MessageContent>,
}

impl ToolContent {
    /// 纯文本结果（绝大多数工具的默认路径）
    pub fn text(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            content: None,
        }
    }

    /// 多模态结果：output 用于展示摘要，content 携带结构化数据（如图片 blocks）
    pub fn rich(output: impl Into<String>, content: MessageContent) -> Self {
        Self {
            output: output.into(),
            content: Some(content),
        }
    }

    /// 图片结果的便捷构造器
    pub fn image(
        media_type: impl Into<String>,
        base64_data: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        let blocks = vec![ContentBlock::image_base64(media_type, base64_data)];
        Self::rich(summary, MessageContent::blocks(blocks))
    }
}

/// BaseTool trait - 对齐 LangChain Python BaseTool
///
/// 所有工具必须实现此 trait，不再依赖 langchain-rust::tools::Tool。
#[async_trait::async_trait]
pub trait BaseTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;

    /// 返回完整工具定义（默认实现，组合 name/description/parameters）
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }

    /// 执行工具，输入为 JSON Value
    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;

    /// 执行工具并返回结构化内容（支持多模态）。
    ///
    /// 默认实现委托给 `invoke()` 并包装为纯文本 `ToolContent`。
    /// 需要返回图片等多模态内容的工具（如 ReadFileTool）应覆写此方法。
    async fn invoke_content(
        &self,
        input: serde_json::Value,
    ) -> Result<ToolContent, Box<dyn std::error::Error + Send + Sync>> {
        self.invoke(input).await.map(ToolContent::text)
    }
}
