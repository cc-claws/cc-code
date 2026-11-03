//! 快照锚点在「消息被整体替换」（full compact）后的行为回归测试。
//!
//! 背景：compact 用全新 id 的消息替换整段历史后，锚点消息不复存在，
//! `index_after_id` 会 fallback 到 0，把压缩后的历史当成新增消息重发。
//! 客户端已在 `CompactCompleted` 里收到这份历史，重发会导致摘要消息重复渲染。

use super::*;
use crate::{
    agent::{react::Reasoning, state::AgentState},
    messages::BaseMessage,
    tools::BaseTool,
};

const REPLACED_SUMMARY: &str = "压缩后的摘要";

/// 首轮 `before_model` 模拟 full compact：整体替换消息，全部 id 都是新的。
struct ReplacingMiddleware {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl Middleware<AgentState> for ReplacingMiddleware {
    fn name(&self) -> &str {
        "replacing_middleware"
    }

    async fn before_model(&self, state: &mut AgentState) -> AgentResult<()> {
        if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            *state.messages_mut() = vec![BaseMessage::human(REPLACED_SUMMARY)];
        }
        Ok(())
    }
}

/// 第一轮要求调用工具（触发工具后的 StateSnapshot），第二轮给出最终答案。
struct ReplacingHistoryLlm;

#[async_trait::async_trait]
impl ReactLLM for ReplacingHistoryLlm {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
        _streaming: Option<crate::llm::types::StreamingContext>,
    ) -> AgentResult<Reasoning> {
        let has_tool_result = messages
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { .. }));
        if has_tool_result {
            Ok(Reasoning::with_answer("完成", "done"))
        } else {
            Ok(Reasoning::with_tools(
                "先调用工具",
                vec![ToolCall::new("call-1", "echo_tool", serde_json::json!({}))],
            ))
        }
    }
}

struct EchoTool;

#[async_trait::async_trait]
impl BaseTool for EchoTool {
    fn name(&self) -> &str {
        "echo_tool"
    }

    fn description(&self) -> &str {
        "echo"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("echo".to_string())
    }
}

struct SnapshotCollector(Arc<parking_lot::Mutex<Vec<Vec<BaseMessage>>>>);

impl AgentEventHandler for SnapshotCollector {
    fn on_event(&self, event: AgentEvent) {
        if let AgentEvent::StateSnapshot(messages) = event {
            self.0.lock().push(messages);
        }
    }
}

#[tokio::test]
async fn test_snapshot_excludes_history_replaced_by_compact() {
    let snapshots = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let agent = ReActAgent::new(ReplacingHistoryLlm)
        .add_middleware(Box::new(ReplacingMiddleware {
            calls: std::sync::atomic::AtomicUsize::new(0),
        }))
        .register_tool(Box::new(EchoTool))
        .with_event_handler(Arc::new(SnapshotCollector(snapshots.clone())));
    let mut state = AgentState::new(".");
    let result = agent
        .execute(AgentInput::text("原任务"), &mut state, None)
        .await;
    assert!(result.is_ok(), "执行应成功: {result:?}");
    let snapshots = snapshots.lock();
    assert!(!snapshots.is_empty(), "应产生状态快照");
    let dumped: Vec<Vec<String>> = snapshots
        .iter()
        .map(|messages| messages.iter().map(|m| m.content()).collect())
        .collect();
    assert!(
        snapshots.iter().all(|messages| messages
            .iter()
            .all(|m| !m.content().contains(REPLACED_SUMMARY))),
        "被 compact 替换的历史不得经快照重发，实际快照: {dumped:?}"
    );
    assert!(
        snapshots.iter().any(|messages| messages
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { .. }))),
        "工具结果仍须经快照送达客户端，实际快照: {dumped:?}"
    );
}
