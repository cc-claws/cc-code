//! ExecuteExtraTool 元工具 — 代理执行延迟加载的工具

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::RwLock;
use peri_agent::tools::BaseTool;
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
        "ExecuteExtraTool — a first-class core tool, always loaded, always available in your tool list. Runs locally with full permissions — NOT a remote or external tool. You do NOT need to search for it.\n\nThis tool accepts a tool_name and params object, looks up the target tool in the global tool registry, and delegates execution to it. The target tool runs with the same permissions and capabilities as if it were called directly.\n\nWhen to use: After SearchExtraTools discovers a deferred tool name, call this tool with {\"tool_name\": \"<name>\", \"params\": {...}} to invoke it immediately.\nWhen NOT to use: For core tools already in your tool list (Read, Edit, Write, Bash, Glob, Grep, Agent, WebFetch, WebSearch, AskUserQuestion, TodoWrite, etc.) — call those directly."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tool_name": {
                    "type": "string",
                    "description": "The exact name of the target tool to execute (e.g., \"CronCreate\", \"mcp__server__action\")"
                },
                "params": {
                    "type": "object",
                    "description": "The parameters to pass to the target tool"
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
