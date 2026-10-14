//! MemoryMiddleware - 自动记忆索引注入中间件。

use async_trait::async_trait;
use cc_agent::{
    agent::state::State, error::AgentResult, messages::BaseMessage, middleware::r#trait::Middleware,
};

/// 记忆注入中间件，负责在会话轮次启动前（`before_agent`）将 MEMORY.md 索引前插至会话上下文。
pub struct MemoryMiddleware {
    /// 冻结的渲染后 MEMORY.md 内容（带 provenance 头与有界截断）。
    frozen: Option<String>,
}

impl Default for MemoryMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryMiddleware {
    pub fn new() -> Self {
        Self { frozen: None }
    }

    /// 使用会话启动时捕获的冻结记忆快照
    pub fn with_frozen_entrypoint(mut self, entrypoint: Option<String>) -> Self {
        self.frozen = entrypoint;
        self
    }
}

#[async_trait]
impl<S: State> Middleware<S> for MemoryMiddleware {
    fn name(&self) -> &str {
        "MemoryMiddleware"
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        if let Some(ref content) = self.frozen {
            if !content.trim().is_empty() {
                state.prepend_message(BaseMessage::system(content.clone()));
            }
        } else {
            let entrypoint = super::paths::get_auto_memory_entrypoint(state.cwd());
            if entrypoint.exists() {
                if let Ok(raw) = std::fs::read_to_string(&entrypoint) {
                    let truncated = super::entrypoint::truncate_entrypoint_content(&raw);
                    if !truncated.content.trim().is_empty() {
                        let formatted = format!(
                            "Contents of {} (auto-memory):\n\n{}",
                            entrypoint.display(),
                            truncated.content
                        );
                        state.prepend_message(BaseMessage::system(formatted));
                    }
                }
            }
        }
        Ok(())
    }
}
