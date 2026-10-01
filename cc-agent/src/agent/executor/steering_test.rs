use super::*;
use crate::{
    agent::{state::AgentState, steering::SteeringQueue},
    llm::types::StreamingContext,
    messages::{ContentBlock, MessageContent},
};
use tokio::sync::{oneshot, Notify};

struct MockSteeringLlm {
    entered: Arc<Notify>,
    proceed: Arc<Notify>,
    tools: bool,
    seen: Arc<parking_lot::Mutex<Vec<Vec<BaseMessage>>>>,
}

#[async_trait::async_trait]
impl ReactLLM for MockSteeringLlm {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        let first = {
            let mut seen = self.seen.lock();
            seen.push(messages.to_vec());
            seen.len() == 1
        };
        if first {
            if self.tools {
                return Ok(Reasoning::with_tools(
                    "读取",
                    vec![ToolCall::new("t1", "wait", serde_json::json!({}))],
                ));
            }
            self.entered.notify_one();
            self.proceed.notified().await;
            return Ok(Reasoning::with_answer("", "旧回答"));
        }
        Ok(Reasoning::with_answer("", "已结合补充"))
    }
}

struct MockWaitingTool {
    entered: Arc<Notify>,
    proceed: Arc<Notify>,
}

#[async_trait::async_trait]
impl BaseTool for MockWaitingTool {
    fn name(&self) -> &str {
        "wait"
    }
    fn description(&self) -> &str {
        "测试等待工具"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.entered.notify_one();
        self.proceed.notified().await;
        Ok("工具已完成".to_string())
    }
}

async fn check_steering_during_execution(tools: bool) {
    let entered = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let snapshots = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let queue = SteeringQueue::default();
    let agent = ReActAgent::new(MockSteeringLlm {
        entered: entered.clone(),
        proceed: proceed.clone(),
        tools,
        seen: seen.clone(),
    })
    .register_tool(Box::new(MockWaitingTool {
        entered: entered.clone(),
        proceed: proceed.clone(),
    }))
    .with_steering(queue.clone())
    .with_system_prompt("冻结系统提示")
    .with_event_handler(Arc::new(crate::agent::FnEventHandler({
        let snapshots = snapshots.clone();
        move |event| {
            if let AgentEvent::StateSnapshot(messages) = event {
                snapshots.lock().extend(messages);
            }
        }
    })));
    let execution = tokio::spawn(async move {
        let mut state = AgentState::new(".");
        let output = agent
            .execute(AgentInput::text("开始"), &mut state, None)
            .await;
        (output, state)
    });
    entered.notified().await;
    let content = MessageContent::blocks(vec![
        ContentBlock::text("看这张图"),
        ContentBlock::image_base64("image/png", "aW1hZ2U="),
    ]);
    let mut receipt = queue.enqueue(content.clone()).expect("执行中应可补充");
    assert!(
        matches!(receipt.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "安全边界前不得确认"
    );
    proceed.notify_one();
    assert!(receipt.await.is_ok(), "边界消费后应确认");
    let (output, state) = execution.await.expect("执行任务正常完成");
    assert_eq!(output.expect("Agent 正常返回").text, "已结合补充");
    let calls = seen.lock();
    assert_eq!(calls.len(), 2, "最终回答期间收到补充也必须继续调用模型");
    assert_eq!(
        calls[1].last().expect("应有补充消息").message_content(),
        &content
    );
    assert_eq!(
        calls[0][0].message_content(),
        calls[1][0].message_content(),
        "系统提示必须不变"
    );
    if tools {
        let preceding = &calls[1][calls[1].len() - 2];
        assert!(
            matches!(preceding, BaseMessage::Tool { .. }),
            "补充必须位于完整工具结果后"
        );
    }
    assert!(
        snapshots
            .lock()
            .iter()
            .any(|message| message.message_content() == &content),
        "补充图片必须进入 UI 快照"
    );
    assert!(state
        .messages()
        .iter()
        .any(|message| message.message_content() == &content));
    assert!(queue.enqueue(MessageContent::text("已结束")).is_none());
}

#[tokio::test]
async fn test_steering_during_tool_preserves_images_and_tool_results() {
    check_steering_during_execution(true).await;
}

#[tokio::test]
async fn test_steering_during_final_answer_continues_model() {
    check_steering_during_execution(false).await;
}
