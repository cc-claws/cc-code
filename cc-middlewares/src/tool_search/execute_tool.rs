//! ExecuteExtraTool 元工具 — 代理执行延迟加载的工具

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::RwLock;
use cc_agent::tools::BaseTool;
use serde_json::{json, Value};

use super::{
    core_tools::{EXECUTE_EXTRA_TOOL_NAME, EXTRA_TOOL_NAME_FIELD, EXTRA_TOOL_PARAMS_FIELD},
    keyword_search::split_camel_case,
};

/// 将 CamelCase 或任意名称规范化为 snake_case 小写
///
/// `CronCreate` → `"cron_create"`
/// `CronRegister` → `"cron_register"`
/// `mcp__slack__send` → `"mcp__slack__send"`
fn normalize_tool_name(name: &str) -> String {
    // 已经是 snake_case（含下划线且全小写）直接返回
    if name.contains('_') && name.chars().all(|c| c == '_' || !c.is_uppercase()) {
        return name.to_lowercase();
    }
    split_camel_case(name).join("_")
}

/// 从注册表中模糊查找工具，支持大小写不敏感、CamelCase↔snake_case、首词前缀匹配
fn resolve_tool(
    tools: &HashMap<String, Arc<dyn BaseTool>>,
    tool_name: &str,
) -> Option<Arc<dyn BaseTool>> {
    // 1. 精确匹配
    if let Some(tool) = tools.get(tool_name) {
        return Some(tool.clone());
    }
    let input_lower = tool_name.to_lowercase();
    let input_norm = normalize_tool_name(tool_name);
    let input_prefix = input_norm.split('_').next().unwrap_or(&input_norm);

    // 2. 大小写不敏感 + 规范化匹配
    for (key, tool) in tools {
        if key.to_lowercase() == input_lower {
            return Some(tool.clone());
        }
        if normalize_tool_name(key) == input_norm {
            return Some(tool.clone());
        }
    }

    // 3. 首词前缀匹配（CronCreate → cron → cron_register）
    let mut prefix_match = None;
    for (key, tool) in tools {
        let key_norm = normalize_tool_name(key);
        let key_prefix = key_norm.split('_').next().unwrap_or(&key_norm);
        if key_prefix == input_prefix {
            if prefix_match.is_some() {
                // 多个同前缀工具，歧义，不匹配
                return None;
            }
            prefix_match = Some(tool.clone());
        }
    }
    prefix_match
}

/// 代理执行延迟加载工具的元工具
///
/// LLM 通过 SearchExtraTools 发现工具后，使用此工具代理调用。
/// 输入目标工具名称和参数，从共享工具注册表中查找并执行。
pub struct ExecuteExtraTool {
    /// 共享工具注册表（由 executor 在工具收集后填充）
    shared_tools: Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
}

impl ExecuteExtraTool {
    pub fn new(shared_tools: Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>) -> Self {
        Self { shared_tools }
    }
}

#[async_trait]
impl BaseTool for ExecuteExtraTool {
    fn name(&self) -> &str {
        EXECUTE_EXTRA_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Invoke a registered deferred tool using tool_name and params. Use the exact name and input schema returned by SearchExtraTools. The target may operate locally or through an external service; its side effects and existing permission checks still apply. This wrapper does not grant additional authorization. Call tools already exposed to you directly."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tool_name": {
                    "type": "string",
                    "description": "REQUIRED. The exact target tool name returned by SearchExtraTools (e.g., \"CronRegister\", \"mcp__server__action\")"
                },
                "params": {
                    "type": "object",
                    "description": "REQUIRED. Parameters matching the discovered target tool's input schema"
                }
            },
            "required": ["tool_name", "params"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let tool_name = input
            .get(EXTRA_TOOL_NAME_FIELD)
            .and_then(|v| v.as_str())
            .ok_or(format!(
                "{}: missing required '{}' parameter",
                EXECUTE_EXTRA_TOOL_NAME, EXTRA_TOOL_NAME_FIELD
            ))?;

        let params = input
            .get(EXTRA_TOOL_PARAMS_FIELD)
            .ok_or(format!(
                "{}: missing required '{}' parameter",
                EXECUTE_EXTRA_TOOL_NAME, EXTRA_TOOL_PARAMS_FIELD
            ))?
            .clone();

        let tool = {
            let tools = self.shared_tools.read();
            resolve_tool(&tools, tool_name).ok_or(format!(
                "{}: tool '{}' not found or not registered as a deferred tool",
                EXECUTE_EXTRA_TOOL_NAME, tool_name
            ))?
        };

        let result = tool.invoke(params).await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("execute_tool_test.rs");
}
