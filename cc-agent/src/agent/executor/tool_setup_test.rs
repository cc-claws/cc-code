use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};

use parking_lot::Mutex;
use serde_json::{json, Value};

use super::*;
use crate::{
    agent::{react::AgentInput, state::AgentState, AgentCancellationToken, ReActAgent},
    error::{AgentError, AgentResult},
    llm::MockLLM,
    middleware::r#trait::Middleware,
};

struct MockOwnedTool {
    payload: Arc<Vec<u8>>,
    dropped: Arc<AtomicUsize>,
}

impl Drop for MockOwnedTool {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl BaseTool for MockOwnedTool {
    fn name(&self) -> &str {
        "OwnedTool"
    }
    fn description(&self) -> &str {
        "持有资源的测试工具"
    }
    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {"value": {"type": "string"}}})
    }
    fn file_snapshot(&self, input: &Value) -> Option<crate::tools::FileSnapshot> {
        Some(crate::tools::FileSnapshot {
            old_content: input["value"].as_str().map(str::to_string),
        })
    }
    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("{}:{}", self.payload.len(), input["value"]))
    }
    async fn invoke_content(
        &self,
        _input: Value,
    ) -> Result<ToolContent, Box<dyn std::error::Error + Send + Sync>> {
        Ok(ToolContent::image("image/png", "dGVzdA==", "测试图片"))
    }
}

#[test]
fn test_box_to_arc_releases_owned_resources_after_last_reference() {
    let payload = Arc::new(vec![42; 1024 * 1024]);
    let weak = Arc::downgrade(&payload);
    let dropped = Arc::new(AtomicUsize::new(0));
    let tool = box_to_arc(Box::new(MockOwnedTool {
        payload,
        dropped: Arc::clone(&dropped),
    }));
    let clone = Arc::clone(&tool);
    drop(tool);
    assert!(weak.upgrade().is_some(), "仍有引用时必须保留工具资源");
    assert_eq!(dropped.load(Ordering::SeqCst), 0, "不能提前析构工具");
    drop(clone);
    assert!(
        weak.upgrade().is_none(),
        "最后一个引用释放后必须释放工具资源"
    );
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "内部工具必须且只能析构一次"
    );
}

#[tokio::test]
async fn test_box_to_arc_preserves_tool_invocation(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tool = box_to_arc(Box::new(MockOwnedTool {
        payload: Arc::new(vec![42]),
        dropped: Arc::new(AtomicUsize::new(0)),
    }));
    assert_eq!(tool.name(), "OwnedTool", "必须保留工具名称");
    assert_eq!(tool.description(), "持有资源的测试工具", "必须保留工具描述");
    assert_eq!(tool.parameters()["properties"]["value"]["type"], "string");
    assert_eq!(
        tool.file_snapshot(&json!({"value": "before"}))
            .and_then(|snapshot| snapshot.old_content),
        Some("before".into()),
        "包装器必须透传执行前快照"
    );
    let output = tool.invoke(json!({"value": "ok"})).await?;
    assert_eq!(output, "1:\"ok\"", "必须透传普通工具调用");
    let content = tool.invoke_content(json!({})).await?;
    assert_eq!(content.output, "测试图片", "必须保留多模态输出摘要");
    assert!(content.content.is_some(), "不能退化为默认的纯文本调用");
    Ok(())
}

#[derive(Default)]
struct MockResourceTracker {
    dropped: Arc<AtomicUsize>,
    payloads: Mutex<Vec<Weak<Vec<u8>>>>,
}

struct MockResourceMiddleware {
    tracker: Arc<MockResourceTracker>,
    fail_before_agent: bool,
}

#[async_trait::async_trait]
impl Middleware<AgentState> for MockResourceMiddleware {
    fn name(&self) -> &str {
        "OwnedToolMiddleware"
    }
    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        let payload = Arc::new(vec![42; 1024 * 1024]);
        self.tracker.payloads.lock().push(Arc::downgrade(&payload));
        vec![Box::new(MockOwnedTool {
            payload,
            dropped: Arc::clone(&self.tracker.dropped),
        })]
    }
    async fn before_agent(&self, _state: &mut AgentState) -> AgentResult<()> {
        if self.fail_before_agent {
            return Err(AgentError::LlmError("测试中间件提前失败".into()));
        }
        Ok(())
    }
}

fn make_agent(
    tracker: &Arc<MockResourceTracker>,
    fail_before_agent: bool,
) -> ReActAgent<MockLLM, AgentState> {
    ReActAgent::new(MockLLM::always_answer("ok")).add_middleware(Box::new(MockResourceMiddleware {
        tracker: Arc::clone(tracker),
        fail_before_agent,
    }))
}

#[tokio::test]
async fn test_execute_releases_middleware_tools_after_each_prompt() -> AgentResult<()> {
    let tracker = Arc::new(MockResourceTracker::default());
    let agent = make_agent(&tracker, false);
    let mut state = AgentState::new(".");
    for round in 1..=12 {
        agent
            .execute(AgentInput::text("继续"), &mut state, None)
            .await?;
        assert_eq!(
            tracker.dropped.load(Ordering::SeqCst),
            round,
            "每轮结束必须释放本轮工具"
        );
        assert!(
            tracker
                .payloads
                .lock()
                .iter()
                .all(|payload| payload.upgrade().is_none()),
            "工具未执行也必须释放其持有的资源"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_execute_releases_middleware_tools_on_before_agent_error() {
    let tracker = Arc::new(MockResourceTracker::default());
    let agent = make_agent(&tracker, true);
    let mut state = AgentState::new(".");
    let result = agent
        .execute(AgentInput::text("继续"), &mut state, None)
        .await;
    assert!(
        matches!(result, Err(AgentError::LlmError(_))),
        "必须保留原有错误"
    );
    assert_eq!(
        tracker.dropped.load(Ordering::SeqCst),
        1,
        "提前失败时也必须析构工具"
    );
    assert!(
        tracker.payloads.lock()[0].upgrade().is_none(),
        "提前失败不能保留工具资源"
    );
}

#[tokio::test]
async fn test_execute_releases_middleware_tools_on_cancel() {
    let tracker = Arc::new(MockResourceTracker::default());
    let agent = make_agent(&tracker, false);
    let mut state = AgentState::new(".");
    let cancel = AgentCancellationToken::new();
    cancel.cancel();
    let result = agent
        .execute(AgentInput::text("继续"), &mut state, Some(cancel))
        .await;
    assert!(
        matches!(result, Err(AgentError::Interrupted)),
        "必须保留取消结果"
    );
    assert_eq!(
        tracker.dropped.load(Ordering::SeqCst),
        1,
        "取消时也必须析构工具"
    );
    assert!(
        tracker.payloads.lock()[0].upgrade().is_none(),
        "取消不能保留工具资源"
    );
}
