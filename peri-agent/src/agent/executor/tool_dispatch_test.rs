use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use super::*;
use crate::{
    agent::{
        events::{AgentEvent, FnEventHandler},
        react::{AgentInput, Reasoning},
        state::AgentState,
    },
    middleware::r#trait::Middleware,
    tools::BaseTool,
};

/// 通用不变量：state 中每个 tool_use 必须有对应 tool_result。
fn assert_no_orphaned_tool_uses(state: &AgentState) {
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }
    assert_eq!(
        ai_tool_ids.len(),
        tool_result_ids.len(),
        "tool_use 数量 ({}) != tool_result 数量 ({})\n\
         tool_use IDs: {:?}\n\
         tool_result IDs: {:?}",
        ai_tool_ids.len(),
        tool_result_ids.len(),
        ai_tool_ids,
        tool_result_ids
    );
    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "tool_use id={} 缺少配对 tool_result（孤儿 tool_use → Anthropic API 400）",
            id
        );
    }
}

/// 并发工具执行中部分失败：3 个工具并发，tool_b 执行失败。
/// 验证所有 tool_use 都有配对 tool_result，Agent 继续（不停止）。
#[tokio::test]
async fn test_concurrent_partial_failure_all_results_written() {
    struct FailToolB;
    #[async_trait::async_trait]
    impl BaseTool for FailToolB {
        fn name(&self) -> &str {
            "tool_b"
        }
        fn description(&self) -> &str {
            "fails"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("tool_b 执行失败".into())
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results processed"))
            }
        }
    }

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(FailToolB))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    assert_no_orphaned_tool_uses(&state);
}

/// 验证 before_tool 非拒绝错误（P4 路径）在 i>0 时，
/// 已通过 before_tool 的 modified_calls 也获得 error tool_result，
/// 不产生孤儿 tool_use（Anthropic API 400）。
///
/// 场景：3 个工具调用，call[0] 通过 before_tool（推入 modified_calls），
/// call[1] 的 before_tool 返回非 ToolRejected 错误 → P3 路径触发。
/// 修复前：call[0] 成为孤儿 tool_use；修复后：call[0] 也获得 error tool_result。
#[tokio::test]
async fn test_p3_error_flushes_modified_calls_no_orphaned_tool_use() {
    // 中间件：第一个工具通过，后续全部返回非 ToolRejected 错误
    struct PartialFailMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for PartialFailMiddleware {
        fn name(&self) -> &str {
            "PartialFailMiddleware"
        }
        async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
            if tool_call.id == "id1" {
                // 第一个工具通过
                Ok(tool_call.clone())
            } else {
                // 后续工具返回非 ToolRejected 错误
                Err(AgentError::ToolExecutionFailed {
                    tool: tool_call.name.clone(),
                    reason: "模拟 before_tool 错误".to_string(),
                })
            }
        }
    }

    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results received"))
            }
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }))
        .add_middleware(Box::new(PartialFailMiddleware))
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    // P3 路径返回错误，execute 应传播该错误
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    // P3 路径返回错误，execute 应传播该错误
    assert!(result.is_err(), "P3 路径应返回错误，实际: {:?}", result);

    // 延迟写入：before_tool 错误路径不写入 AI 消息到 state
    assert_no_orphaned_tool_uses(&state);
    // AI 消息未写入，state 中无 tool_use 也无 tool_result
    let ai_count = state
        .messages()
        .iter()
        .filter(|m| matches!(m, BaseMessage::Ai { .. }))
        .count();
    assert_eq!(ai_count, 0, "before_tool 错误路径不应写入 AI 消息到 state");
}

/// 验证取消信号在 i>0 时，modified_calls 也获得 error tool_result。
#[tokio::test]
async fn test_cancel_at_i_gt_0_flushes_modified_calls() {
    struct SlowTool;
    #[async_trait::async_trait]
    impl BaseTool for SlowTool {
        fn name(&self) -> &str {
            "slow_tool"
        }
        fn description(&self) -> &str {
            "hangs in before_tool then in execution"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok("never".to_string())
        }
    }

    // 中间件：第一个工具通过 before_tool 但后续挂起
    struct HangingBeforeToolMiddleware {
        call_count: Arc<Mutex<usize>>,
    }
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for HangingBeforeToolMiddleware {
        fn name(&self) -> &str {
            "HangingBeforeToolMiddleware"
        }
        async fn before_tool(
            &self,
            _state: &mut S,
            _tool_call: &ToolCall,
        ) -> AgentResult<ToolCall> {
            let should_hang = {
                let mut count = self.call_count.lock().unwrap();
                *count += 1;
                *count > 1
            };
            // guard 已在块内释放
            if should_hang {
                // 后续工具挂起（等待取消），200ms 足够让 cancel (100ms) 触发
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Ok(_tool_call.clone())
        }
    }

    struct TwoToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for TwoToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call two tools",
                    vec![
                        ToolCall::new("id1", "slow_tool", serde_json::json!({})),
                        ToolCall::new("id2", "slow_tool", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "ok"))
            }
        }
    }

    let cancel = CancellationToken::new();
    let call_count = Arc::new(Mutex::new(0usize));
    let agent = ReActAgent::new(TwoToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(SlowTool))
        .add_middleware(Box::new(HangingBeforeToolMiddleware {
            call_count: Arc::clone(&call_count),
        }));

    // 在 before_tool 处理第二个工具时触发取消
    let token = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, Some(cancel))
        .await;

    assert!(
        matches!(result, Err(AgentError::Interrupted)),
        "取消后应返回 Interrupted，实际: {:?}",
        result
    );

    // 核心断言：所有 tool_use 必须有配对 tool_result
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }

    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "取消后 tool_use id={} 缺少配对的 tool_result",
            id
        );
    }
    assert_eq!(
        ai_tool_ids.len(),
        tool_result_ids.len(),
        "取消后所有 tool_use 必须有配对 tool_result"
    );
}

/// 验证混合路径：Ok + ToolRejected + 非 ToolRejected 错误
/// call[0] Ok → 推入 modified_calls
/// call[1] ToolRejected → 独立写入 error tool_result，continue
/// call[2] 非 ToolRejected 错误 → P4 路径，flush modified_calls + flush pending
/// 所有 3 个 tool_use 都应有 tool_result，且无重复写入。
#[tokio::test]
async fn test_mixed_ok_rejected_error_all_tool_results_written() {
    struct MixedResultMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for MixedResultMiddleware {
        fn name(&self) -> &str {
            "MixedResultMiddleware"
        }
        async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
            match tool_call.id.as_str() {
                "id1" => Ok(tool_call.clone()),
                "id2" => Err(AgentError::ToolRejected {
                    tool: tool_call.name.clone(),
                    reason: "用户拒绝".to_string(),
                }),
                _ => Err(AgentError::ToolExecutionFailed {
                    tool: tool_call.name.clone(),
                    reason: "before_tool 错误".to_string(),
                }),
            }
        }
    }

    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results received"))
            }
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }))
        .add_middleware(Box::new(MixedResultMiddleware));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_err(), "混合路径应返回错误，实际: {:?}", result);

    // 延迟写入：before_tool P4 错误路径不写入 AI 消息到 state
    assert_no_orphaned_tool_uses(&state);
    let ai_count = state
        .messages()
        .iter()
        .filter(|m| matches!(m, BaseMessage::Ai { .. }))
        .count();
    assert_eq!(ai_count, 0, "before_tool P4 错误路径不应写入 AI 消息");
}

/// LLM 输出小写 "bash"，工具注册为 PascalCase "Bash"，
/// resolve_tool 通过大小写无关匹配找到工具，不产生 ToolNotFound。
#[tokio::test]
async fn test_tool_name_case_insensitive_fallback() {
    struct EchoTool;
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            "Bash"
        }
        fn description(&self) -> &str {
            "echo shell"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("shell done".to_string())
        }
    }

    struct LowerCaseBashLLM;
    #[async_trait::async_trait]
    impl ReactLLM for LowerCaseBashLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                // LLM 输出小写 "bash"
                Ok(Reasoning::with_tools(
                    "call bash",
                    vec![ToolCall::new("id1", "bash", serde_json::json!({}))],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "shell executed"))
            }
        }
    }

    let agent = ReActAgent::new(LowerCaseBashLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_ok(), "大小写无关匹配应成功，实际: {:?}", result);
    // 验证 tool_result 不是错误
    let has_error_result = state
        .messages()
        .iter()
        .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
    assert!(!has_error_result, "不应有 ToolNotFound 错误结果");
}

/// LLM 输出 "Task"（别名），工具注册为 "Agent"，
/// resolve_tool 通过语义别名表匹配，不产生 ToolNotFound。
#[tokio::test]
async fn test_tool_name_alias_fallback() {
    struct EchoAgent;
    #[async_trait::async_trait]
    impl BaseTool for EchoAgent {
        fn name(&self) -> &str {
            "Agent"
        }
        fn description(&self) -> &str {
            "echo agent"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("agent done".to_string())
        }
    }

    struct AliasTaskLLM;
    #[async_trait::async_trait]
    impl ReactLLM for AliasTaskLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                // LLM 输出别名 "Task"
                Ok(Reasoning::with_tools(
                    "call task",
                    vec![ToolCall::new("id1", "Task", serde_json::json!({}))],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "agent executed"))
            }
        }
    }

    let agent = ReActAgent::new(AliasTaskLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoAgent));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_ok(), "别名匹配应成功，实际: {:?}", result);
    let has_error_result = state
        .messages()
        .iter()
        .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
    assert!(!has_error_result, "不应有 ToolNotFound 错误结果");
}

/// 连续 5 次同工具+同错误后注入系统纠正消息
#[tokio::test]
async fn test_consecutive_failure_injects_correction() {
    struct AlwaysFailRead;
    #[async_trait::async_trait]
    impl BaseTool for AlwaysFailRead {
        fn name(&self) -> &str {
            "Read"
        }
        fn description(&self) -> &str {
            "read"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({ "properties": { "file_path": { "type": "string" } } })
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("The 'file_path' parameter is required for the Read tool.".into())
        }
    }

    struct StubbornLLM;
    #[async_trait::async_trait]
    impl ReactLLM for StubbornLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_correction = messages.iter().any(|m| {
                matches!(m, BaseMessage::System { content, .. }
                    if content.text_content().contains("5 consecutive times"))
            });
            if has_correction {
                return Ok(Reasoning::with_answer("done", "I'll stop retrying"));
            }
            Ok(Reasoning::with_tools(
                "retrying",
                vec![ToolCall::new(
                    format!("id_{}", messages.len()),
                    "Read",
                    serde_json::json!({}),
                )],
            ))
        }
    }

    let agent = ReActAgent::new(StubbornLLM)
        .max_iterations(20)
        .register_tool(Box::new(AlwaysFailRead));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    let has_correction = state.messages().iter().any(|m| {
        matches!(m, BaseMessage::System { content, .. }
            if content.text_content().contains("5 consecutive times"))
    });
    assert!(has_correction, "应注入连续失败纠正消息");
}

#[test]
fn test_normalize_params_path_to_file_path() {
    // Arrange: input has "path" but no "file_path"
    let input = serde_json::json!({"path": "/tmp/test.rs", "offset": 10});

    // Act: 对 Read 工具执行归一化
    let normalized = super::normalize_params("Read", input);

    // Assert: "path" → "file_path", "offset" unchanged
    assert_eq!(normalized["file_path"], "/tmp/test.rs");
    assert!(normalized.get("path").is_none());
    assert_eq!(normalized["offset"], 10);
}

#[test]
fn test_normalize_params_file_path_already_exists() {
    // Arrange: input has both "path" and "file_path" (LLM wrote both)
    let input = serde_json::json!({"path": "/tmp/wrong.rs", "file_path": "/tmp/right.rs"});

    // Act
    let normalized = super::normalize_params("Read", input);

    // Assert: "file_path" unchanged, "path" 不清除（保守策略，不丢数据）
    assert_eq!(normalized["file_path"], "/tmp/right.rs");
    // "path" 保留原样（因为已经有了 file_path，不覆盖）
    assert_eq!(normalized["path"], "/tmp/wrong.rs");
}

#[test]
fn test_normalize_params_no_alias_present() {
    // Arrange: normal Read call with correct parameter names
    let input = serde_json::json!({"file_path": "/tmp/test.rs"});

    // Act
    let normalized = super::normalize_params("Read", input);

    // Assert: no change
    assert_eq!(normalized["file_path"], "/tmp/test.rs");
}

#[test]
fn test_normalize_params_non_object_input() {
    // Arrange: input is a string (edge: unlikely but safe)
    let input = serde_json::Value::String("hello".to_string());

    // Act
    let normalized = super::normalize_params("Read", input);

    // Assert: returned as-is
    assert_eq!(normalized, serde_json::Value::String("hello".to_string()));
}

#[test]
fn test_normalize_params_grep_path_not_renamed() {
    // Arrange: Grep 传入 path 参数
    let input = serde_json::json!({"pattern": "needle", "path": "spec/"});

    // Act: 对 Grep 工具执行归一化
    let normalized = super::normalize_params("Grep", input);

    // Assert: path 不应被改名为 file_path
    assert_eq!(normalized["path"], "spec/");
    assert!(normalized.get("file_path").is_none());
    assert_eq!(normalized["pattern"], "needle");
}

#[test]
fn test_normalize_params_glob_path_not_renamed() {
    // Arrange: Glob 传入 path 参数
    let input = serde_json::json!({"pattern": "**/*.rs", "path": "src/"});

    // Act: 对 Glob 工具执行归一化
    let normalized = super::normalize_params("Glob", input);

    // Assert: path 不应被改名为 file_path
    assert_eq!(normalized["path"], "src/");
    assert!(normalized.get("file_path").is_none());
    assert_eq!(normalized["pattern"], "**/*.rs");
}

#[test]
fn test_normalize_params_bash_not_affected() {
    // Arrange: Bash 传入 command 参数（无 path/file_path 混淆场景）
    let input = serde_json::json!({"command": "ls -la"});

    // Act
    let normalized = super::normalize_params("Bash", input);

    // Assert: 无变化
    assert_eq!(normalized["command"], "ls -la");
}

#[test]
fn test_validate_against_schema_missing_required() {
    // Arrange: Glob 缺少必填字段 pattern
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string" },
            "path": { "type": "string" }
        },
        "required": ["pattern"]
    });
    let input = serde_json::json!({ "path": "src/" });
    // Act
    let result = super::validate_against_schema(&input, &schema);
    // Assert
    assert!(result.is_err(), "缺少必填字段应报错");
    let err = result.unwrap_err();
    assert!(
        err.contains("missing required field 'pattern' (expected string)"),
        "错误信息应包含缺失字段与期望类型，实际: {err}"
    );
}

#[test]
fn test_validate_against_schema_invalid_type() {
    // Arrange: pattern 期望 string，传入了 integer
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string" }
        },
        "required": ["pattern"]
    });
    let input = serde_json::json!({ "pattern": 12345 });
    // Act
    let result = super::validate_against_schema(&input, &schema);
    // Assert
    assert!(result.is_err(), "类型不匹配应报错");
    let err = result.unwrap_err();
    assert!(
        err.contains("field 'pattern' has invalid type: expected string, got integer"),
        "错误信息应包含字段名、期望类型与实际类型，实际: {err}"
    );
}

#[test]
fn test_validate_against_schema_success() {
    // Arrange: 传入合法参数
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string" },
            "path": { "type": "string" }
        },
        "required": ["pattern"]
    });
    let input = serde_json::json!({ "pattern": "**/*.rs", "path": "src" });
    // Act
    let result = super::validate_against_schema(&input, &schema);
    // Assert
    assert!(result.is_ok(), "合法参数应通过校验");
}

#[test]
fn test_validate_against_schema_non_object() {
    // Arrange: schema 期望 object，输入为 string
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string" }
        },
        "required": ["command"]
    });
    let input = serde_json::json!("git status");
    // Act
    let result = super::validate_against_schema(&input, &schema);
    // Assert
    assert!(result.is_err(), "非 object 输入应报错");
    let err = result.unwrap_err();
    assert!(
        err.contains("expected an object for arguments, got string"),
        "应提示期望 object，实际: {err}"
    );
}

#[test]
fn test_validate_against_schema_enum_mismatch() {
    // Arrange: effort 字段有 enum 限制
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "effort": {
                "type": "string",
                "enum": ["low", "medium", "high"]
            }
        }
    });
    let input = serde_json::json!({ "effort": "extreme" });
    // Act
    let result = super::validate_against_schema(&input, &schema);
    // Assert
    assert!(result.is_err(), "枚举不匹配应报错");
    let err = result.unwrap_err();
    assert!(
        err.contains("not one of"),
        "错误信息应指出不在枚举中，实际: {err}"
    );
}

#[test]
fn test_step_action_signature_canonical() {
    // Arrange: 两个输入键序不同的工具调用
    let tc1 = ToolCall::new(
        "id1",
        "Grep",
        serde_json::json!({ "path": "src/", "pattern": "fn main" }),
    );
    let tc2 = ToolCall::new(
        "id1",
        "Grep",
        serde_json::json!({ "pattern": "fn main", "path": "src/" }),
    );
    // Act
    let sig1 = super::compute_step_action_signature(&[tc1]);
    let sig2 = super::compute_step_action_signature(&[tc2]);
    // Assert
    assert_eq!(sig1, sig2, "键序不同的相同参数应产生一致的动作签名");
}

#[test]
fn test_action_loop_detector() {
    // Arrange
    let mut detector = super::ActionLoopDetector::new();
    let sig_a = "Bash:{\"command\":\"ls\"}";
    let sig_b = "Bash:{\"command\":\"pwd\"}";
    // Act & Assert: 第 1 次
    let (count, is_loop) = detector.record(sig_a);
    assert_eq!(count, 1);
    assert!(!is_loop, "第 1 次不应判定为循环");
    // 第 2 次
    let (count, is_loop) = detector.record(sig_a);
    assert_eq!(count, 2);
    assert!(!is_loop, "第 2 次不应判定为循环");
    // 第 3 次达到阈值
    let (count, is_loop) = detector.record(sig_a);
    assert_eq!(count, 3);
    assert!(is_loop, "第 3 次连续相同动作应判定为循环");
    // 切换动作后重置
    let (count, is_loop) = detector.record(sig_b);
    assert_eq!(count, 1);
    assert!(!is_loop, "切换动作后计数应重置为 1");
}

/// 连续 3 次相同成功动作签名后注入系统纠正消息
#[tokio::test]
async fn test_consecutive_action_injects_correction() {
    struct AlwaysSucceedTool;
    #[async_trait::async_trait]
    impl BaseTool for AlwaysSucceedTool {
        fn name(&self) -> &str {
            "EchoTool"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": { "msg": { "type": "string" } },
                "required": ["msg"]
            })
        }
        async fn invoke(
            &self,
            input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(input["msg"].as_str().unwrap_or("ok").to_string())
        }
    }

    struct LoopActionLLM;
    #[async_trait::async_trait]
    impl ReactLLM for LoopActionLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_loop_warning = messages.iter().any(|m| {
                matches!(m, BaseMessage::System { content, .. }
                    if content.text_content().contains("3 consecutive times with identical parameters"))
            });
            if has_loop_warning {
                return Ok(Reasoning::with_answer("done", "I noticed the loop and will stop."));
            }
            Ok(Reasoning::with_tools(
                "calling echo again",
                vec![ToolCall::new(
                    format!("id_{}", messages.len()),
                    "EchoTool",
                    serde_json::json!({ "msg": "same_action" }),
                )],
            ))
        }
    }

    let agent = ReActAgent::new(LoopActionLLM)
        .max_iterations(10)
        .register_tool(Box::new(AlwaysSucceedTool));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("run"), &mut state, None)
        .await;

    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);
    let has_loop_warning = state.messages().iter().any(|m| {
        matches!(m, BaseMessage::System { content, .. }
            if content.text_content().contains("3 consecutive times with identical parameters"))
    });
    assert!(has_loop_warning, "应注入连续相同动作纠正消息");
}

/// 验证工具入参违反 schema 时，返回字段级错误提示且包含 Received keys
#[tokio::test]
async fn test_tool_execution_schema_validation_error_message() {
    struct StrictTool;
    #[async_trait::async_trait]
    impl BaseTool for StrictTool {
        fn name(&self) -> &str {
            "StrictTool"
        }
        fn description(&self) -> &str {
            "strict"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" }
                },
                "required": ["pattern"]
            })
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("ok".to_string())
        }
    }

    struct InvalidParamLLM;
    #[async_trait::async_trait]
    impl ReactLLM for InvalidParamLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages.iter().any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                // 错误地传入了 command 字段，缺失 pattern
                Ok(Reasoning::with_tools(
                    "calling with wrong param",
                    vec![ToolCall::new(
                        "id_err",
                        "StrictTool",
                        serde_json::json!({ "command": "ls" }),
                    )],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "got error"))
            }
        }
    }

    let agent = ReActAgent::new(InvalidParamLLM)
        .max_iterations(5)
        .register_tool(Box::new(StrictTool));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("run"), &mut state, None)
        .await;

    assert!(result.is_ok(), "Agent 应正常处理错误结果并完成");
    let error_msg = state
        .messages()
        .iter()
        .find_map(|m| match m {
            BaseMessage::Tool { content, is_error: true, .. } => Some(content.text_content()),
            _ => None,
        })
        .expect("应包含 Tool 错误消息");

    assert!(
        error_msg.contains("Invalid arguments for tool StrictTool: missing required field 'pattern' (expected string)"),
        "错误信息应包含缺失字段与期望类型，实际: {error_msg}"
    );
    assert!(
        error_msg.contains("Received keys: [\"command\"]"),
        "错误信息应包含实际收到的 keys，实际: {error_msg}"
    );
}
