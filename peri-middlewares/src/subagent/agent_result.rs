use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::json;

const AGENT_RESULT_DESCRIPTION: &str = "Compatibility marker for automatically delivered background agent results. This tool cannot query or poll task status or output, including Bash tasks. Do not call it directly or through ExecuteExtraTool; wait for the background completion notification instead.";

const AGENT_RESULT_UNSUPPORTED: &str = "AgentResult does not support queries or polling. No task status or output was checked. Background results are delivered automatically on completion; continue other work or wait for that notification. Do not retry AgentResult.";

/// AgentResult 工具：兼容后台结果合成消息中的工具名。
///
/// 实际的后台任务结果通过合成消息注入（tool_use + tool_result），
/// 此工具的 invoke 不执行真实查询，仅作为工具定义占位使 LLM
/// 能识别 AgentResult 类型的 tool_use 块。
pub struct AgentResultTool;

impl Default for AgentResultTool {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentResultTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl BaseTool for AgentResultTool {
    fn name(&self) -> &str {
        "AgentResult"
    }

    fn description(&self) -> &str {
        AGENT_RESULT_DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Legacy optional task ID retained for compatibility. Supplying or omitting it does not enable queries; direct invocation is unsupported."
                }
            }
        })
    }

    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, AGENT_RESULT_UNSUPPORTED).into())
    }
}

#[cfg(test)]
#[path = "agent_result_test.rs"]
mod tests;
