use super::*;
use crate::{
    agent::{events::FnEventHandler, state::AgentState},
    middleware::r#trait::Middleware,
};
use std::sync::{Arc, Mutex};

struct MockUnusedLLM;

#[async_trait::async_trait]
impl ReactLLM for MockUnusedLLM {
    async fn generate_reasoning(
        &self,
        _: &[BaseMessage],
        _: &[&dyn BaseTool],
        _: Option<crate::llm::types::StreamingContext>,
    ) -> AgentResult<Reasoning> {
        unreachable!("本测试直接验证工具收集，不应调用模型")
    }
}

struct MockPendingMiddleware {
    events: Arc<Mutex<Vec<AgentEvent>>>,
    cancel: Option<CancellationToken>,
    fail: bool,
}

#[async_trait::async_trait]
impl Middleware<AgentState> for MockPendingMiddleware {
    fn name(&self) -> &str {
        "MockPendingMiddleware"
    }

    async fn before_tool(&self, state: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        assert!(state.messages().is_empty(), "审批不能提前写入消息");
        let pending_ids = self
            .events
            .lock()
            .expect("读取事件")
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolPending { tool_call_id, .. } => Some(tool_call_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(pending_ids, ["first", "second", "third"], "审批前全部排队");
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
            std::future::pending::<()>().await;
        }
        if self.fail {
            return match call.id.as_str() {
                "second" => Err(AgentError::ToolRejected {
                    tool: call.name.clone(),
                    reason: "拒绝本项".into(),
                }),
                "third" => Err(AgentError::MiddlewareError {
                    middleware: self.name().into(),
                    reason: "审批失败".into(),
                }),
                _ => Ok(call.clone()),
            };
        }
        Ok(call.clone())
    }
}

fn make_pending_agent(
    cancel: Option<CancellationToken>,
    fail: bool,
) -> (
    ReActAgent<MockUnusedLLM, AgentState>,
    Arc<Mutex<Vec<AgentEvent>>>,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let agent = ReActAgent::new(MockUnusedLLM)
        .add_middleware(Box::new(MockPendingMiddleware {
            events: events.clone(),
            cancel,
            fail,
        }))
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            sink.lock().expect("记录事件").push(event);
        })));
    (agent, events)
}

fn make_pending_calls() -> Vec<ToolCall> {
    ["first", "second", "third"]
        .into_iter()
        .map(|id| ToolCall::new(id, "Bash", serde_json::json!({"command": id})))
        .collect()
}

#[tokio::test]
async fn test_tool_pending_emitted_before_before_tool() {
    let (agent, events) = make_pending_agent(None, false);
    let mut state = AgentState::new(".");
    let result = collect_tool_results(
        &agent,
        &mut state,
        make_pending_calls(),
        &HashMap::new(),
        &CancellationToken::new(),
        MessageId::new(),
    )
    .await
    .expect("工具不存在应作为结果收集");
    assert_eq!(result.0.len(), 3, "不能漏掉工具结果");
    assert!(state.messages().is_empty(), "收集阶段不写 state");
    let events = events.lock().expect("读取事件");
    assert!(events[..3]
        .iter()
        .all(|event| matches!(event, AgentEvent::ToolPending { .. })));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolStart { .. }))
            .count(),
        3
    );
}

#[tokio::test]
async fn test_tool_pending_cancelled_emits_tool_end() {
    let cancel = CancellationToken::new();
    let (agent, events) = make_pending_agent(Some(cancel.clone()), false);
    let mut state = AgentState::new(".");
    let result = collect_tool_results(
        &agent,
        &mut state,
        make_pending_calls(),
        &HashMap::new(),
        &cancel,
        MessageId::new(),
    )
    .await;
    assert!(
        matches!(result, Err(AgentError::Interrupted)),
        "审批应被取消"
    );
    assert!(state.messages().is_empty(), "取消审批不能写入 state");
    let events = events.lock().expect("读取事件");
    for call in make_pending_calls() {
        let ends = events
            .iter()
            .filter(|event| {
                matches!(event, AgentEvent::ToolEnd {
            tool_call_id, output, is_error: true, ..
        } if tool_call_id == &call.id && output == "Interrupted by user")
            })
            .count();
        assert_eq!(ends, 1, "每个排队工具应闭合一次：{}", call.id);
    }
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::ToolStart { .. })),
        "取消审批不能进入运行态"
    );
}

#[tokio::test]
async fn test_tool_pending_before_tool_error_closes_unsettled_calls_once() {
    let (agent, events) = make_pending_agent(None, true);
    let mut state = AgentState::new(".");
    let result = collect_tool_results(
        &agent,
        &mut state,
        make_pending_calls(),
        &HashMap::new(),
        &CancellationToken::new(),
        MessageId::new(),
    )
    .await;
    assert!(
        matches!(result, Err(AgentError::MiddlewareError { .. })),
        "应传播审批错误"
    );
    assert!(state.messages().is_empty(), "审批错误不能写入 state");
    let events = events.lock().expect("读取事件");
    for call in make_pending_calls() {
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, AgentEvent::ToolEnd {
            tool_call_id, is_error: true, ..
        } if tool_call_id == &call.id))
                .count(),
            1,
            "包含未轮到的工具且不能重复闭合"
        );
    }
    assert!(!events.iter().any(|event| matches!(event, AgentEvent::ToolStart { tool_call_id, .. } if tool_call_id == "second")), "拒绝应直接结束，不能闪现运行态");
}
