use super::*;
use crate::hooks::types::HookEvent;
use cc_agent::agent::react::Reasoning;
use cc_agent::messages::MessageId;
use std::path::PathBuf;

fn make_registered() -> RegisteredHook {
    RegisteredHook {
        hook: serde_json::from_str(r#"{"type":"command","command":"echo"}"#).unwrap(),
        event: HookEvent::PreToolUse,
        matcher: None,
        plugin_name: "test-plugin".to_string(),
        plugin_id: "test-id".to_string(),
        plugin_root: PathBuf::from("/tmp/test-plugin"),
        plugin_data_dir: PathBuf::from("/tmp/test-plugin-data"),
        plugin_options: std::collections::HashMap::new(),
    }
}

fn make_hook_input() -> HookInput {
    HookInput::session_start(
        "sess-1",
        "/tmp/transcript.json",
        env!("CARGO_MANIFEST_DIR"),
        "startup",
        "opus",
    )
}

fn make_command_hook(command: &str) -> HookType {
    serde_json::from_value(serde_json::json!({
        "type": "command",
        "command": command
    }))
    .unwrap()
}

#[test]
fn test_decode_output_bytes_utf8() {
    assert_eq!(
        cc_agent::encoding::decode_output_bytes("hook 错误".as_bytes()),
        "hook 错误"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn test_decode_output_bytes_windows_gbk_fallback() {
    let gbk_bytes = [
        0xB2, 0xBB, 0xCA, 0xC7, 0xC4, 0xDA, 0xB2, 0xBF, 0xBB, 0xF2, 0xCD, 0xE2, 0xB2, 0xBF, 0xC3,
        0xFC, 0xC1, 0xEE,
    ];

    assert_eq!(
        cc_agent::encoding::decode_output_bytes(&gbk_bytes),
        "不是内部或外部命令"
    );
}

#[tokio::test]
async fn test_command_hook_echo_plain_text() {
    let hook = make_command_hook("cat");
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(action, HookAction::Allow));
}

#[cfg(unix)]
#[tokio::test]
async fn test_command_hook_exit_code_2_blocks() {
    let hook = make_command_hook("exit 2");
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(action, HookAction::Block { .. }));
}

#[tokio::test]
async fn test_command_hook_exit_code_1_reports_error() {
    let hook = make_command_hook("echo 'error msg' >&2 && exit 1");
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("code 1")),
        "exit 1 是 hook 自身出错，不阻断但必须上报：{action:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_command_hook_json_output_continue_false() {
    let hook = make_command_hook(r#"echo '{"continue":false,"stopReason":"test stop"}'"#);
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(
        action,
        HookAction::PreventContinuation {
            stop_reason: Some(ref s)
        } if s == "test stop"
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn test_command_hook_json_output_block() {
    let hook = make_command_hook(r#"echo '{"decision":"block","reason":"not allowed"}'"#);
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(
        action,
        HookAction::Block {
            reason: ref r
        } if r == "not allowed"
    ));
}

#[tokio::test]
async fn test_command_hook_timeout_reports_error() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "command",
        "command": "sleep 10",
        "timeout": 1
    }))
    .unwrap();
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("timed out")),
        "超时不阻断调用，但要作为 hook 失败上报：{action:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_command_hook_exit_code_2_with_stdout_reason() {
    let hook = make_command_hook("echo 'custom block reason' && exit 2");
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(
        action,
        HookAction::Block {
            reason: ref r
        } if r == "custom block reason"
    ));
}

#[tokio::test]
async fn test_command_hook_plugin_options_env() {
    let mut registered = make_registered();
    registered
        .plugin_options
        .insert("api_key".to_string(), serde_json::json!("sk-test-123"));

    let hook = make_command_hook("echo $CLAUDE_PLUGIN_OPTION_API_KEY");
    let input = make_hook_input();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(action, HookAction::Allow));
}

// === sanitize_header_value tests ===

#[test]
fn test_sanitize_crlf_injection() {
    let allowed: HashSet<String> = HashSet::new();
    let result = sanitize_header_value("value\r\nX-Injected: evil", &allowed);
    assert_eq!(result, "valueX-Injected: evil");
}

#[test]
fn test_sanitize_lf_only() {
    let allowed: HashSet<String> = HashSet::new();
    let result = sanitize_header_value("value\nX-Injected: evil", &allowed);
    assert_eq!(result, "valueX-Injected: evil");
}

#[test]
fn test_sanitize_cr_only() {
    let allowed: HashSet<String> = HashSet::new();
    let result = sanitize_header_value("value\rX-Injected: evil", &allowed);
    assert_eq!(result, "valueX-Injected: evil");
}

#[test]
fn test_sanitize_env_var_expansion_allowed() {
    std::env::set_var("TEST_SANITIZE_HOOK_VAR", "secret-value");
    let allowed: HashSet<String> = ["TEST_SANITIZE_HOOK_VAR".to_string()].into_iter().collect();
    let result = sanitize_header_value("Bearer ${TEST_SANITIZE_HOOK_VAR}", &allowed);
    assert_eq!(result, "Bearer secret-value");
    std::env::remove_var("TEST_SANITIZE_HOOK_VAR");
}

#[test]
fn test_sanitize_env_var_expansion_not_allowed() {
    let allowed: HashSet<String> = HashSet::new();
    let result = sanitize_header_value("Bearer ${SECRET_KEY}", &allowed);
    assert_eq!(result, "Bearer ${SECRET_KEY}");
}

#[test]
fn test_sanitize_env_var_brace_expansion() {
    std::env::set_var("TEST_SANITIZE_HOOK_BRACE", "expanded");
    let allowed: HashSet<String> = ["TEST_SANITIZE_HOOK_BRACE".to_string()]
        .into_iter()
        .collect();
    let result = sanitize_header_value("token-${TEST_SANITIZE_HOOK_BRACE}", &allowed);
    assert_eq!(result, "token-expanded");
    std::env::remove_var("TEST_SANITIZE_HOOK_BRACE");
}

// === extract_structured_output tests ===

#[test]
fn test_extract_empty_messages() {
    let action = extract_structured_output(&[]);
    assert!(matches!(action, HookAction::Allow));
}

#[test]
fn test_extract_no_tool_messages() {
    let messages = vec![BaseMessage::system("no tools here")];
    let action = extract_structured_output(&messages);
    assert!(matches!(action, HookAction::Allow));
}

#[test]
fn test_extract_ai_message_json() {
    use cc_agent::messages::MessageContent;

    let messages = vec![BaseMessage::Ai {
        id: MessageId::new(),
        content: MessageContent::text(r#"{"decision":"block","reason":"ai says no"}"#),
        tool_calls: vec![],
    }];
    let action = extract_structured_output(&messages);
    assert!(matches!(
        action,
        HookAction::Block {
            reason: ref r
        } if r == "ai says no"
    ));
}

#[test]
fn test_extract_ai_message_plain_text() {
    use cc_agent::messages::MessageContent;

    let messages = vec![BaseMessage::Ai {
        id: MessageId::new(),
        content: MessageContent::text("just some text"),
        tool_calls: vec![],
    }];
    let action = extract_structured_output(&messages);
    assert!(matches!(action, HookAction::Allow));
}

#[test]
fn test_extract_tool_message_with_json() {
    use cc_agent::messages::MessageContent;

    let messages = vec![BaseMessage::Tool {
        id: MessageId::new(),
        tool_call_id: "tc-1".into(),
        content: MessageContent::text(r#"{"continue":false,"stopReason":"agent stop"}"#),
        is_error: false,
    }];
    let action = extract_structured_output(&messages);
    assert!(matches!(
        action,
        HookAction::PreventContinuation {
            stop_reason: Some(ref s)
        } if s == "agent stop"
    ));
}

// === HTTP hook tests (no mock server, just SSRF/blocking logic) ===

#[tokio::test]
async fn test_http_hook_ssrf_blocked() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": "http://192.168.1.1/hook",
        "timeout": 5
    }))
    .unwrap();
    let input = make_hook_input();
    let action = execute_http_hook(&hook, &input).await;
    assert!(matches!(action, HookAction::Block { .. }));
}

#[tokio::test]
async fn test_http_hook_invalid_url() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": "not-a-valid-url",
        "timeout": 5
    }))
    .unwrap();
    let input = make_hook_input();
    let action = execute_http_hook(&hook, &input).await;
    assert!(matches!(action, HookAction::Block { .. }));
}

#[tokio::test]
async fn test_http_hook_circuit_breaker_skips_when_open() {
    let _guard = crate::hooks::http_health::test_lock().await;
    let url = "http://127.0.0.1:49998/hook";
    crate::hooks::http_health::reset_all_http_health_for_test();
    let health = crate::hooks::http_health::http_health_for(url);
    health.trip();
    assert!(health.is_open());

    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": url,
        "timeout": 600
    }))
    .unwrap();
    let input = make_hook_input();
    let start = std::time::Instant::now();
    let action = execute_http_hook(&hook, &input).await;
    let elapsed = start.elapsed();

    assert!(matches!(action, HookAction::Allow));
    assert!(elapsed < std::time::Duration::from_millis(50));
    crate::hooks::http_health::reset_all_http_health_for_test();
}

#[tokio::test]
async fn test_http_hook_connect_failure_trips_circuit_breaker() {
    let _guard = crate::hooks::http_health::test_lock().await;
    let url = "http://127.0.0.1:49999/permission";
    crate::hooks::http_health::reset_all_http_health_for_test();
    let health = crate::hooks::http_health::http_health_for(url);
    assert!(!health.is_open());

    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": url,
        "timeout": 600
    }))
    .unwrap();
    let input = make_hook_input();
    let start = std::time::Instant::now();
    let action = execute_http_hook(&hook, &input).await;
    let elapsed = start.elapsed();

    // 验证即使配置了 600 秒超时，连接失败也在 connect_timeout (800ms) 内快速退出
    // 语义：连接失败 = hook 端点没跑通 → Error（不阻断，但要上报给 UI）
    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("hook request failed")),
        "连接失败必须上报为 hook 失败：{action:?}"
    );
    assert!(elapsed < std::time::Duration::from_secs(3));
    // 验证熔断已被触发
    assert!(health.is_open());

    // 验证后续请求直接瞬间跳过（熔断期间不上报失败，避免刷屏）
    let start_second = std::time::Instant::now();
    let action_second = execute_http_hook(&hook, &input).await;
    assert!(matches!(action_second, HookAction::Allow));
    assert!(start_second.elapsed() < std::time::Duration::from_millis(50));

    crate::hooks::http_health::reset_all_http_health_for_test();
}

// === Wrong hook type dispatch tests ===

#[tokio::test]
async fn test_command_hook_wrong_type_returns_allow() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": "http://example.com"
    }))
    .unwrap();
    let input = make_hook_input();
    let registered = make_registered();
    let action = execute_command_hook(&hook, &input, &registered).await;
    assert!(matches!(action, HookAction::Allow));
}

#[tokio::test]
async fn test_prompt_hook_wrong_type_returns_allow() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "command",
        "command": "echo test"
    }))
    .unwrap();
    let input = make_hook_input();
    let llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync> =
        Arc::new(|| unimplemented!());
    let action = execute_prompt_hook(&hook, &input, &llm_factory).await;
    assert!(matches!(action, HookAction::Allow));
}

#[tokio::test]
async fn test_http_hook_wrong_type_returns_allow() {
    let hook = make_command_hook("echo test");
    let input = make_hook_input();
    let action = execute_http_hook(&hook, &input).await;
    assert!(matches!(action, HookAction::Allow));
}

#[tokio::test]
async fn test_agent_hook_wrong_type_returns_allow() {
    let hook = make_command_hook("echo test");
    let input = make_hook_input();
    let llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync> =
        Arc::new(|| unimplemented!());
    let action = execute_agent_hook(&hook, &input, &llm_factory, "/tmp").await;
    assert!(matches!(action, HookAction::Allow));
}

/// reason 最终渲染成单行：换行/制表符折叠成空格，ANSI 转义剥离。
#[test]
fn test_hook_error_reason_flattens_whitespace_and_ansi() {
    let reason = hook_error_reason(
        "hook exited with code 1",
        "\x1b[31mboom\x1b[0m\nsecond\tline",
    );
    assert_eq!(reason, "hook exited with code 1: boom second line");
}

/// 只有空白（或全被剥掉）的 stderr 只保留前缀，不留悬空冒号。
#[test]
fn test_hook_error_reason_blank_detail_returns_prefix() {
    assert_eq!(
        hook_error_reason("hook failed to run", " \r\n\t\x1b[0m "),
        "hook failed to run"
    );
}

// === hook 自身失败上报（非阻断 Error）测试 ===

/// 恒定返回错误的 mock LLM：用于验证 prompt / agent hook 的失败分支。
struct MockFailingLlm;

#[async_trait::async_trait]
impl ReactLLM for MockFailingLlm {
    async fn generate_reasoning(
        &self,
        _messages: &[BaseMessage],
        _tools: &[&dyn cc_agent::tools::BaseTool],
        _streaming: Option<cc_agent::llm::types::StreamingContext>,
    ) -> cc_agent::error::AgentResult<Reasoning> {
        Err(cc_agent::error::AgentError::LlmError(
            "mock llm unavailable".to_string(),
        ))
    }
}

fn make_failing_llm_factory() -> Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync> {
    Arc::new(|| Box::new(MockFailingLlm) as Box<dyn ReactLLM + Send + Sync>)
}

/// 起一个只回固定响应的本地 HTTP 桩（仅 127.0.0.1 环回，不走外网）。
async fn spawn_http_stub(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            // 读完整个请求（头 + content-length 指定的 body）再回包，
            // 避免带着未读数据关闭连接被对端判为 RST。
            let mut request = Vec::new();
            let mut buf = [0_u8; 4096];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => request.extend_from_slice(&buf[..count]),
                }
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });
    (url, task)
}

#[tokio::test]
async fn test_prompt_hook_llm_failure_reports_error() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "prompt",
        "prompt": "check $ARGUMENTS",
        "timeout": 5
    }))
    .unwrap();
    let input = make_hook_input();
    let action = execute_prompt_hook(&hook, &input, &make_failing_llm_factory()).await;
    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("LLM call failed")),
        "prompt hook 的 LLM 调用失败必须上报为 hook 失败：{action:?}"
    );
}

#[tokio::test]
async fn test_agent_hook_llm_failure_reports_error() {
    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "agent",
        "prompt": "decide $ARGUMENTS",
        "timeout": 10
    }))
    .unwrap();
    let input = make_hook_input();
    let action = execute_agent_hook(
        &hook,
        &input,
        &make_failing_llm_factory(),
        env!("CARGO_MANIFEST_DIR"),
    )
    .await;
    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("hook agent failed")),
        "agent hook 执行失败必须上报为 hook 失败：{action:?}"
    );
}

#[tokio::test]
async fn test_http_hook_non_success_status_reports_error() {
    let _guard = crate::hooks::http_health::test_lock().await;
    crate::hooks::http_health::reset_all_http_health_for_test();
    let (url, server) = spawn_http_stub("500 Internal Server Error", r#"{"error":"boom"}"#).await;

    let hook: HookType = serde_json::from_value(serde_json::json!({
        "type": "http",
        "url": format!("{url}/hook"),
        "timeout": 5
    }))
    .unwrap();
    let input = make_hook_input();
    let action = execute_http_hook(&hook, &input).await;
    server.abort();
    crate::hooks::http_health::reset_all_http_health_for_test();

    assert!(
        matches!(action, HookAction::Error { ref reason } if reason.contains("500")),
        "非 2xx 状态码必须上报为 hook 失败并带状态码：{action:?}"
    );
}
