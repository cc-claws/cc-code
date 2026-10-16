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
    assert!(
        snapshots
            .lock()
            .iter()
            .any(|message| message.message_content() == &content),
        "确认消费前必须已发送包含补充信息的状态快照"
    );
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

struct MockCompactingSteeringMiddleware {
    calls: std::sync::atomic::AtomicUsize,
    compact_step: usize,
    entered: Arc<Notify>,
    proceed: Arc<Notify>,
    fail: bool,
}

#[async_trait::async_trait]
impl Middleware<AgentState> for MockCompactingSteeringMiddleware {
    fn name(&self) -> &str {
        "mock_compacting_steering"
    }
    async fn before_model(&self, state: &mut AgentState) -> AgentResult<()> {
        let step = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if step == self.compact_step {
            self.entered.notify_one();
            self.proceed.notified().await;
            if self.fail {
                return Err(AgentError::MiddlewareError {
                    middleware: self.name().to_string(),
                    reason: "模拟压缩检查点失败".to_string(),
                });
            }
            *state.messages_mut() = vec![BaseMessage::human("压缩后的摘要")];
        }
        Ok(())
    }
}

struct MockRecordingSteeringLlm {
    seen: Arc<parking_lot::Mutex<Vec<Vec<BaseMessage>>>>,
}

#[async_trait::async_trait]
impl ReactLLM for MockRecordingSteeringLlm {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        self.seen.lock().push(messages.to_vec());
        Ok(Reasoning::with_answer("", "完成"))
    }
}

#[tokio::test]
async fn test_steering_initial_message_survives_compaction_rewrite() {
    let entered = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let queue = SteeringQueue::default();
    let content =
        MessageContent::text("<background-task-completed>initial-task</background-task-completed>");
    let mut receipt = queue
        .enqueue(content.clone())
        .expect("执行开始前应允许入队");
    let agent = ReActAgent::new(MockRecordingSteeringLlm { seen: seen.clone() })
        .add_middleware(Box::new(MockCompactingSteeringMiddleware {
            calls: std::sync::atomic::AtomicUsize::new(0),
            compact_step: 0,
            entered: entered.clone(),
            proceed: proceed.clone(),
            fail: false,
        }))
        .with_steering(queue);
    let execution = tokio::spawn(async move {
        let mut state = AgentState::new(".");
        agent
            .execute(AgentInput::text("原任务"), &mut state, None)
            .await
    });
    entered.notified().await;
    assert!(
        matches!(receipt.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "压缩完成前不得确认消费"
    );
    proceed.notify_one();
    assert!(receipt.await.is_ok(), "压缩后应消费并确认");
    assert!(execution.await.expect("执行任务正常结束").is_ok());
    let calls = seen.lock();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].last().expect("应存在补充通知").message_content(),
        &content,
        "模型必须看到未经摘要改写的通知原文"
    );
}

#[tokio::test]
async fn test_steering_final_answer_message_survives_compaction_rewrite() {
    let model_entered = Arc::new(Notify::new());
    let model_proceed = Arc::new(Notify::new());
    let compact_entered = Arc::new(Notify::new());
    let compact_proceed = Arc::new(Notify::new());
    let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let queue = SteeringQueue::default();
    let agent = ReActAgent::new(MockSteeringLlm {
        entered: model_entered.clone(),
        proceed: model_proceed.clone(),
        tools: false,
        seen: seen.clone(),
    })
    .add_middleware(Box::new(MockCompactingSteeringMiddleware {
        calls: std::sync::atomic::AtomicUsize::new(0),
        compact_step: 1,
        entered: compact_entered.clone(),
        proceed: compact_proceed.clone(),
        fail: false,
    }))
    .with_steering(queue);
    let active_queue = agent.steering.as_ref().expect("应存在补充队列").clone();
    let execution = tokio::spawn(async move {
        let mut state = AgentState::new(".");
        let result = agent
            .execute(AgentInput::text("原任务"), &mut state, None)
            .await;
        (result, state)
    });
    model_entered.notified().await;
    let content =
        MessageContent::text("<background-task-completed>late-task</background-task-completed>");
    let mut receipt = active_queue
        .enqueue(content.clone())
        .expect("最终回答期间应允许入队");
    model_proceed.notify_one();
    compact_entered.notified().await;
    assert!(
        matches!(receipt.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "最终回答后须等待下一次压缩结束再确认消费"
    );
    compact_proceed.notify_one();
    assert!(receipt.await.is_ok());
    let (result, state) = execution.await.expect("执行任务正常结束");
    assert!(result.is_ok());
    let calls = seen.lock();
    assert_eq!(calls.len(), 2, "迟到通知应继续当前轮次");
    assert_eq!(
        calls[1].last().expect("应存在补充通知").message_content(),
        &content,
        "压缩后模型必须看到通知原文"
    );
    assert_eq!(
        state
            .messages()
            .iter()
            .filter(|message| message.message_content() == &content)
            .count(),
        1,
        "通知只能写入历史一次"
    );
}

#[tokio::test]
async fn test_steering_final_answer_message_is_rejected_when_compaction_fails() {
    let model_entered = Arc::new(Notify::new());
    let model_proceed = Arc::new(Notify::new());
    let compact_entered = Arc::new(Notify::new());
    let compact_proceed = Arc::new(Notify::new());
    let queue = SteeringQueue::default();
    let agent = ReActAgent::new(MockSteeringLlm {
        entered: model_entered.clone(),
        proceed: model_proceed.clone(),
        tools: false,
        seen: Arc::new(parking_lot::Mutex::new(Vec::new())),
    })
    .add_middleware(Box::new(MockCompactingSteeringMiddleware {
        calls: std::sync::atomic::AtomicUsize::new(0),
        compact_step: 1,
        entered: compact_entered.clone(),
        proceed: compact_proceed.clone(),
        fail: true,
    }))
    .with_steering(queue.clone());
    let execution = tokio::spawn(async move {
        let mut state = AgentState::new(".");
        let result = agent
            .execute(AgentInput::text("原任务"), &mut state, None)
            .await;
        (result, state)
    });
    model_entered.notified().await;
    let content = MessageContent::text("检查点失败后应保留重试的通知");
    let receipt = queue.enqueue(content.clone()).expect("执行中应允许入队");
    model_proceed.notify_one();
    compact_entered.notified().await;
    compact_proceed.notify_one();
    let (result, state) = execution.await.expect("执行任务正常返回错误");
    assert!(matches!(result, Err(AgentError::MiddlewareError { .. })));
    assert!(
        receipt.await.is_err(),
        "未写入历史的暂存通知必须拒绝，让调用方重试"
    );
    assert!(!state
        .messages()
        .iter()
        .any(|message| message.message_content() == &content));
    assert!(queue.enqueue(MessageContent::text("已结束")).is_none());
}

#[tokio::test]
async fn test_steering_pending_message_is_rejected_on_cancellation() {
    let entered = Arc::new(Notify::new());
    let proceed = Arc::new(Notify::new());
    let queue = SteeringQueue::default();
    let cancel = CancellationToken::new();
    let agent = ReActAgent::new(MockSteeringLlm {
        entered: entered.clone(),
        proceed,
        tools: false,
        seen: Arc::new(parking_lot::Mutex::new(Vec::new())),
    })
    .with_steering(queue.clone());
    let execution_cancel = cancel.clone();
    let execution = tokio::spawn(async move {
        let mut state = AgentState::new(".");
        let result = agent
            .execute(
                AgentInput::text("原任务"),
                &mut state,
                Some(execution_cancel),
            )
            .await;
        (result, state)
    });
    entered.notified().await;
    let content = MessageContent::text("取消前尚未消费的通知");
    let receipt = queue.enqueue(content.clone()).expect("执行中应允许入队");
    cancel.cancel();
    let (result, state) = execution.await.expect("取消任务应正常收尾");
    assert!(matches!(result, Err(AgentError::Interrupted)));
    assert!(receipt.await.is_err(), "取消时不得误报尚未消费的通知成功");
    assert!(!state
        .messages()
        .iter()
        .any(|message| message.message_content() == &content));
    assert!(queue.enqueue(MessageContent::text("已结束")).is_none());
}
