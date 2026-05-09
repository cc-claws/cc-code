//! ToolSearchMiddleware — 注册元工具并注入延迟工具列表到 system prompt

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::AgentResult;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::BaseTool;

use super::execute_tool::ExecuteExtraTool;
use super::search_tool::SearchExtraTools;
use super::tool_index::ToolSearchIndex;

/// ToolSearch 中间件
///
/// 职责：
/// 1. 注册 SearchExtraTools 和 ExecuteExtraTool 两个元工具
/// 2. 在 before_agent 时注入延迟工具列表到 system prompt
pub struct ToolSearchMiddleware {
    tool_search_index: Arc<ToolSearchIndex>,
    shared_tools: Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
}

impl ToolSearchMiddleware {
    pub fn new(
        tool_search_index: Arc<ToolSearchIndex>,
        shared_tools: Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
    ) -> Self {
        Self {
            tool_search_index,
            shared_tools,
        }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for ToolSearchMiddleware {
    fn name(&self) -> &str {
        "ToolSearch"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(SearchExtraTools::new(Arc::clone(&self.tool_search_index))),
            Box::new(ExecuteExtraTool::new(Arc::clone(&self.shared_tools))),
        ]
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 首次调用时，从 shared_tools 构建 deferred 工具索引
        // executor 在 before_agent 之前已将所有工具写入 shared_tools
        if self.tool_search_index.total_count() == 0 {
            let tools = self.shared_tools.read();
            let deferred_arcs: Vec<Arc<dyn BaseTool>> = tools
                .iter()
                .filter(|(name, _)| {
                    !super::core_tools::CORE_TOOLS.contains(name.as_str())
                        && !super::core_tools::META_TOOLS.contains(name.as_str())
                })
                .map(|(_, tool)| Arc::clone(tool))
                .collect();
            if !deferred_arcs.is_empty() {
                self.tool_search_index.build(deferred_arcs);
            }
        }

        let list = self.tool_search_index.format_deferred_list();
        if !list.is_empty() {
            state.prepend_message(BaseMessage::system(list));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool {
        name_str: String,
        desc_str: String,
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
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
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("mock".to_string())
        }
    }

    fn build_test_components() -> (
        Arc<ToolSearchIndex>,
        Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
    ) {
        let index = Arc::new(ToolSearchIndex::new());
        index.build(vec![
            Arc::new(MockTool::new("CronRegister", "Register a cron task")),
            Arc::new(MockTool::new("mcp__slack__send", "Send Slack message")),
        ]);

        let mut shared = HashMap::new();
        shared.insert(
            "CronRegister".to_string(),
            Arc::new(MockTool::new("CronRegister", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        shared.insert(
            "mcp__slack__send".to_string(),
            Arc::new(MockTool::new("mcp__slack__send", "Send Slack message")) as Arc<dyn BaseTool>,
        );

        (index, Arc::new(RwLock::new(shared)))
    }

    #[test]
    fn test_collect_tools_returns_meta_tools() {
        let (index, shared) = build_test_components();
        let mw = ToolSearchMiddleware::new(index, shared);
        let tools = <ToolSearchMiddleware as Middleware<
            rust_create_agent::agent::state::AgentState,
        >>::collect_tools(&mw, "/tmp");

        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"SearchExtraTools"));
        assert!(names.contains(&"ExecuteExtraTool"));
    }

    #[tokio::test]
    async fn test_before_agent_injects_system_prompt() {
        let (index, shared) = build_test_components();
        let mw = ToolSearchMiddleware::new(index, shared);

        let mut state = rust_create_agent::agent::state::AgentState::new("/tmp");
        mw.before_agent(&mut state).await.unwrap();

        let messages = state.messages();
        assert!(!messages.is_empty(), "before_agent 应注入 system 消息");
        let first = messages.first().unwrap();
        assert!(
            matches!(first, BaseMessage::System { .. }),
            "第一条消息应为 System"
        );
        assert!(
            first.content().contains("CronRegister"),
            "system 消息应包含延迟工具列表"
        );
    }
}
