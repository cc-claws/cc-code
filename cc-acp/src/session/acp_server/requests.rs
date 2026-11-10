//! ACP Request dispatch — handles all ACP protocol request methods.
//! Extracted from original acp_server.rs (2026-05-20 split).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tracing::{debug, info};

use cc_acp::{dispatch, provider::parse_model_selection_value, transport::types::AcpError};
use cc_agent::thread::{ThreadId, ThreadMeta};

use agent_client_protocol::schema::{
    CloseSessionResponse, ForkSessionResponse, ListSessionsResponse, LoadSessionResponse,
    NewSessionResponse, ResumeSessionResponse, SessionId, SessionInfo,
    SetSessionConfigOptionResponse, SetSessionModeResponse, SetSessionModelResponse,
};

use crate::provider::{save_to, LlmProvider};

use super::{
    apply_thinking_effort, build_mode_state, build_model_state,
    notify::{
        extract_session_id, send_available_commands_update, send_config_option_update,
        session_config_options,
    },
    parse_permission_mode, worktree_requests, AcpServerConfig, SessionState,
};

fn persist_config(cfg: &AcpServerConfig) {
    let c = cfg.peri_config.read();
    if let Err(e) = save_to(&c, &cfg.config_path) {
        tracing::warn!(error = %e, "Failed to persist config");
    }
}

fn project_config(cfg: &AcpServerConfig, cwd: &str) -> cc_acp::provider::AppConfig {
    let fallback = cfg.peri_config.read();
    cc_acp::provider::store::load_for_session(std::path::Path::new(cwd), &fallback)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, %cwd, "Failed to load session project settings");
            fallback.clone()
        })
        .config
}

async fn persist_session_snapshot(cfg: &AcpServerConfig, session_id: &str, cwd: &str) {
    let config = crate::provider::PeriConfig {
        config: project_config(cfg, cwd),
        ..Default::default()
    };
    let provider = LlmProvider::from_config(&config).unwrap_or_else(|| cfg.provider.read().clone());
    cc_acp::session::executor::persist_runtime_snapshot(
        cfg.thread_store.as_ref(),
        session_id,
        &provider,
        cwd,
    )
    .await;
}

fn apply_model_selection(cfg: &AcpServerConfig, model_id: &str) -> Option<LlmProvider> {
    let (provider_id, alias) = parse_model_selection_value(model_id);
    {
        let mut c = cfg.peri_config.write();
        let mut final_provider_id = provider_id.map(|s| s.to_string());
        let mut final_alias = alias.to_string();

        let is_standard_alias = cc_acp::provider::ProviderModels::ALL_ALIASES
            .iter()
            .any(|a| a.eq_ignore_ascii_case(alias));
        if !is_standard_alias {
            // 防御兜底：若传入的是具体模型全名而非 alias，在各 provider 中按模型名反查匹配的 (provider_id, alias)
            let matched = c.config.providers.iter().find_map(|p| {
                if let Some(ref target_pid) = final_provider_id {
                    if &p.id != target_pid {
                        return None;
                    }
                }
                for a in cc_acp::provider::ProviderModels::ALL_ALIASES {
                    if let Some(m) = p.models.get_model(a) {
                        if m.eq_ignore_ascii_case(alias) {
                            return Some((p.id.clone(), a.to_string()));
                        }
                    }
                }
                None
            });
            if let Some((pid, a)) = matched {
                final_provider_id = Some(pid);
                final_alias = a;
            }
        }

        if let Some(ref pid) = final_provider_id {
            if c.config.providers.iter().any(|p| &p.id == pid) {
                c.config.active_provider_id = pid.clone();
            } else {
                tracing::warn!(
                    provider_id = %pid,
                    model_id = %model_id,
                    "Model selection provider not found"
                );
            }
        }
        c.config.active_alias = final_alias;
    }

    let c = cfg.peri_config.read();
    LlmProvider::from_config(&c)
}

/// 校验客户端传入的 sessionId 是合法 UUID 格式。
///
/// ThreadId 是 String 类型，未做校验会让任意字符串直接作为 SQLite 主键
/// 查询。UUID 校验拒绝路径穿越、文本垃圾等非法 ID。详见 issue #70。
fn validate_session_id(id: &str) -> Result<(), AcpError> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AcpError::new(-32602, "sessionId must be a valid UUID"))
}

pub(crate) async fn handle_request(
    method: &str,
    params: &Value,
    cfg: &AcpServerConfig,
    sessions: &mut HashMap<String, SessionState>,
    transport: &dyn cc_acp::transport::AcpTransport,
) -> Result<Value, AcpError> {
    match method {
        "workspace/worktrees" | "workspace/worktree/create" => {
            worktree_requests::handle(method, params, &cfg.config_path).await
        }

        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            info!(protocol_version = %version, "ACP initialize");
            let resp = dispatch::build_initialize_response();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        // ── v1 `authenticate` / v2 `auth/login` (#24) ──
        // cc-code 的认证由自有配置（API key / OAuth）完成，此处仅做协议握手：
        // 校验 method_id 为 initialize 广播的 "cc-code"，通过即返回成功。
        "authenticate" | "auth/login" => {
            let method_id = params
                .get("methodId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if method_id != "cc-code" {
                return Err(AcpError::new(
                    -32602,
                    format!("unsupported auth method: {method_id}"),
                ));
            }
            info!("ACP authenticate (method: cc-code)");
            // AuthenticateResponse 无字段，返回空对象
            Ok(serde_json::json!({}))
        }

        // ── v1 `logout` / v2 `auth/logout` (#24) ──
        // 无服务端持久化凭证，清理由调用方持有；此处仅协议应答。
        "logout" | "auth/logout" => {
            info!("ACP logout");
            Ok(serde_json::json!({}))
        }

        // ── MCP-over-ACP (unstable, #25) ──
        // 数据流与 McpMiddleware 相反：MCP 服务器由客户端（IDE）托管，
        // Agent 经 ACP 通道收发 MCP JSON-RPC 消息。
        //
        // `mcp/connect`: 客户端声明其托管的 MCP 服务器列表。
        // params: { sessionId, servers: [{ name, version?, ... }] }
        "mcp/connect" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let state = sessions
                .get_mut(req_session_id)
                .ok_or_else(|| AcpError::new(-32602, "unknown sessionId"))?;
            let servers = params
                .get("servers")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AcpError::new(-32602, "missing servers"))?;
            let mut connected = Vec::new();
            for srv in servers {
                if let Some(name) = srv.get("name").and_then(|v| v.as_str()) {
                    state
                        .mcp_over_acp_servers
                        .insert(name.to_string(), srv.clone());
                    connected.push(name.to_string());
                }
            }
            info!(session_id = %req_session_id, connected = ?connected, "MCP-over-ACP connected");
            Ok(serde_json::json!({ "connected": connected }))
        }

        // `mcp/message` (Client → Agent)：客户端中继 MCP 服务器发来的
        // JSON-RPC 消息（通知或对 Agent 之前请求的响应）。
        // params: { sessionId, serverName, message: <JSON-RPC> }
        // 当前实现记录并确认接收；完整双向路由由 Agent 侧按需经
        // transport.send_request("mcp/message", …) 发起。
        "mcp/message" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let server_name = params
                .get("serverName")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing serverName"))?;
            let state = sessions
                .get(req_session_id)
                .ok_or_else(|| AcpError::new(-32602, "unknown sessionId"))?;
            if !state.mcp_over_acp_servers.contains_key(server_name) {
                return Err(AcpError::new(-32602, "unknown serverName"));
            }
            let message = params.get("message").cloned().unwrap_or(Value::Null);
            debug!(
                session_id = %req_session_id,
                server = %server_name,
                "MCP-over-ACP message received"
            );
            // JSON-RPC 响应（有 id）在这里仅确认通道畅通；
            // 实际请求/响应配对由发起方（Agent 经 send_request）持有。
            let _ = message;
            Ok(serde_json::json!({}))
        }

        // `mcp/disconnect`：客户端断开指定的 MCP 服务器。
        // params: { sessionId, serverName }
        "mcp/disconnect" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let server_name = params
                .get("serverName")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing serverName"))?;
            let state = sessions
                .get_mut(req_session_id)
                .ok_or_else(|| AcpError::new(-32602, "unknown sessionId"))?;
            state.mcp_over_acp_servers.remove(server_name);
            info!(session_id = %req_session_id, server = %server_name, "MCP-over-ACP disconnected");
            Ok(serde_json::json!({}))
        }

        "session/new" => {
            let cwd = params
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            if let Some(model_val) = params
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                if let Some(new_provider) = apply_model_selection(cfg, model_val) {
                    *cfg.provider.write() = new_provider;
                    persist_config(cfg);
                }
            }
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                &cwd,
                project_config(cfg, &cwd),
                &cfg.plugin_skill_dirs,
                &cfg.plugin_agent_dirs,
                &frozen_date,
                cc_acp::session::frozen::rule_model_from(&cfg.provider.read()),
            );
            let mut meta = ThreadMeta::new(&cwd);
            meta.kernel_id = frozen_data.kernel_id.clone();
            let thread_id = cfg
                .thread_store
                .create_thread(meta)
                .await
                .map_err(|e| AcpError::new(-32603, format!("Thread creation failed: {e}")))?;
            let session_id = thread_id.clone();
            persist_session_snapshot(cfg, &session_id, &cwd).await;
            sessions.insert(
                session_id.clone(),
                SessionState {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    cwd: cwd.clone(),
                    history: Vec::new(),
                    cancel_token: None,
                    background_registry: Arc::default(),
                    steering: None,
                    frozen: None,
                    recall_items: Vec::new(),
                    agent_pool: Arc::default(),
                    approval_memory: cc_middlewares::hitl::ApprovalMemory::new(),
                    mcp_over_acp_servers: HashMap::new(),
                },
            );

            let state = sessions.get_mut(&session_id).unwrap();
            state.frozen = Some(frozen_data);
            info!(session_id = %session_id, "ACP session created with ThreadStore");
            let modes = build_mode_state(&cfg.permission_mode);
            let models = {
                let p = cfg.provider.read();
                let c = cfg.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = session_config_options(cfg, sessions, &session_id);
            let resp = NewSessionResponse::new(SessionId::new(&*session_id))
                .modes(modes)
                .models(models)
                .config_options(config_options);
            // Scan skills for AvailableCommands
            let skill_dirs =
                cc_middlewares::SkillsMiddleware::resolve_dirs_static(&cwd, &cfg.plugin_skill_dirs);
            let skills = cc_middlewares::skills::list_skills(&skill_dirs);
            send_available_commands_update(transport, &session_id, &skills).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_model" => {
            let model_id = params.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
            let session_id = extract_session_id(params, "");
            let new_provider = apply_model_selection(cfg, model_id);
            if let Some(new_provider) = new_provider {
                info!(model_id = %model_id, model = %new_provider.model_name(), "Model changed");
                *cfg.provider.write() = new_provider;
            }
            // Model switch → invalidate cached LLM instances (Main Agent + SubAgent)
            if let Some(s) = sessions.get_mut(session_id) {
                s.agent_pool.lock().invalidate();
            }
            persist_config(cfg);
            if let Some(session) = sessions.get(session_id) {
                persist_session_snapshot(cfg, session_id, &session.cwd).await;
            }
            let resp = SetSessionModelResponse::new();
            send_config_option_update(transport, session_id, cfg, sessions).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_mode" => {
            let mode_id = params
                .get("modeId")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let session_id = extract_session_id(params, "");
            let mode = parse_permission_mode(mode_id);
            cfg.permission_mode.store(mode);
            info!(mode_id = %mode_id, "Permission mode changed");
            let resp = SetSessionModeResponse::new();
            send_config_option_update(transport, session_id, cfg, sessions).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_config_option" => {
            let config_id = params
                .get("configId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let session_id = extract_session_id(params, "");
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");
            match config_id {
                "mode" => {
                    let mode = parse_permission_mode(value);
                    cfg.permission_mode.store(mode);
                    info!(mode = %value, "Permission mode changed via configOption");
                }
                "model" => {
                    let new_provider = apply_model_selection(cfg, value);
                    if let Some(new_provider) = new_provider {
                        info!(model_id = %value, model = %new_provider.model_name(), "Model changed via configOption");
                        *cfg.provider.write() = new_provider;
                    }
                    // Model switch → invalidate cached LLM instances
                    if let Some(s) = sessions.get_mut(session_id) {
                        s.agent_pool.lock().invalidate();
                    }
                    persist_config(cfg);
                }
                "thinking_effort" => {
                    apply_thinking_effort(&cfg.peri_config, value);
                    persist_config(cfg);
                    info!(effort = %value, "Thinking effort changed via configOption (persisted)");
                }
                "context_1m" => {
                    let enabled = value == "true" || value == "1";
                    {
                        let mut c = cfg.peri_config.write();
                        c.config.context_1m = Some(enabled);
                    }
                    persist_config(cfg);
                    info!(enabled = %enabled, "Context 1M changed via configOption (persisted)");
                }
                "agent_kernel" => {
                    // 严格解析：未知内核不写配置、不切换。
                    let Some(kernel) = cc_agent::kernel::Kernel::from_config_checked(value) else {
                        return Err(AcpError::new(
                            -32602,
                            format!("未知内核: {value}，可用内核：cc / dsh / pi"),
                        ));
                    };
                    let mut next_config = cfg.peri_config.read().clone();
                    next_config.config.agent_kernel = Some(kernel.id().to_string());
                    cc_acp::session::kernel::persist_switch(
                        cfg.thread_store.as_ref(),
                        session_id,
                        &next_config,
                        &cfg.config_path,
                    )
                    .await
                    .map_err(|error| AcpError::new(-32603, format!("内核切换失败: {error}")))?;
                    *cfg.peri_config.write() = next_config;
                    if let Some(session) = sessions.get_mut(session_id) {
                        // 冻结的内核身份随切换更新；builder 每轮据此重算内核提示词段落，
                        // 无需重建整个 system_prompt（避免绕过 memory / feature / attribution）。
                        if let Some(ref mut frozen) = session.frozen {
                            frozen.kernel_id = Some(kernel.id().to_string());
                        }
                        // 内核影响 LLM adapter 的工具定义，缓存实例必须失效。
                        session.agent_pool.lock().invalidate();
                    }
                    info!(kernel = %value, "Agent kernel changed via configOption");
                }
                _ => {
                    debug!(config_id = %config_id, "Unknown config option");
                }
            }
            if matches!(config_id, "model" | "thinking_effort" | "agent_kernel") {
                if let Some(session) = sessions.get(session_id) {
                    persist_session_snapshot(cfg, session_id, &session.cwd).await;
                }
            }
            let config_options = session_config_options(cfg, sessions, session_id);
            let resp = SetSessionConfigOptionResponse::new(config_options);
            send_config_option_update(transport, session_id, cfg, sessions).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/load" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            validate_session_id(req_session_id)?;
            if let Some(model_val) = params
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                if let Some(new_provider) = apply_model_selection(cfg, model_val) {
                    *cfg.provider.write() = new_provider;
                    persist_config(cfg);
                }
            }

            // 所有权校验超出本次修复范围（需 DB schema 变更，见 issue #70 方案 3）。
            // 这里至少做存在性校验，避免对未知 sessionId 静默插入空 SessionState。
            let thread_id = ThreadId::from(req_session_id.to_string());
            let meta = cfg.thread_store.load_meta(&thread_id).await.map_err(|_| {
                AcpError::new(-32001, format!("session not found: {req_session_id}"))
            })?;
            if let Some(id) = meta.kernel_id.as_deref() {
                cc_agent::kernel::Kernel::try_from_id(id)
                    .map_err(|error| AcpError::new(-32602, error.to_string()))?;
            }
            // 历史线程的目录是持久化事实，调用方的启动目录不能覆盖它。
            let cwd = meta.cwd.as_str();

            // 已有非空历史会保留在内存中，不再读取随后会被丢弃的完整上下文。
            let needs_history = sessions
                .get(req_session_id)
                .is_none_or(|state| state.history.is_empty());
            let history = if needs_history {
                dispatch::load_session_messages(cfg.thread_store.as_ref(), req_session_id).await
            } else {
                Vec::new()
            };

            // Insert into sessions if not already present
            if let Some(state) = sessions.get_mut(req_session_id) {
                if state.history.is_empty() {
                    state.history = history;
                }
            } else {
                sessions.insert(
                    req_session_id.to_string(),
                    SessionState {
                        session_id: req_session_id.to_string(),
                        thread_id: req_session_id.to_string(),
                        cwd: cwd.to_string(),
                        history,
                        cancel_token: None,
                        background_registry: Arc::default(),
                        steering: None,
                        frozen: None,
                        recall_items: Vec::new(),
                        agent_pool: Arc::default(),
                        approval_memory: cc_middlewares::hitl::ApprovalMemory::new(),
                        mcp_over_acp_servers: HashMap::new(),
                    },
                );
            }

            // ── Freeze session data at load time ──
            if let Some(s) = sessions.get_mut(req_session_id) {
                if s.cwd != cwd {
                    return Err(AcpError::new(
                        -32602,
                        "loaded session cwd does not match thread cwd",
                    ));
                }
                // 同一 session 的反复 load 不得改变已冻结的系统提示词。
                if s.frozen.is_none() {
                    let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    s.frozen = Some(cc_acp::session::frozen::build_frozen_session_data(
                        cwd,
                        project_config(cfg, cwd),
                        &cfg.plugin_skill_dirs,
                        &cfg.plugin_agent_dirs,
                        &frozen_date,
                        cc_acp::session::frozen::rule_model_from(&cfg.provider.read()),
                    ));
                }
                // 持久化的内核身份优先于当前全局配置，保证重启后恢复原内核。
                if let (Some(frozen), Some(kernel_id)) = (s.frozen.as_mut(), meta.kernel_id.clone())
                {
                    frozen.kernel_id = Some(kernel_id);
                }
            }

            let modes = build_mode_state(&cfg.permission_mode);
            let models = {
                let p = cfg.provider.read();
                let c = cfg.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = session_config_options(cfg, sessions, req_session_id);
            let resp = LoadSessionResponse::new()
                .modes(modes)
                .models(models)
                .config_options(config_options);
            // Scan skills for AvailableCommands (same as session/new)
            let skill_dirs =
                cc_middlewares::SkillsMiddleware::resolve_dirs_static(cwd, &cfg.plugin_skill_dirs);
            let skills = cc_middlewares::skills::list_skills(&skill_dirs);
            send_available_commands_update(transport, req_session_id, &skills).await;
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/list" => {
            let threads = cfg
                .thread_store
                .list_threads()
                .await
                .map_err(|e| AcpError::new(-32603, format!("Failed to list sessions: {e}")))?;

            let cwd_filter = params.get("cwd").and_then(|v| v.as_str());

            let entries: Vec<SessionInfo> = threads
                .into_iter()
                .filter(|t| {
                    if let Some(cwd) = cwd_filter {
                        t.cwd == cwd
                    } else {
                        true
                    }
                })
                .map(|t| {
                    SessionInfo::new(
                        SessionId::new(t.id.clone()),
                        std::path::PathBuf::from(t.cwd.clone()),
                    )
                    .title(t.title.clone())
                    .updated_at(t.updated_at.to_rfc3339())
                })
                .collect();

            let resp = ListSessionsResponse::new(entries);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/close" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;

            if let Some(state) = sessions.remove(req_session_id) {
                state.background_registry.cancel_all();
                if let Some(ref token) = state.cancel_token {
                    token.cancel();
                }
                info!(session_id = %req_session_id, "Session closed");
            }
            let resp = CloseSessionResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/resume" => {
            let req_session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            validate_session_id(req_session_id)?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

            if !sessions.contains_key(req_session_id) {
                sessions.insert(
                    req_session_id.to_string(),
                    SessionState {
                        session_id: req_session_id.to_string(),
                        thread_id: req_session_id.to_string(),
                        cwd: cwd.to_string(),
                        history: Vec::new(),
                        cancel_token: None,
                        background_registry: Arc::default(),
                        steering: None,
                        frozen: None,
                        recall_items: Vec::new(),
                        agent_pool: Arc::default(),
                        approval_memory: cc_middlewares::hitl::ApprovalMemory::new(),
                        mcp_over_acp_servers: HashMap::new(),
                    },
                );
                info!(session_id = %req_session_id, "Session resumed (new)");
            } else {
                info!(session_id = %req_session_id, "Session resumed (existing)");
            }

            // ── Freeze session data at resume time ──
            // 会话冻结的内核身份优先于当前全局配置：中途改过配置不应让历史上下文
            // 与新提示词错配。
            let mut existing_kernel_id = sessions
                .get(req_session_id)
                .and_then(|s| s.frozen.as_ref())
                .and_then(|f| f.kernel_id.clone());
            // 进程重启后内存 sessions 为空，回落到持久化的 kernel_id。
            if existing_kernel_id.is_none() {
                existing_kernel_id = cfg
                    .thread_store
                    .load_meta(&req_session_id.to_string())
                    .await
                    .ok()
                    .and_then(|m| m.kernel_id);
            }
            if let Some(id) = existing_kernel_id.as_deref() {
                cc_agent::kernel::Kernel::try_from_id(id)
                    .map_err(|error| AcpError::new(-32602, error.to_string()))?;
            }
            if sessions
                .get(req_session_id)
                .is_some_and(|session| session.frozen.is_some())
            {
                return serde_json::to_value(ResumeSessionResponse::new())
                    .map_err(|error| AcpError::new(-32603, error.to_string()));
            }
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let mut frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                cwd,
                cfg.peri_config.read().config.clone(),
                &cfg.plugin_skill_dirs,
                &cfg.plugin_agent_dirs,
                &frozen_date,
                cc_acp::session::frozen::rule_model_from(&cfg.provider.read()),
            );
            if let Some(id) = existing_kernel_id {
                frozen_data.kernel_id = Some(id);
            } else {
                tracing::warn!("resume: 会话缺少 kernel_id，回退全局配置");
            }
            if let Some(s) = sessions.get_mut(req_session_id) {
                s.frozen = Some(frozen_data);
            }

            let resp = ResumeSessionResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/kernel" => {
            let id = extract_session_id(params, "");
            let session = sessions.get(id);
            if !id.is_empty() && session.is_none() {
                return Err(AcpError::new(-32602, "session not found"));
            }
            let cwd = session
                .map(|session| session.cwd.as_str())
                .unwrap_or_else(|| params.get("cwd").and_then(Value::as_str).unwrap_or("."));
            let config = cc_acp::provider::PeriConfig {
                config: project_config(cfg, cwd),
                ..Default::default()
            };
            let provider =
                LlmProvider::from_config(&config).unwrap_or_else(|| cfg.provider.read().clone());
            cc_acp::session::kernel::describe(
                session.and_then(|session| session.frozen.as_ref()),
                &config,
                &provider,
                cwd,
                &cfg.plugin_agent_dirs,
            )
            .map_err(|error| AcpError::new(-32602, error.to_string()))
        }

        "session/fork" => {
            let source_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
            let cwd = params.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

            let source_history = sessions
                .get(source_id)
                .map(|s| s.history.clone())
                .ok_or_else(|| {
                    AcpError::new(-32602, format!("source session not found: {source_id}"))
                })?;

            // fork 继承源会话的内核身份。
            let mut source_kernel_id = sessions
                .get(source_id)
                .and_then(|s| s.frozen.as_ref())
                .and_then(|f| f.kernel_id.clone());
            if source_kernel_id.is_none() {
                source_kernel_id = cfg
                    .thread_store
                    .load_meta(&source_id.to_string())
                    .await
                    .map_err(|error| AcpError::new(-32602, error.to_string()))?
                    .kernel_id;
            }

            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let mut frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                cwd,
                project_config(cfg, cwd),
                &cfg.plugin_skill_dirs,
                &cfg.plugin_agent_dirs,
                &frozen_date,
                cc_acp::session::frozen::rule_model_from(&cfg.provider.read()),
            );
            if let Some(id) = source_kernel_id {
                frozen_data.kernel_id = Some(id);
            }
            let (new_thread_id, copied_history) = dispatch::session_fork::fork_session_with_kernel(
                cfg.thread_store.as_ref(),
                source_id,
                &source_history,
                cwd,
                frozen_data.kernel_id.clone(),
            )
            .await
            .map_err(|error| AcpError::new(-32603, error))?;

            let new_session_id = new_thread_id.clone();
            persist_session_snapshot(cfg, &new_session_id, cwd).await;
            sessions.insert(
                new_session_id.clone(),
                SessionState {
                    session_id: new_session_id.clone(),
                    thread_id: new_thread_id.clone(),
                    cwd: cwd.to_string(),
                    history: copied_history,
                    cancel_token: None,
                    background_registry: Arc::default(),
                    steering: None,
                    frozen: Some(frozen_data),
                    recall_items: Vec::new(),
                    agent_pool: Arc::default(),
                    approval_memory: cc_middlewares::hitl::ApprovalMemory::new(),
                    mcp_over_acp_servers: HashMap::new(),
                },
            );

            info!(source = %source_id, new = %new_session_id, "Session forked");
            let resp = ForkSessionResponse::new(SessionId::new(new_session_id));
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/update_config" => {
            let session_id = extract_session_id(params, "");
            let new_cfg: crate::provider::PeriConfig =
                serde_json::from_value(params.get("config").cloned().unwrap_or_default())
                    .map_err(|e| AcpError::new(-32602, format!("Invalid config: {e}")))?;

            if new_cfg.config.providers.is_empty() {
                return Err(AcpError::new(-32602, "providers cannot be empty"));
            }
            let active_pid = new_cfg.config.active_provider_id.as_str();
            if !active_pid.is_empty()
                && !new_cfg.config.providers.iter().any(|p| p.id == active_pid)
            {
                return Err(AcpError::new(
                    -32602,
                    format!("active_provider_id '{active_pid}' not found"),
                ));
            }

            cc_acp::session::kernel::persist_switch(
                cfg.thread_store.as_ref(),
                session_id,
                &new_cfg,
                &cfg.config_path,
            )
            .await
            .map_err(|error| AcpError::new(-32603, format!("保存配置失败: {error}")))?;
            *cfg.peri_config.write() = new_cfg.clone();

            // 配置整体替换时，内核变更也要同步到会话冻结身份，
            // 否则 resolve_kernel 仍读旧的 frozen.kernel_id，切换不生效。
            {
                let kernel =
                    cc_agent::kernel::Kernel::from_config(new_cfg.config.agent_kernel.as_deref());
                if let Some(session) = sessions.get_mut(session_id) {
                    if let Some(ref mut frozen) = session.frozen {
                        if frozen.kernel_id.as_deref() != Some(kernel.id()) {
                            frozen.kernel_id = Some(kernel.id().to_string());
                            session.agent_pool.lock().invalidate();
                        }
                    }
                }
            }

            if let Some(p) = LlmProvider::from_config(&new_cfg) {
                *cfg.provider.write() = p;
            } else {
                tracing::warn!(
                    "update_config: LlmProvider::from_config returned None, provider not updated"
                );
            }

            // Model switch → invalidate cached LLM instances (Main Agent + SubAgent)
            if let Some(s) = sessions.get_mut(session_id) {
                s.agent_pool.lock().invalidate();
            }

            if let Some(session) = sessions.get(session_id) {
                persist_session_snapshot(cfg, session_id, &session.cwd).await;
            }

            let config_options = session_config_options(cfg, sessions, session_id);
            send_config_option_update(transport, session_id, cfg, sessions).await;
            serde_json::to_value(SetSessionConfigOptionResponse::new(config_options))
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        _ => Err(AcpError::new(-32601, format!("Method not found: {method}"))),
    }
}

#[cfg(test)]
#[path = "requests_test.rs"]
mod tests;
