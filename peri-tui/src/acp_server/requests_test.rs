use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use peri_acp::provider::{PeriConfig, ProviderConfig, ProviderModels};
use peri_acp::transport::types::{AcpError, IncomingMessage, RequestId};
use peri_agent::thread::FilesystemThreadStore;
use peri_middlewares::hitl::shared_mode::{PermissionMode, SharedPermissionMode};
use serde_json::{json, Value};

use crate::app::agent::LlmProvider;

use super::*;

// ── Mock AcpTransport ─────────────────────────────────────────────────────────

/// 丢弃所有发送操作的 mock transport
struct MockTransport;

#[async_trait]
impl peri_acp::transport::AcpTransport for MockTransport {
    async fn send_request(&self, _method: &str, _params: Value) -> Result<Value, AcpError> {
        Ok(json!({}))
    }
    async fn send_notification(&self, _method: &str, _params: Value) -> Result<(), AcpError> {
        Ok(())
    }
    async fn recv(&self) -> Option<IncomingMessage> {
        None
    }
    async fn send_response(
        &self,
        _id: RequestId,
        _result: Result<Value, AcpError>,
    ) -> Result<(), AcpError> {
        Ok(())
    }
}

// ── 辅助函数 ──────────────────────────────────────────────────────────────────

fn make_provider_config(
    id: &str,
    provider_type: &str,
    api_key: &str,
    model: &str,
) -> ProviderConfig {
    ProviderConfig {
        id: id.to_string(),
        provider_type: provider_type.to_string(),
        api_key: api_key.to_string(),
        // 将模型名填入 sonnet 别名（默认 alias）
        models: ProviderModels {
            sonnet: model.to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn make_server_config(
    peri_config: PeriConfig,
    provider: LlmProvider,
    tmp: &tempfile::TempDir,
) -> AcpServerConfig {
    let thread_store = FilesystemThreadStore::new(tmp.path().join("threads"));
    AcpServerConfig {
        provider: Arc::new(parking_lot::RwLock::new(provider)),
        peri_config: Arc::new(parking_lot::RwLock::new(peri_config)),
        permission_mode: SharedPermissionMode::new(PermissionMode::Bypass),
        cron_scheduler: None,
        mcp_pool: None,
        channel_state: None,
        plugin_skill_dirs: Vec::new(),
        plugin_agent_dirs: Vec::new(),
        plugin_hooks: Vec::new(),
        hook_groups: Vec::new(),
        plugin_lsp_servers: Vec::new(),
        tool_search_index: Arc::new(peri_middlewares::tool_search::ToolSearchIndex::new()),
        shared_tools: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        thread_store: Arc::new(thread_store),
        langfuse_session: None,
        config_path: tmp.path().join("test_config.json"),
    }
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

/// 验证 session/update_config 切换 active_provider_id 后 cfg.provider 正确更新
#[tokio::test]
async fn test_update_config_切换provider后cfg_provider更新() {
    // Arrange: 构造两个 provider（a=openai, b=anthropic），初始 active_provider_id = "a"
    let tmp = tempfile::TempDir::new().unwrap();
    let provider_a = make_provider_config("a", "openai", "sk-openai-test", "gpt-4o");
    let provider_b = make_provider_config("b", "anthropic", "sk-ant-test", "claude-sonnet-4-6");

    let mut peri_config = PeriConfig::default();
    peri_config.config.active_provider_id = "a".to_string();
    peri_config.config.active_alias = "sonnet".to_string();
    peri_config.config.providers = vec![provider_a.clone(), provider_b.clone()];

    let initial_provider = LlmProvider::from_config(&peri_config).unwrap();
    assert!(
        matches!(initial_provider, LlmProvider::OpenAi { .. }),
        "初始 provider 应为 OpenAI"
    );

    let cfg = make_server_config(peri_config.clone(), initial_provider, &tmp);
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    // 构造 update_config 参数：active_provider_id 改为 "b"
    let mut updated_config = peri_config.clone();
    updated_config.config.active_provider_id = "b".to_string();

    let params = json!({
        "sessionId": "test-session",
        "config": updated_config,
    });

    // Act: 调用 handle_request
    let result = handle_request(
        "session/update_config",
        &params,
        &cfg,
        &mut sessions,
        &transport,
    )
    .await
    .unwrap();

    // Assert: cfg.provider 应切换到 anthropic
    let provider = cfg.provider.read();
    assert!(
        matches!(&*provider, LlmProvider::Anthropic { model, .. } if model == "claude-sonnet-4-6"),
        "切换后 provider 应为 Anthropic claude-sonnet-4-6，实际: display={} model={}",
        provider.display_name(),
        provider.model_name(),
    );
    assert_eq!(
        provider.display_name(),
        "Anthropic",
        "display_name 应为 Anthropic"
    );

    // 验证返回值包含 configOptions
    assert!(
        result.get("configOptions").is_some(),
        "响应应包含 configOptions"
    );
}

/// 验证 session/update_config 空 providers 时返回错误
#[tokio::test]
async fn test_update_config_空providers返回错误() {
    let tmp = tempfile::TempDir::new().unwrap();
    let provider_a = make_provider_config("a", "openai", "sk-openai-test", "gpt-4o");

    let mut peri_config = PeriConfig::default();
    peri_config.config.active_provider_id = "a".to_string();
    peri_config.config.providers = vec![provider_a];

    let initial_provider = LlmProvider::from_config(&peri_config).unwrap();
    let cfg = make_server_config(peri_config.clone(), initial_provider, &tmp);
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    // 空 providers
    let mut bad_config = PeriConfig::default();
    bad_config.config.providers = vec![];

    let params = json!({
        "sessionId": "test-session",
        "config": bad_config,
    });

    let result = handle_request(
        "session/update_config",
        &params,
        &cfg,
        &mut sessions,
        &transport,
    )
    .await;

    assert!(result.is_err(), "空 providers 应返回错误");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("providers cannot be empty"),
        "错误消息应提及 providers 为空，实际: {}",
        err.message,
    );
}

/// 验证 session/update_config 不存在的 active_provider_id 返回错误
#[tokio::test]
async fn test_update_config_不存在的provider_id返回错误() {
    let tmp = tempfile::TempDir::new().unwrap();
    let provider_a = make_provider_config("a", "openai", "sk-openai-test", "gpt-4o");

    let mut peri_config = PeriConfig::default();
    peri_config.config.active_provider_id = "a".to_string();
    peri_config.config.providers = vec![provider_a];

    let initial_provider = LlmProvider::from_config(&peri_config).unwrap();
    let cfg = make_server_config(peri_config.clone(), initial_provider, &tmp);
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    // active_provider_id 指向不存在的 provider
    let mut bad_config = peri_config.clone();
    bad_config.config.active_provider_id = "nonexistent".to_string();
    bad_config.config.providers = vec![make_provider_config(
        "a",
        "openai",
        "sk-openai-test",
        "gpt-4o",
    )];

    let params = json!({
        "sessionId": "test-session",
        "config": bad_config,
    });

    let result = handle_request(
        "session/update_config",
        &params,
        &cfg,
        &mut sessions,
        &transport,
    )
    .await;

    assert!(result.is_err(), "不存在的 provider_id 应返回错误");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("not found"),
        "错误消息应提及 not found，实际: {}",
        err.message,
    );
}

// ── sessionId 校验测试（issue #70） ─────────────────────────────────────────

/// 构造一个最小的可运行 server config，复用 update_config 测试的 fixture 逻辑
fn make_minimal_cfg() -> (tempfile::TempDir, AcpServerConfig) {
    let tmp = tempfile::TempDir::new().unwrap();
    let provider_a = make_provider_config("a", "openai", "sk-openai-test", "gpt-4o");
    let mut peri_config = PeriConfig::default();
    peri_config.config.active_provider_id = "a".to_string();
    peri_config.config.providers = vec![provider_a];
    let provider = LlmProvider::from_config(&peri_config).unwrap();
    let cfg = make_server_config(peri_config, provider, &tmp);
    (tmp, cfg)
}

/// session/load 传入非 UUID（含路径穿越片段）应被 -32602 拒绝
#[tokio::test]
async fn test_session_load_非uuid_sessionid返回错误() {
    let (_tmp, cfg) = make_minimal_cfg();
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    let params = json!({ "sessionId": "../../etc/passwd" });

    let result = handle_request("session/load", &params, &cfg, &mut sessions, &transport).await;

    assert!(result.is_err(), "非 UUID sessionId 应返回错误");
    let err = result.unwrap_err();
    assert_eq!(
        err.code, -32602,
        "非 UUID 应返回 invalid params (-32602)，实际: {}",
        err.code,
    );
    assert!(
        err.message.contains("UUID"),
        "错误消息应提及 UUID，实际: {}",
        err.message,
    );
    // 路径穿越尝试不应触发任何 thread_store 查询，sessions map 也不应被污染
    assert!(sessions.is_empty(), "校验失败时不应静默插入 SessionState",);
}

/// session/load 传入合法 UUID 但 thread 不存在 应返回 -32001 session_not_found，
/// 而非静默插入新 SessionState（issue #70 修复点）
#[tokio::test]
async fn test_session_load_不存在uuid_返回session_not_found() {
    let (_tmp, cfg) = make_minimal_cfg();
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    // 任意合法 UUID v4，thread_store 为空目录，必定不存在
    let ghost_id = "00000000-0000-4000-8000-000000000000";
    let params = json!({ "sessionId": ghost_id });

    let result = handle_request("session/load", &params, &cfg, &mut sessions, &transport).await;

    assert!(result.is_err(), "不存在的 thread 应返回错误");
    let err = result.unwrap_err();
    assert_eq!(
        err.code, -32001,
        "不存在 thread 应返回 -32001 session_not_found，实际: {}",
        err.code,
    );
    assert!(
        err.message.contains("session not found"),
        "错误消息应包含 session not found，实际: {}",
        err.message,
    );
    // 不应静默插入空 SessionState
    assert!(
        sessions.is_empty(),
        "thread 不存在时不应静默插入 SessionState",
    );
}

/// session/load 传入已存在 thread 的合法 UUID 应正常加载
#[tokio::test]
async fn test_session_load_存在thread_正常加载() {
    use peri_agent::thread::ThreadMeta;

    let (_tmp, cfg) = make_minimal_cfg();

    // Arrange: 先在 thread_store 中创建一个 thread
    let meta = ThreadMeta::new("/tmp");
    let thread_id = cfg.thread_store.create_thread(meta).await.unwrap();

    let mut sessions = HashMap::new();
    let transport = MockTransport;
    let params = json!({ "sessionId": thread_id, "cwd": "/tmp" });

    let result = handle_request("session/load", &params, &cfg, &mut sessions, &transport).await;

    assert!(
        result.is_ok(),
        "存在 thread 应正常加载，错误: {:?}",
        result.err(),
    );
    // 加载后 sessions 应被填充（非静默插入空状态，而是合法恢复）
    assert_eq!(
        sessions.len(),
        1,
        "session/load 成功后 sessions 应包含 1 个 entry",
    );
}

/// session/resume 传入非 UUID 应被 -32602 拒绝（resume 仍允许新建，但 ID 必须合法）
#[tokio::test]
async fn test_session_resume_非uuid_sessionid返回错误() {
    let (_tmp, cfg) = make_minimal_cfg();
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    let params = json!({ "sessionId": "not-a-uuid-at-all" });

    let result = handle_request("session/resume", &params, &cfg, &mut sessions, &transport).await;

    assert!(result.is_err(), "非 UUID sessionId 应返回错误");
    let err = result.unwrap_err();
    assert_eq!(
        err.code, -32602,
        "非 UUID 应返回 invalid params (-32602)，实际: {}",
        err.code,
    );
    assert!(sessions.is_empty(), "校验失败时不应静默插入 SessionState",);
}

/// session/resume 传入合法 UUID 应允许恢复（若 sessions 中无则新建空 entry）
#[tokio::test]
async fn test_session_resume_合法uuid_允许恢复() {
    let (_tmp, cfg) = make_minimal_cfg();
    let mut sessions = HashMap::new();
    let transport = MockTransport;

    let valid_id = "11111111-2222-4333-8444-555555555555";
    let params = json!({ "sessionId": valid_id, "cwd": "/tmp" });

    let result = handle_request("session/resume", &params, &cfg, &mut sessions, &transport).await;

    assert!(
        result.is_ok(),
        "合法 UUID 应允许 resume，错误: {:?}",
        result.err(),
    );
    assert_eq!(
        sessions.len(),
        1,
        "resume 成功后 sessions 应包含 1 个 entry",
    );
}

/// validate_session_id_format 单元测试：覆盖合法/非法输入
#[tokio::test]
async fn test_validate_session_id_format_拒绝非法id() {
    use peri_acp::session::validate_session_id_format;

    // 非法:路径穿越
    assert!(
        validate_session_id_format("../../etc/passwd").is_err(),
        "路径穿越应被拒绝",
    );
    // 非法:中文字符
    assert!(
        validate_session_id_format("你好").is_err(),
        "非 ASCII 应被拒绝",
    );
    // 非法:普通字符串
    assert!(
        validate_session_id_format("test-session").is_err(),
        "普通字符串应被拒绝",
    );
    // 合法:UUID v4
    assert!(
        validate_session_id_format("00000000-0000-4000-8000-000000000000").is_ok(),
        "合法 UUID v4 应通过",
    );
    // 合法:UUID v7
    assert!(
        validate_session_id_format("017f22e2-79b0-7cc3-98c4-dc0c0c07398f").is_ok(),
        "合法 UUID v7 应通过",
    );
}
