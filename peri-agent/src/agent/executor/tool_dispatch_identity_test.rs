use super::*;
use crate::{
    agent::{react::AgentInput, state::AgentState},
    tools::ToolInvocationContext,
};

struct MockIdentityTool;

#[async_trait::async_trait]
impl BaseTool for MockIdentityTool {
    fn name(&self) -> &str {
        "identity"
    }
    fn description(&self) -> &str {
        "读取可信调用身份"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        tokio::task::yield_now().await;
        let identity = ToolInvocationContext::current().ok_or("调度器未注入调用身份")?;
        Ok(format!(
            "{}:{}",
            identity.source_agent_id.as_deref().unwrap_or("main"),
            identity.tool_call_id
        ))
    }
}

struct MockIdentityLLM;

#[async_trait::async_trait]
impl ReactLLM for MockIdentityLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        _: &[&dyn BaseTool],
        _: Option<crate::llm::types::StreamingContext>,
    ) -> AgentResult<Reasoning> {
        if messages
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { .. }))
        {
            Ok(Reasoning::with_answer("done", "done"))
        } else {
            Ok(Reasoning::with_tools(
                "parallel",
                vec![
                    ToolCall::new(
                        "first",
                        "identity",
                        serde_json::json!({"tool_call_id":"forged"}),
                    ),
                    ToolCall::new(
                        "second",
                        "identity",
                        serde_json::json!({"source_agent_id":"forged"}),
                    ),
                ],
            ))
        }
    }
}

#[tokio::test]
async fn test_tool_dispatch_preserves_identity_in_concurrent_main_and_child_calls() {
    for source in [None, Some("child-instance")] {
        let agent = ReActAgent::new(MockIdentityLLM)
            .max_iterations(3)
            .register_tool(Box::new(MockIdentityTool));
        let mut state = AgentState::new(".");
        if let Some(source) = source {
            state = state.with_context("source_agent_id", source);
        }
        agent
            .execute(AgentInput::text("go"), &mut state, None)
            .await
            .expect("调度应成功");
        let results: Vec<_> = state
            .messages()
            .iter()
            .filter_map(|message| {
                if let BaseMessage::Tool { tool_call_id, .. } = message {
                    Some((tool_call_id.as_str(), message.content()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(results.len(), 2, "并发调用不能漏 tool_result");
        for (call_id, result) in results {
            assert_eq!(
                result,
                format!("{}:{call_id}", source.unwrap_or("main")),
                "身份必须来自调度器，不能被参数伪造或并发串位"
            );
        }
        assert!(
            ToolInvocationContext::current().is_none(),
            "调用上下文不能泄漏到下一轮"
        );
    }
}
