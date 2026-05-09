//! ExecuteExtraTool 元工具 — 代理执行延迟加载的工具

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use rust_create_agent::tools::BaseTool;
use serde_json::{json, Value};

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
        "ExecuteExtraTool"
    }

    fn description(&self) -> &str {
        "代理执行延迟加载的工具。输入目标工具名称和参数，返回执行结果。使用 SearchExtraTools 发现可用工具。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tool_name": {
                    "type": "string",
                    "description": "延迟加载工具的名称"
                },
                "params": {
                    "type": "object",
                    "description": "目标工具的参数"
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
            .get("tool_name")
            .and_then(|v| v.as_str())
            .ok_or("ExecuteExtraTool: 缺少 tool_name 参数")?;

        let params = input
            .get("params")
            .ok_or("ExecuteExtraTool: 缺少 params 参数")?
            .clone();

        let tool = {
            let tools = self.shared_tools.read();
            tools.get(tool_name).cloned().ok_or(format!(
                "ExecuteExtraTool: 工具 '{}' 不存在或未注册为延迟工具",
                tool_name
            ))?
        };

        let result = tool.invoke(params).await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool {
        name_str: String,
        desc_str: String,
        should_fail: bool,
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
                should_fail: false,
            }
        }

        fn new_failing(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl BaseTool for MockTool {
        fn name(&self) -> &str {
            &self.name_str
        }
        fn description(&self) -> &str {
            &self.desc_str
        }
        fn parameters(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }
        async fn invoke(
            &self,
            _input: Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            if self.should_fail {
                Err("mock tool error".into())
            } else {
                Ok(format!("{} executed", self.name_str))
            }
        }
    }

    fn build_test_registry() -> Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>> {
        let mut map = HashMap::new();
        map.insert(
            "CronRegister".to_string(),
            Arc::new(MockTool::new("CronRegister", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        map.insert(
            "mcp__slack__send_message".to_string(),
            Arc::new(MockTool::new(
                "mcp__slack__send_message",
                "Send Slack message",
            )) as Arc<dyn BaseTool>,
        );
        map.insert(
            "FailingTool".to_string(),
            Arc::new(MockTool::new_failing(
                "FailingTool",
                "A tool that always fails",
            )) as Arc<dyn BaseTool>,
        );
        Arc::new(RwLock::new(map))
    }

    #[test]
    fn test_tool_name_is_execute_extra_tool() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);
        assert_eq!(tool.name(), "ExecuteExtraTool");
    }

    #[test]
    fn test_parameters_schema() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["tool_name"].is_object());
        assert!(params["properties"]["params"].is_object());
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("tool_name")));
        assert!(required.contains(&json!("params")));
    }

    #[tokio::test]
    async fn test_invoke_executes_deferred_tool() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool
            .invoke(json!({"tool_name": "CronRegister", "params": {"expression": "* * * * *", "prompt": "test"}}))
            .await
            .unwrap();
        assert_eq!(result, "CronRegister executed");
    }

    #[tokio::test]
    async fn test_tool_not_found_returns_error() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool
            .invoke(json!({"tool_name": "UnknownTool", "params": {}}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("不存在或未注册为延迟工具"));
    }

    #[tokio::test]
    async fn test_missing_tool_name() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool.invoke(json!({"params": {}})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("缺少 tool_name 参数"));
    }

    #[tokio::test]
    async fn test_missing_params() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool.invoke(json!({"tool_name": "CronRegister"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("缺少 params 参数"));
    }

    #[tokio::test]
    async fn test_target_tool_error_propagates() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool
            .invoke(json!({"tool_name": "FailingTool", "params": {}}))
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "mock tool error");
    }
}
