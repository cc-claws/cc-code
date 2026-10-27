use super::*;
use cc_agent::{
    agent::{compact::CompactConfig, state::State, token::ContextBudget},
    error::{AgentError, AgentResult},
    llm::{BaseModel, LlmRequest, LlmResponse, StopReason},
    messages::BaseMessage,
};

const PROJECT_SENTINEL: &str = "FROZEN_PROJECT_RULE_SENTINEL_732";
const SUMMARY_SENTINEL: &str = "COMPACT_SUMMARY_SENTINEL_732";

struct MockCompactModel {
    requests: Arc<Mutex<Vec<LlmRequest>>>,
}

#[async_trait]
impl BaseModel for MockCompactModel {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        self.requests.lock().push(request);
        Ok(LlmResponse {
            message: BaseMessage::ai(format!("<summary>{SUMMARY_SENTINEL}</summary>")),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "memory-compact-test"
    }
}

fn make_compact_tool_response(protocol: MockProvider) -> String {
    let arguments =
        json!({"todos": [{"content": "Verify memory after compaction", "status": "in_progress"}]});
    let body = match protocol {
        MockProvider::OpenAi => format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({
                "id": "memory-compact-response",
                "choices": [{
                    "index": 0,
                    "delta": {"role": "assistant", "tool_calls": [{
                        "index": 0, "id": "memory-compact-call", "type": "function",
                        "function": {"name": "TodoWrite", "arguments": arguments.to_string()}
                    }]},
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 900, "completion_tokens": 1, "total_tokens": 901}
            })
        ),
        MockProvider::Anthropic => [
            ("message_start", json!({"message": {"id": "memory-compact-response", "usage": {"input_tokens": 900}}})),
            ("content_block_start", json!({"index": 0, "content_block": {"type": "tool_use", "id": "memory-compact-call", "name": "TodoWrite", "input": {}}})),
            ("content_block_delta", json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": arguments.to_string()}})),
            ("content_block_stop", json!({"index": 0})),
            ("message_delta", json!({"delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 1}})),
            ("message_stop", json!({})),
        ]
        .into_iter()
        .map(|(event, data)| format!("event: {event}\ndata: {data}\n\n"))
        .collect(),
    };
    make_http_stream_response(&body)
}

fn make_non_retryable_model_error_response() -> String {
    let body = json!({"error": {"type": "invalid_request_error", "message": "memory compact test failure"}}).to_string();
    format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn make_compacted_memory_requests(protocol: MockProvider, fail_after_compact: bool) {
    let directory = tempfile::tempdir().expect("应能创建独立测试目录");
    let mut responses = vec![make_compact_tool_response(protocol)];
    if fail_after_compact {
        responses.push(make_non_retryable_model_error_response());
    }
    let endpoint = make_scripted_model_endpoint(protocol, responses).await;
    let system_prompt = format!(
        "Test agent\n\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\n\n{}",
        cc_middlewares::memory::build_memory_read_prompt("/test/memory", Some(MEMORY_SENTINEL))
    );
    let mut config = make_agent_config(
        directory.path(),
        make_provider(protocol, &endpoint.base_url),
        system_prompt,
        None,
    );
    let summaries = Arc::new(Mutex::new(Vec::new()));
    let snapshots = Arc::new(Mutex::new(Vec::new()));
    let captured_snapshots = Arc::clone(&snapshots);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    config.frozen_instructions = Some(PROJECT_SENTINEL.to_string());
    config.compact_config = Some(CompactConfig::default());
    config.compact_budget = Some(ContextBudget::new(1000));
    config.compact_model = Some(Arc::new(MockCompactModel {
        requests: Arc::clone(&summaries),
    }));
    config.compact_event_tx = Some(Arc::new(std::sync::Mutex::new(Some(event_tx))));
    config.event_handler = Arc::new(FnEventHandler(move |event| {
        if let AgentEvent::StateSnapshot(messages) = event {
            captured_snapshots.lock().push(messages);
        }
    }));
    let cancel = config.cancel.clone();
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    let (mut agent, _) = build_agent_with_embedded_memory(config, None, &pool);
    assert!(agent
        .executor
        .middleware_names()
        .contains(&"CompactMiddleware"));
    let mut state = AgentState::new(directory.path().to_string_lossy());
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        agent.executor.execute(
            AgentInput::text("继续当前任务"),
            &mut state,
            Some(cancel.clone()),
        ),
    )
    .await
    .expect("同一轮中的工具、自动压缩和后续请求应及时完成");
    if fail_after_compact {
        assert!(
            matches!(result, Err(AgentError::LlmHttpError { status: 400, .. })),
            "应覆盖压缩成功后模型请求报错：{result:?}"
        );
    } else {
        let output = result.expect("压缩后 Agent 应继续回答");
        assert_eq!(output.text, "ok");
        assert_eq!(output.tool_calls.len(), 1, "应通过真实工具调度触发轮内压缩");
        assert!(!output.tool_calls[0].1.is_error, "TodoWrite 应执行成功");
    }
    assert_eq!(
        agent.todo_rx.try_recv().expect("应收到真实工具更新").len(),
        1
    );
    assert_eq!(
        summaries.lock().len(),
        1,
        "只调用一次必要的对话摘要，不生成额外记忆"
    );
    assert!(
        state.messages().iter().all(|message| !message.is_system()),
        "成功或失败返回时均应清理临时系统前缀"
    );
    assert!(state.messages()[0].content().contains(SUMMARY_SENTINEL));
    let events: Vec<_> = std::iter::from_fn(|| event_rx.try_recv().ok()).collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentEvent::CompactCompleted { .. }))
            .count(),
        1
    );
    for event in events {
        if let AgentEvent::CompactCompleted { messages, .. } = event {
            assert!(matches!(messages[0], BaseMessage::Human { .. }));
            assert!(
                messages.iter().all(|message| !message.is_system()),
                "压缩事件不得携带临时系统前缀"
            );
        }
    }
    assert!(!snapshots.lock().is_empty(), "真实 Agent 应产生状态快照");
    for messages in snapshots.lock().iter() {
        assert!(
            messages.iter().all(|message| !message.is_system()),
            "状态快照不得泄漏系统规则或记忆索引"
        );
    }
    let next = tokio::time::timeout(
        Duration::from_secs(15),
        agent
            .executor
            .execute(AgentInput::text("下一轮继续"), &mut state, Some(cancel)),
    )
    .await
    .expect("下一轮应及时完成")
    .expect("上一轮成功或失败都不应破坏后续调用");
    assert_eq!(next.text, "ok");
    assert!(
        state.messages().iter().all(|message| !message.is_system()),
        "下一轮完成后同样不得残留临时系统前缀"
    );
    assert_eq!(summaries.lock().len(), 1, "下一轮不应额外触发摘要");
    let requests = endpoint.requests.lock();
    assert_eq!(
        requests.len(),
        3,
        "首轮两次模型请求和下一轮一次请求，不增加记忆生成调用"
    );
    assert!(
        requests[1].body["messages"]
            .to_string()
            .contains(SUMMARY_SENTINEL),
        "第二次模型调用必须已进入压缩后的上下文"
    );
    let systems: Vec<_> = requests
        .iter()
        .map(|request| {
            assert_eq!(request.body["stream"], true);
            for sentinel in [MEMORY_SENTINEL, PROJECT_SENTINEL] {
                assert_eq!(
                    request.body.to_string().matches(sentinel).count(),
                    1,
                    "压缩前、压缩后及下一轮都应恰好携带一份记忆和规则"
                );
            }
            match protocol {
                MockProvider::OpenAi => Value::Array(
                    request.body["messages"]
                        .as_array()
                        .expect("OpenAI 消息应为数组")
                        .iter()
                        .filter(|message| message["role"] == "system")
                        .cloned()
                        .collect(),
                ),
                MockProvider::Anthropic => request.body["system"].clone(),
            }
        })
        .collect();
    assert_eq!(
        systems[0], systems[1],
        "轮内压缩不得改变系统提示词、索引或缓存块"
    );
    assert_eq!(systems[0], systems[2], "下一轮不得累积系统前缀或改变缓存块");
}

#[tokio::test]
async fn test_memory_survives_automatic_compact_in_openai_agent_loop() {
    make_compacted_memory_requests(MockProvider::OpenAi, false).await;
}

#[tokio::test]
async fn test_memory_survives_automatic_compact_in_anthropic_agent_loop() {
    make_compacted_memory_requests(MockProvider::Anthropic, false).await;
}

#[tokio::test]
async fn test_memory_prefix_is_cleaned_after_compact_and_openai_model_error() {
    make_compacted_memory_requests(MockProvider::OpenAi, true).await;
}

#[tokio::test]
async fn test_memory_prefix_is_cleaned_after_compact_and_anthropic_model_error() {
    make_compacted_memory_requests(MockProvider::Anthropic, true).await;
}
