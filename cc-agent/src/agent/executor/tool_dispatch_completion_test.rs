use super::*;
use crate::agent::{events::FnEventHandler, state::AgentState};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::oneshot;

struct FastTool;

#[async_trait::async_trait]
impl BaseTool for FastTool {
    fn name(&self) -> &str {
        "Edit"
    }
    fn description(&self) -> &str {
        "立即完成的快工具"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("fast-done".into())
    }
}

/// 等到放行信号才结束，模拟跑长测试的 Bash。
struct SlowTool {
    gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
}

#[async_trait::async_trait]
impl BaseTool for SlowTool {
    fn name(&self) -> &str {
        "Bash"
    }
    fn description(&self) -> &str {
        "等待放行才结束的慢工具"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let rx = self
            .gate
            .lock()
            .expect("读取闸门")
            .take()
            .expect("闸门只取一次");
        let _ = rx.await;
        Ok("slow-done".into())
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

async fn wait_for_fast_end(events: Arc<Mutex<Vec<AgentEvent>>>) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if events.lock().expect("读取事件").iter().any(|event| {
                matches!(event, AgentEvent::ToolEnd { tool_call_id, .. } if tool_call_id == "fast-call")
            }) {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("快工具的 ToolEnd 应在慢工具结束前出现");
}

/// 回归：快工具（Update/Edit）已落盘完成时，ToolEnd 不能被同批长跑工具
/// （Bash 跑测试）拖住；同时慢工具仍运行时不应提前闭合。
#[tokio::test]
async fn test_fast_tool_end_emitted_while_slow_tool_still_running() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let agent =
        ReActAgent::new(MockUnusedLLM).with_event_handler(Arc::new(FnEventHandler(move |event| {
            sink.lock().expect("记录事件").push(event);
        })));
    let mut state = AgentState::new(".");
    let (tx, rx) = oneshot::channel();
    let gate = Arc::new(Mutex::new(Some(rx)));
    let fast = FastTool;
    let slow = SlowTool {
        gate: Arc::clone(&gate),
    };
    let tools = HashMap::from([
        ("Edit".into(), &fast as &dyn BaseTool),
        ("Bash".into(), &slow as &dyn BaseTool),
    ]);
    let ai_id = MessageId::new();
    let calls = vec![
        ToolCall::new("fast-call", "Edit", serde_json::json!({})),
        ToolCall::new("slow-call", "Bash", serde_json::json!({})),
    ];

    let cancel = CancellationToken::new();
    let mut collect = Box::pin(collect_tool_results(
        &agent, &mut state, calls, &tools, &cancel, ai_id,
    ));

    // 同时驱动 collect 与观察循环：慢工具仍被闸门阻塞，整批不应返回。
    tokio::select! {
        _ = &mut collect => panic!("慢工具仍运行时 collect 不应返回"),
        _ = wait_for_fast_end(Arc::clone(&events)) => {}
    }

    {
        let recorded = events.lock().expect("读取事件");
        assert_eq!(
            recorded
                .iter()
                .filter(|event| matches!(event, AgentEvent::ToolEnd { tool_call_id, .. } if tool_call_id == "fast-call"))
                .count(),
            1,
            "快工具完成时 ToolEnd 应已发出且只发一次"
        );
        assert!(
            !recorded
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolEnd { tool_call_id, .. } if tool_call_id == "slow-call")),
            "慢工具仍在运行，不应提前发 ToolEnd"
        );
        assert!(
            recorded
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolPending { tool_call_id, .. } if tool_call_id == "fast-call")),
            "快工具仍应保留完整的排队态"
        );
    }

    // 放行慢工具，整批结束。
    tx.send(()).expect("放行慢工具");
    let (results, was_cancelled, deferred) = collect.await.expect("工具收集应成功");
    assert!(!was_cancelled, "正常路径不应标记取消");
    assert!(deferred.is_none(), "正常路径不应有延迟错误");
    assert_eq!(results.len(), 2, "不能漏掉工具结果");
    assert_eq!(results[0].0.id, "fast-call", "结果顺序须与调用顺序一致");
    assert_eq!(results[1].0.id, "slow-call", "结果顺序须与调用顺序一致");
    assert!(
        state.messages().is_empty(),
        "收集阶段仍不得写入 state，批量写入由 dispatch_tools 负责"
    );

    let recorded = events.lock().expect("读取事件");
    for id in ["fast-call", "slow-call"] {
        assert_eq!(
            recorded
                .iter()
                .filter(|event| matches!(event, AgentEvent::ToolEnd { tool_call_id, .. } if tool_call_id == id))
                .count(),
            1,
            "每个工具应恰好闭合一次: {id}"
        );
    }
}
