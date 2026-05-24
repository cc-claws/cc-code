use super::*;
use langfuse_client::{BackpressurePolicy, Batcher, BatcherConfig, LangfuseClient};
use std::sync::Arc;
use std::time::Duration;

fn make_tracer() -> LangfuseTracer {
    let client = LangfuseClient::new("pk-test", "sk-test", "http://127.0.0.1:1", 0);
    let config = BatcherConfig {
        max_events: 1000,
        flush_interval: Duration::from_secs(600),
        backpressure: BackpressurePolicy::DropNew,
        max_retries: 0,
    };
    let batcher = Arc::new(Batcher::new(client, config));
    let session = Arc::new(LangfuseSession {
        client: Arc::new(LangfuseClient::new("pk", "sk", "http://127.0.0.1:1", 0)),
        batcher,
    });
    LangfuseTracer::new(session, "test-session".to_string())
}

fn agent_tool_input(subagent_type: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "subagent_type": subagent_type,
        "prompt": prompt,
        "description": "test task"
    })
}

// === Test 1: Agent 工具 on_tool_start 压栈 ===
#[tokio::test]
async fn test_agent_tool_start_pushes_subagent_stack() {
    let mut tracer = make_tracer();
    let main_agent_id = tracer.agent_observation_id.clone();

    assert!(tracer.subagent_stack.is_empty());
    assert_eq!(tracer.current_agent_id(), main_agent_id);

    let input = agent_tool_input("code-reviewer", "review this code");
    tracer.on_tool_start("tc-1", "Agent", &input);

    assert_eq!(tracer.subagent_stack.len(), 1);
    let subagent_obs_id = tracer.subagent_stack[0].observation_id.clone();
    assert_ne!(subagent_obs_id, main_agent_id);
    assert_eq!(tracer.current_agent_id(), subagent_obs_id);
    assert_eq!(tracer.subagent_stack[0].agent_id, "code-reviewer");
}

// === Test 2: 非 Agent 工具不影响栈 ===
#[tokio::test]
async fn test_non_agent_tool_does_not_push_subagent_stack() {
    let mut tracer = make_tracer();
    let input = serde_json::json!({"file_path": "/tmp/test.rs"});
    tracer.on_tool_start("tc-1", "Read", &input);
    assert!(tracer.subagent_stack.is_empty());
}

// === Test 3: Agent 工具 on_tool_end 弹出栈 ===
#[tokio::test]
async fn test_agent_tool_end_pops_subagent_stack() {
    let mut tracer = make_tracer();
    tracer.on_tool_start("tc-1", "Agent", &agent_tool_input("explorer", "find files"));
    assert_eq!(tracer.subagent_stack.len(), 1);
    tracer.on_tool_end("tc-1", "found 3 files", false);
    assert!(tracer.subagent_stack.is_empty());
}

// === Test 4: 非 Agent 工具 on_tool_end 不弹栈 ===
#[tokio::test]
async fn test_non_agent_tool_end_does_not_pop_subagent_stack() {
    let mut tracer = make_tracer();
    tracer.on_tool_start("tc-agent", "Agent", &agent_tool_input("plan", "plan this"));
    assert_eq!(tracer.subagent_stack.len(), 1);

    let input = serde_json::json!({"pattern": "*.rs"});
    tracer.on_tool_start("tc-glob", "Glob", &input);
    tracer.on_tool_end("tc-glob", "file1.rs", false);

    assert_eq!(tracer.subagent_stack.len(), 1);

    tracer.on_tool_end("tc-agent", "plan done", false);
    assert!(tracer.subagent_stack.is_empty());
}

// === Test 5: SubAgent 生命周期中内部事件路由正确 ===
#[tokio::test]
async fn test_subagent_internal_events_use_subagent_context() {
    let mut tracer = make_tracer();
    let main_agent_id = tracer.agent_observation_id.clone();

    tracer.on_tool_start(
        "tc-1",
        "Agent",
        &agent_tool_input("code-reviewer", "review"),
    );
    let subagent_obs_id = tracer.current_agent_id();
    assert_ne!(subagent_obs_id, main_agent_id);

    // SubAgent 内部 LLM 调用：parent 应为 subagent obs
    tracer.on_llm_start(0, &[], &[]);
    assert_eq!(tracer.current_agent_id(), subagent_obs_id);

    // SubAgent 内部工具调用：使用 subagent 的 tools context
    tracer.on_tool_start(
        "tc-inner",
        "Read",
        &serde_json::json!({"file_path": "x.rs"}),
    );
    assert_eq!(tracer.subagent_stack[0].pending_tools.len(), 1);

    tracer.on_tool_end("tc-inner", "content", false);
    assert!(tracer.subagent_stack[0].tools_batch_end_time.is_some());

    tracer.on_tool_end("tc-1", "review done", false);
    assert!(tracer.subagent_stack.is_empty());
    assert_eq!(tracer.current_agent_id(), main_agent_id);
}

// === Test 6: 嵌套 SubAgent ===
#[tokio::test]
async fn test_nested_subagent_stack_depth() {
    let mut tracer = make_tracer();

    tracer.on_tool_start("tc-a", "Agent", &agent_tool_input("planner", "plan"));
    assert_eq!(tracer.subagent_stack.len(), 1);
    let planner_obs_id = tracer.current_agent_id();

    tracer.on_tool_start("tc-b", "Agent", &agent_tool_input("explorer", "find"));
    assert_eq!(tracer.subagent_stack.len(), 2);
    let explorer_obs_id = tracer.current_agent_id();
    assert_ne!(explorer_obs_id, planner_obs_id);

    tracer.on_llm_start(0, &[], &[]);
    assert_eq!(tracer.current_agent_id(), explorer_obs_id);

    tracer.on_tool_end("tc-b", "found files", false);
    assert_eq!(tracer.subagent_stack.len(), 1);
    assert_eq!(tracer.current_agent_id(), planner_obs_id);

    tracer.on_tool_end("tc-a", "plan done", false);
    assert!(tracer.subagent_stack.is_empty());
}

// === Test 7: 未知 tool_call_id 的 on_tool_end 不 panic ===
#[tokio::test]
async fn test_on_tool_end_unknown_tool_call_id_no_panic() {
    let mut tracer = make_tracer();
    tracer.on_tool_end("nonexistent", "output", false);
    // Should return early without panicking
}

// === Test 8: Fork 类型识别 ===
#[test]
fn test_fork_subagent_identity() {
    assert_eq!(
        LangfuseTracer::subagent_identity(
            &serde_json::json!({"prompt": "do something", "fork": true})
        ),
        "fork"
    );
    assert_eq!(
        LangfuseTracer::subagent_identity(
            &serde_json::json!({"subagent_type": "code-reviewer", "fork": true, "prompt": "x"})
        ),
        "code-reviewer"
    );
    assert_eq!(
        LangfuseTracer::subagent_identity(&serde_json::json!({"prompt": "x"})),
        "fork"
    );
}
