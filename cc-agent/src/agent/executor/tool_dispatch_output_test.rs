use super::*;
use crate::{
    agent::{events::FnEventHandler, state::AgentState},
    tools::ToolOutputContext,
};
use std::sync::Mutex;

struct MockOutputTool;

#[async_trait::async_trait]
impl BaseTool for MockOutputTool {
    fn name(&self) -> &str {
        "Bash"
    }
    fn description(&self) -> &str {
        "测试展示输出与最终结果隔离"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = ToolOutputContext::current().ok_or("缺少输出通道")?.stream();
        stream.push(b"ephemeral-preview\n");
        stream.push(b"more-preview\n");
        Ok("model-visible-final".into())
    }
}

struct MockUnusedLLM;

#[async_trait::async_trait]
impl ReactLLM for MockUnusedLLM {
    async fn generate_reasoning(
        &self,
        _: &[BaseMessage],
        _: &[&dyn BaseTool],
        _: Option<crate::llm::types::StreamingContext>,
    ) -> AgentResult<Reasoning> {
        unreachable!("直接收集工具结果不调用模型")
    }
}

#[tokio::test]
async fn test_tool_output_does_not_write_agent_state_and_preserves_child_identity() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let agent =
        ReActAgent::new(MockUnusedLLM).with_event_handler(Arc::new(FnEventHandler(move |event| {
            sink.lock().expect("记录展示事件").push(event);
        })));
    let mut state = AgentState::new(".").with_context("source_agent_id", "child-instance");
    state.add_message(BaseMessage::human("existing-history"));
    let message_count = state.messages().len();
    let tool = MockOutputTool;
    let tools = HashMap::from([("Bash".into(), &tool as &dyn BaseTool)]);
    let ai_id = MessageId::new();
    let result = collect_tool_results(
        &agent,
        &mut state,
        vec![ToolCall::new("output-call", "Bash", serde_json::json!({}))],
        &tools,
        &CancellationToken::new(),
        ai_id,
    )
    .await
    .expect("工具收集应成功");
    assert_eq!(
        state.messages().len(),
        message_count,
        "增量输出与收集阶段不能写 AgentState"
    );
    assert_eq!(
        result.0[0].1.output, "model-visible-final",
        "模型只收到原始最终结果"
    );
    let events = events.lock().expect("读取展示事件");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::ToolOutput {
        message_id, tool_call_id, tail, source_agent_id: Some(source), ..
    } if *message_id == ai_id && tool_call_id == "output-call" && tail == "ephemeral-preview\n" && source == "child-instance")), "输出必须保留可信 message/call/source 身份");
    let end = events
        .iter()
        .position(|event| matches!(event, AgentEvent::ToolEnd { .. }))
        .expect("工具应结束");
    assert!(
        !events[end + 1..]
            .iter()
            .any(|event| matches!(event, AgentEvent::ToolOutput { .. })),
        "结束后不能输出迟到尾部"
    );
}
