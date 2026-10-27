use super::{build_agent, build_agent_with_embedded_memory, AcpAgentConfig};
use crate::{
    provider::{LlmProvider, PeriConfig},
    session::agent_pool::AgentPool,
};
use async_trait::async_trait;
use cc_agent::{
    agent::{AgentCancellationToken, AgentEvent, AgentInput, AgentState, FnEventHandler},
    interaction::{InteractionContext, InteractionResponse, UserInteractionBroker},
};
use cc_middlewares::{
    hitl::{ApprovalMemory, PermissionMode, SharedPermissionMode},
    tool_search::ToolSearchIndex,
};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MEMORY_SENTINEL: &str = "MEMORY_INDEX_SENTINEL_732";
const CUSTOM_PROMPT_SENTINEL: &str = "CUSTOM_SYSTEM_PROMPT_SENTINEL_732";

#[derive(Clone, Copy)]
enum MockProvider {
    OpenAi,
    Anthropic,
}

#[derive(Clone)]
struct MockModelRequest {
    path: String,
    body: Value,
}

struct MockModelEndpoint {
    base_url: String,
    requests: Arc<Mutex<Vec<MockModelRequest>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockModelEndpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct MockUnusedBroker;

#[async_trait]
impl UserInteractionBroker for MockUnusedBroker {
    async fn request(&self, _context: InteractionContext) -> InteractionResponse {
        panic!("固定文本响应不应触发用户交互");
    }
}

fn make_provider(protocol: MockProvider, base_url: &str) -> LlmProvider {
    match protocol {
        MockProvider::OpenAi => LlmProvider::OpenAi {
            api_key: "local-memory-test".to_string(),
            base_url: format!("{base_url}/v1"),
            model: "memory-test-model".to_string(),
            thinking: None,
        },
        MockProvider::Anthropic => LlmProvider::Anthropic {
            api_key: "local-memory-test".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            base_url: Some(base_url.to_string()),
            thinking: None,
        },
    }
}

fn make_agent_config(
    cwd: &Path,
    provider: LlmProvider,
    system_prompt: String,
    frozen_memory_entrypoint: Option<String>,
) -> AcpAgentConfig {
    AcpAgentConfig {
        provider,
        cwd: cwd.to_string_lossy().into_owned(),
        system_prompt,
        frozen_instructions: Some(String::new()),
        frozen_memory_entrypoint,
        jev_rule_loader: None,
        frozen_skill_summary: Some(String::new()),
        frozen_date: Some("2026-10-26".to_string()),
        event_handler: Arc::new(FnEventHandler(|_| {})),
        cancel: AgentCancellationToken::new(),
        permission_mode: SharedPermissionMode::new(PermissionMode::Bypass),
        approval_memory: ApprovalMemory::new(),
        peri_config: Arc::new(PeriConfig::default()),
        cron_scheduler: None,
        agent_overrides: None,
        preload_skills: Vec::new(),
        session_id: None,
        broker: Arc::new(MockUnusedBroker),
        shell_executor: None,
        plugin_skill_dirs: Vec::new(),
        plugin_agent_dirs: Vec::new(),
        hook_groups: Vec::new(),
        hook_session_start: false,
        mcp_pool: None,
        channel_state: None,
        tool_search_index: Arc::new(ToolSearchIndex::new()),
        shared_tools: Arc::new(RwLock::new(HashMap::new())),
        child_handler_factory: None,
        lsp_servers: Vec::new(),
        compact_config: None,
        compact_budget: None,
        compact_model: None,
        compact_event_tx: None,
        thread_store: None,
        parent_thread_id: None,
        register_runtime: None,
        deregister_runtime: None,
        background_registry: None,
    }
}

fn make_stream_response(protocol: MockProvider) -> String {
    let body = match protocol {
        MockProvider::OpenAi => format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({
                "id": "memory-test-response",
                "choices": [{
                    "index": 0,
                    "delta": {"role": "assistant", "content": "ok"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })
        ),
        MockProvider::Anthropic => [
            (
                "message_start",
                json!({"message": {"id": "memory-test-response", "usage": {"input_tokens": 1}}}),
            ),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "text_delta", "text": "ok"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "message_delta",
                json!({"delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 1}}),
            ),
            ("message_stop", json!({})),
        ]
        .into_iter()
        .map(|(event, data)| format!("event: {event}\ndata: {data}\n\n"))
        .collect::<String>(),
    };
    make_http_stream_response(&body)
}

fn make_http_stream_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn make_fork_response() -> String {
    let body = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({
            "id": "memory-fork-test-response",
            "choices": [{
                "index": 0,
                "delta": {"role": "assistant", "tool_calls": [{
                    "index": 0,
                    "id": "memory-fork-test-call",
                    "type": "function",
                    "function": {
                        "name": "Agent",
                        "arguments": json!({
                            "fork": true,
                            "description": "Check memory inheritance",
                            "prompt": "FORK_CHILD_INPUT_SENTINEL_732"
                        }).to_string()
                    }
                }]},
                "finish_reason": "tool_calls"
            }]
        })
    );
    make_http_stream_response(&body)
}

async fn make_model_request(
    stream: &mut tokio::net::TcpStream,
) -> anyhow::Result<MockModelRequest> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        anyhow::ensure!(count > 0, "本地模型请求未完整发送");
        request.extend_from_slice(&buffer[..count]);
        anyhow::ensure!(request.len() <= 2 * 1024 * 1024, "本地模型请求异常过大");
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header = std::str::from_utf8(&request[..header_end])?;
        let content_length = header
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .ok_or_else(|| anyhow::anyhow!("本地模型请求缺少正文长度"))?
            .1
            .trim()
            .parse::<usize>()?;
        let body_start = header_end + 4;
        if request.len() < body_start + content_length {
            continue;
        }
        let path = header
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| anyhow::anyhow!("本地模型请求缺少路径"))?
            .to_string();
        let body = serde_json::from_slice(&request[body_start..body_start + content_length])?;
        return Ok(MockModelRequest { path, body });
    }
}

async fn make_model_endpoint(
    protocol: MockProvider,
    initial_response: Option<String>,
) -> MockModelEndpoint {
    make_scripted_model_endpoint(protocol, initial_response.into_iter().collect()).await
}

async fn make_scripted_model_endpoint(
    protocol: MockProvider,
    responses: Vec<String>,
) -> MockModelEndpoint {
    // 只绑定本机回环端口；测试不修改全局配置、环境变量或用户记忆文件。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("应能绑定本地模型端口");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("应能读取本地端口")
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&requests);
    let response = make_stream_response(protocol);
    let task = tokio::spawn(async move {
        let mut responses = responses.into_iter();
        while let Ok((mut stream, _)) = listener.accept().await {
            let request =
                tokio::time::timeout(Duration::from_secs(10), make_model_request(&mut stream))
                    .await
                    .expect("本地模型请求应及时发送")
                    .expect("本地模型请求应是完整 JSON");
            captured_requests.lock().push(request);
            let response = responses.next().unwrap_or_else(|| response.clone());
            stream
                .write_all(response.as_bytes())
                .await
                .expect("本地模型应能返回固定响应");
        }
    });
    MockModelEndpoint {
        base_url,
        requests,
        task,
    }
}

#[path = "builder_memory_compact_test.rs"]
mod compact_tests;

async fn make_two_turn_requests(
    protocol: MockProvider,
    embedded_memory: bool,
) -> Vec<MockModelRequest> {
    let directory = tempfile::tempdir().expect("应能创建独立测试目录");
    let endpoint = make_model_endpoint(protocol, None).await;
    let system_prompt = if embedded_memory {
        format!(
            "Test agent\n\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\n\n{}",
            cc_middlewares::memory::build_memory_read_prompt("/test/memory", Some(MEMORY_SENTINEL))
        )
    } else {
        CUSTOM_PROMPT_SENTINEL.to_string()
    };
    // 即便兼容字段仍有同一份索引，embedded 入口也必须仅使用提示词中的正文。
    let config = make_agent_config(
        directory.path(),
        make_provider(protocol, &endpoint.base_url),
        system_prompt,
        Some(MEMORY_SENTINEL.to_string()),
    );
    let cancel = config.cancel.clone();
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    let (agent, _) = if embedded_memory {
        build_agent_with_embedded_memory(config, None, &pool)
    } else {
        build_agent(config, None, &pool)
    };
    let mut state = AgentState::new(directory.path().to_string_lossy());
    for input in ["first turn", "second turn"] {
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            agent
                .executor
                .execute(AgentInput::text(input), &mut state, Some(cancel.clone())),
        )
        .await
        .expect("真实中间件链应及时完成本地模型调用")
        .expect("真实中间件链应接受固定模型响应");
        assert_eq!(output.text, "ok", "应执行真实模型适配及完整响应路径");
        assert!(output.tool_calls.is_empty(), "固定响应不应执行工具");
        let history = serde_json::to_string(&state.messages).expect("消息历史应能序列化");
        assert_eq!(
            history.matches(MEMORY_SENTINEL).count(),
            0,
            "临时系统记忆不应残留到下一轮历史"
        );
    }
    let requests = endpoint.requests.lock().clone();
    assert_eq!(requests.len(), 2, "两轮对话应各请求一次本地模型");
    for request in &requests {
        assert_eq!(request.body["stream"], true, "应覆盖真实流式请求");
        let serialized = request.body.to_string();
        assert_eq!(
            serialized.matches(MEMORY_SENTINEL).count(),
            1,
            "最终请求中的记忆索引只能出现一次"
        );
    }
    assert!(
        requests[1].body["messages"]
            .to_string()
            .contains("first turn"),
        "第二轮应保留第一轮历史"
    );
    assert!(
        requests[1].body["messages"]
            .to_string()
            .contains("second turn"),
        "第二轮应包含本轮输入"
    );
    requests
}

#[tokio::test]
async fn test_build_agent_memory_middleware_registration() {
    let directory = tempfile::tempdir().expect("应能创建独立测试目录");
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    for frozen_memory in [None, Some(String::new()), Some(MEMORY_SENTINEL.to_string())] {
        let config = make_agent_config(
            directory.path(),
            make_provider(MockProvider::OpenAi, "http://127.0.0.1:1"),
            "custom prompt".to_string(),
            frozen_memory,
        );
        let (agent, _) = build_agent_with_embedded_memory(config, None, &pool);
        assert!(
            !agent
                .executor
                .middleware_names()
                .contains(&"MemoryMiddleware"),
            "embedded 入口无论索引为空、非空或未提供，都不应注册记忆中间件"
        );
    }
    let config = make_agent_config(
        directory.path(),
        make_provider(MockProvider::OpenAi, "http://127.0.0.1:1"),
        "custom prompt".to_string(),
        Some(String::new()),
    );
    let (agent, _) = build_agent(config, None, &pool);
    assert!(
        agent
            .executor
            .middleware_names()
            .contains(&"MemoryMiddleware"),
        "公开兼容入口应继续为自定义提示词注册记忆中间件"
    );
}

#[tokio::test]
async fn test_build_agent_embedded_memory_openai_requests_include_index_once_across_turns() {
    let requests = make_two_turn_requests(MockProvider::OpenAi, true).await;
    let mut system_messages = Vec::new();
    for request in &requests {
        assert_eq!(
            request.path, "/v1/chat/completions",
            "应通过 OpenAI 真实请求路径"
        );
        let systems: Vec<_> = request.body["messages"]
            .as_array()
            .expect("OpenAI 消息应为数组")
            .iter()
            .filter(|message| message["role"] == "system")
            .cloned()
            .collect();
        assert_eq!(
            Value::Array(systems.clone())
                .to_string()
                .matches(MEMORY_SENTINEL)
                .count(),
            1,
            "唯一索引应位于系统提示词中"
        );
        system_messages.push(systems);
    }
    assert_eq!(
        system_messages[0], system_messages[1],
        "多轮系统提示词应保持稳定"
    );
}

#[tokio::test]
async fn test_build_agent_embedded_memory_anthropic_requests_include_index_once_across_turns() {
    let requests = make_two_turn_requests(MockProvider::Anthropic, true).await;
    for request in &requests {
        assert_eq!(
            request.path, "/v1/messages",
            "应通过 Anthropic 真实请求路径"
        );
        assert_eq!(
            request.body["system"]
                .to_string()
                .matches(MEMORY_SENTINEL)
                .count(),
            1,
            "唯一索引应位于 Anthropic 系统段中"
        );
        assert!(
            !request.body["messages"]
                .to_string()
                .contains(MEMORY_SENTINEL),
            "普通历史消息不应携带系统记忆副本"
        );
    }
    assert_eq!(
        requests[0].body["system"], requests[1].body["system"],
        "多轮系统提示词及缓存块应保持稳定"
    );
}

#[tokio::test]
async fn test_build_agent_legacy_custom_prompt_keeps_memory_injection() {
    let requests = make_two_turn_requests(MockProvider::OpenAi, false).await;
    for request in &requests {
        assert_eq!(
            request
                .body
                .to_string()
                .matches(CUSTOM_PROMPT_SENTINEL)
                .count(),
            1,
            "旧入口应保留调用方自定义提示词"
        );
        assert_eq!(
            request.body.to_string().matches(MEMORY_SENTINEL).count(),
            1,
            "旧入口应继续通过记忆中间件补齐索引"
        );
    }
}

#[tokio::test]
async fn test_build_agent_fork_does_not_inherit_parent_memory_copy() {
    let directory = tempfile::tempdir().expect("应能创建独立测试目录");
    let endpoint = make_model_endpoint(MockProvider::OpenAi, Some(make_fork_response())).await;
    let system_prompt = format!(
        "Test agent\n\n__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__\n\n{}",
        cc_middlewares::memory::build_memory_read_prompt("/test/memory", Some(MEMORY_SENTINEL))
    );
    let config = make_agent_config(
        directory.path(),
        make_provider(MockProvider::OpenAi, &endpoint.base_url),
        system_prompt,
        Some(MEMORY_SENTINEL.to_string()),
    );
    let cancel = config.cancel.clone();
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    let (mut agent, _) = build_agent_with_embedded_memory(config, None, &pool);
    let mut state = AgentState::new(directory.path().to_string_lossy());
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        agent.executor.execute(
            AgentInput::text("FORK_PARENT_HISTORY_SENTINEL_732"),
            &mut state,
            Some(cancel),
        ),
    )
    .await
    .expect("父 Agent 应及时完成本地模型调用")
    .expect("父 Agent 应成功启动 fork");
    assert_eq!(output.text, "ok", "父 Agent 应继续处理工具结果");
    let child_result = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = agent.bg_event_rx.recv().await {
            if let AgentEvent::BackgroundTaskCompleted(result) = event {
                return result;
            }
        }
        panic!("fork 完成前不应关闭事件通道");
    })
    .await
    .expect("fork 应及时完成本地模型调用");
    assert!(
        child_result.success,
        "fork 应成功完成：{}",
        child_result.output
    );
    let requests = endpoint.requests.lock().clone();
    assert_eq!(requests.len(), 3, "父两次、子一次，无额外记忆模型请求");
    let child_request = requests
        .iter()
        .find(|request| {
            request.body["messages"]
                .as_array()
                .and_then(|messages| messages.last())
                .is_some_and(|message| {
                    message["role"] == "user"
                        && message["content"]
                            .to_string()
                            .contains("FORK_CHILD_INPUT_SENTINEL_732")
                })
        })
        .expect("应捕获真正的 fork 模型请求");
    let child_body = child_request.body.to_string();
    assert!(
        child_body.contains("FORK_PARENT_HISTORY_SENTINEL_732"),
        "fork 必须真实继承父对话历史"
    );
    // 父提示词使用独立哨兵；临时项目没有该索引，子提示词只能含自身读取规则。
    assert!(
        !child_body.contains(MEMORY_SENTINEL),
        "父系统记忆不得经 before_agent 快照泄漏给 fork"
    );
    assert_eq!(
        child_body.matches("# Memory").count(),
        1,
        "子请求只能携带自身的一份精简记忆规则"
    );
}
