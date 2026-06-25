//! ToolSearchMiddleware — 注册元工具并注入延迟工具列表到 system prompt

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::RwLock;
use peri_agent::{
    agent::state::State, error::AgentResult, messages::BaseMessage,
    middleware::r#trait::Middleware, tools::BaseTool,
};

use super::{
    execute_tool::ExecuteExtraTool, search_tool::SearchExtraTools, tool_index::ToolSearchIndex,
};

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
        // 检查 shared_tools 是否有变化（MCP 后续连接等场景）
        let tools = self.shared_tools.read();
        let deferred_arcs: Vec<Arc<dyn BaseTool>> = tools
            .iter()
            .filter(|(name, _)| {
                !super::core_tools::CORE_TOOLS.contains(name.as_str())
                    && !super::core_tools::META_TOOLS.contains(name.as_str())
            })
            .map(|(_, tool)| Arc::clone(tool))
            .collect();
        drop(tools);

        let old_count = self.tool_search_index.total_count();
        let should_rebuild = !deferred_arcs.is_empty()
            && (self.tool_search_index.cached_prompt().is_none()
                || old_count != deferred_arcs.len());

        if should_rebuild {
            self.tool_search_index.build(deferred_arcs);
            let new_count = self.tool_search_index.total_count();
            if old_count > 0 && new_count != old_count {
                state.push_recall(format!(
                    "[ToolSearch] Deferred tools updated: {} tools available (was {})",
                    new_count, old_count
                ));
            }
            let list = self.tool_search_index.format_deferred_list();
            if !list.is_empty() {
                self.tool_search_index.set_cached_prompt(list);
            }
        }

        // 每轮都注入缓存的提示词（System 消息在 agent 完成后被过滤，
        // 不写入 agent_state_messages，所以每轮需重新注入以保证前缀一致）
        if let Some(cached) = self.tool_search_index.cached_prompt() {
            state.prepend_message(BaseMessage::system(cached));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("middleware_test.rs");
}
