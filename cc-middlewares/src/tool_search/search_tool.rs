//! SearchExtraTools 元工具 — 搜索并发现延迟加载的工具

use std::sync::Arc;

use async_trait::async_trait;
use cc_agent::tools::BaseTool;
use serde_json::{json, Value};

use super::{core_tools::SEARCH_EXTRA_TOOLS_NAME, tool_index::ToolSearchIndex};

/// 搜索延迟加载工具的元工具
///
/// LLM 通过此工具发现不在直接工具列表中的 deferred tools，
/// 获取完整 schema 后通过 ExecuteExtraTool 调用。
pub struct SearchExtraTools {
    index: Arc<ToolSearchIndex>,
}

impl SearchExtraTools {
    pub fn new(index: Arc<ToolSearchIndex>) -> Self {
        Self { index }
    }
}

#[async_trait]
impl BaseTool for SearchExtraTools {
    fn name(&self) -> &str {
        SEARCH_EXTRA_TOOLS_NAME
    }

    fn description(&self) -> &str {
        "Discover deferred tools when the tools already exposed do not provide the required capability, or when the user requests a specific integration. Returns matching names, descriptions, and full JSON schemas.\n\nUse keyword queries such as \"slack send\", or \"select:CronRegister,CronList\" for exact names. Discovery does not execute or authorize an action. When a returned tool is needed and authorized, invoke it through ExecuteExtraTool using its exact name and schema. Call tools already exposed to you directly."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("SearchExtraTools: missing required 'query' parameter")?;

        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let results = self.index.search(query, max_results);
        let total = self.index.total_count();
        let output = json!({
            "results": results,
            "total_available": total
        });

        Ok(serde_json::to_string(&output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_search::tool_index::ToolSearchIndex;
    include!("search_tool_test.rs");
}
