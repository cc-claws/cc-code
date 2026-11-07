//! builder 接线回归测试：HookMiddleware 必须拿到真实 session_id。
//!
//! 外部 hook 生态（如 clawd 宠物）按 session_id 聚合会话上报，builder 若把
//! session_id 硬编码为空串，所有 peri 会话会在接收端合并成同一条记录互相
//! 覆盖（2 个会话只显示 1 个）。本测试注册 SessionStart hook 落盘 stdin
//! payload 的 session_id，断言其等于 AcpAgentConfig 传入的真实会话 ID。

use super::{build_agent, AcpAgentConfig};
use crate::{
    provider::{LlmProvider, PeriConfig},
    session::agent_pool::AgentPool,
};
use cc_agent::agent::{AgentCancellationToken, AgentInput, AgentState, FnEventHandler};
use cc_middlewares::{
    hooks::types::{HookEvent, HookType, RegisteredHook},
    hitl::{ApprovalMemory, PermissionMode, SharedPermissionMode},
    tool_search::ToolSearchIndex,
};
use parking_lot::{Mutex, RwLock};
use serde_json::json;
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const EXPECTED_SESSION_ID: &str = "clawd-regression-session-732";

fn make_stream_response() -> String {
    let body = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({
            "id": "hook-session-test-response",
            "choices": [{
                "index": 0,
                "delta": {"role": "assistant", "content": "ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        })
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn make_hook_session_config(
    cwd: &Path,
    base_url: &str,
    hook_groups: Vec<Vec<RegisteredHook>>,
) -> AcpAgentConfig {
    AcpAgentConfig {
        provider: LlmProvider::OpenAi {
            api_key: "local-hook-session-test".to_string(),
            base_url: format!("{base_url}/v1"),
            model: "hook-session-test-model".to_string(),
            thinking: None,
        },
        cwd: cwd.to_string_lossy().into_owned(),
        system_prompt: "hook session test".to_string(),
        frozen_instructions: Some(String::new()),
        frozen_memory_entrypoint: None,
        jev_rule_loader: None,
        frozen_skill_summary: Some(String::new()),
        frozen_date: Some("2026-11-07".to_string()),
        event_handler: Arc::new(FnEventHandler(|_| {})),
        cancel: AgentCancellationToken::new(),
        permission_mode: SharedPermissionMode::new(PermissionMode::Bypass),
        approval_memory: ApprovalMemory::new(),
        peri_config: Arc::new(PeriConfig::default()),
        cron_scheduler: None,
        agent_overrides: None,
        preload_skills: Vec::new(),
        session_id: Some(EXPECTED_SESSION_ID.to_string()),
        broker: Arc::new(MockUnusedBroker),
        shell_executor: None,
        plugin_skill_dirs: Vec::new(),
        plugin_agent_dirs: Vec::new(),
        hook_groups,
        hook_session_start: true,
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
        hook_event_tx: None,
        thread_store: None,
        parent_thread_id: None,
        register_runtime: None,
        deregister_runtime: None,
        background_registry: None,
        kernel: cc_agent::kernel::Kernel::default(),
    }
}

struct MockUnusedBroker;

#[async_trait::async_trait]
impl cc_agent::interaction::UserInteractionBroker for MockUnusedBroker {
    async fn request(
        &self,
        _context: cc_agent::interaction::InteractionContext,
    ) -> cc_agent::interaction::InteractionResponse {
        panic!("固定文本响应不应触发用户交互");
    }
}

/// build_agent 构造 HookMiddleware 时必须把 AcpAgentConfig.session_id 传入，
/// SessionStart hook 的 stdin payload.session_id 应等于真实会话 ID。
/// 回归目标：builder 曾把 session_id 硬编码为空串，导致外部 hook 按
/// session_id 聚合时全部 peri 会话合并成一条（clawd 宠物 2 会话只显示 1 个）。
#[cfg(unix)]
#[tokio::test]
async fn test_build_agent_passes_real_session_id_to_hooks() {
    let directory = tempfile::tempdir().expect("应创建临时目录");
    let marker_path = directory.path().join("session_id_marker.txt");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("应绑定本地端口");
    let base_url = format!("http://{}", listener.local_addr().expect("应取得本地地址"));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let resp = make_stream_response();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
        }
    });
    let marker = marker_path.to_string_lossy().into_owned();
    let hook: HookType = serde_json::from_value(json!({
        "type": "command",
        "command": format!(
            "python3 -c \"import json,sys; d=json.load(sys.stdin); open('{}','w').write(d['session_id'])\"",
            marker
        ),
    }))
    .unwrap();
    let registered = RegisteredHook {
        hook,
        event: HookEvent::SessionStart,
        matcher: None,
        plugin_name: "test-plugin".to_string(),
        plugin_id: "test-plugin-id".to_string(),
        plugin_root: Path::new("/tmp/test-plugin").to_path_buf(),
        plugin_data_dir: Path::new("/tmp/test-plugin-data").to_path_buf(),
        plugin_options: HashMap::new(),
    };
    let config = make_hook_session_config(directory.path(), &base_url, vec![vec![registered]]);
    let cancel = config.cancel.clone();
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    let (agent, _) = build_agent(config, None, &pool);
    let mut state = AgentState::new(directory.path().to_string_lossy());
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        agent
            .executor
            .execute(AgentInput::text("hi"), &mut state, Some(cancel.clone())),
    )
    .await
    .expect("真实中间件链应及时完成本地模型调用")
    .expect("真实中间件链应接受固定模型响应");
    assert_eq!(output.text, "ok", "应执行真实模型适配及完整响应路径");
    let content = std::fs::read_to_string(&marker_path)
        .expect("SessionStart hook 应把 session_id 写入 marker 文件");
    assert_eq!(
        content, EXPECTED_SESSION_ID,
        "HookMiddleware 必须把真实 session_id 传入 hook payload，当前为: {}",
        content
    );
}
