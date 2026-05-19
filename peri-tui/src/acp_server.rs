//! ACP Server — transport-agnostic request handler.
//!
//! Accepts any [`AcpTransport`] implementation (mpsc for TUI, stdio for IDE),
//! builds and executes ReAct agents, and pushes [`SessionUpdate`] notifications
//! back through the transport.
//!
//! **Cancel architecture**: `session/prompt` execution is spawned into a
//! background tokio task so the main server loop remains responsive to
//! `$/cancel_request` notifications. Sessions are shared via
//! `Arc<tokio::sync::Mutex<HashMap>>`.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde_json::Value;
use tracing::{debug, info};

use peri_acp::broker::AcpTransportBroker;
use peri_acp::session::event_sink::TransportEventSink;
use peri_acp::session::executor;
use peri_acp::transport::types::{AcpError, IncomingMessage};
use peri_agent::agent::AgentCancellationToken;
use peri_agent::messages::BaseMessage;
use peri_middlewares::prelude::*;

use agent_client_protocol::schema::{
    AgentCapabilities, InitializeResponse, NewSessionResponse, PromptResponse, ProtocolVersion,
    SessionId, SetSessionConfigOptionResponse, SetSessionModeResponse, SetSessionModelResponse,
    StopReason,
};
use agent_client_protocol_schema::{
    ModelId, ModelInfo, SessionConfigId, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionConfigValueId, SessionMode,
    SessionModeId, SessionModeState, SessionModelState,
};

use crate::app::agent::LlmProvider;
use crate::config::PeriConfig;

// ── Session state ────────────────────────────────────────────────────────────

struct SessionState {
    #[allow(dead_code)]
    session_id: String,
    cwd: String,
    history: Vec<BaseMessage>,
    cancel_token: Option<AgentCancellationToken>,
}

// ── Server config ────────────────────────────────────────────────────────────

/// All cross-session configuration needed by the ACP server.
pub struct AcpServerConfig {
    pub provider: Arc<RwLock<LlmProvider>>,
    pub peri_config: Arc<RwLock<PeriConfig>>,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub plugin_hooks: Vec<peri_middlewares::hooks::RegisteredHook>,
    pub hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    pub plugin_lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    pub tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
    pub thread_store: Arc<dyn peri_agent::thread::ThreadStore>,
}

// ── Main server loop ────────────────────────────────────────────────────────

type SharedSessions = Arc<tokio::sync::Mutex<HashMap<String, SessionState>>>;

/// Main ACP server loop. Accepts any `AcpTransport` (mpsc for TUI, stdio for IDE).
///
/// `session/prompt` is spawned into a background task so the loop stays
/// responsive to `$/cancel_request` and other incoming messages.
pub async fn run_acp_server(
    transport: Arc<dyn peri_acp::transport::AcpTransport>,
    cfg: AcpServerConfig,
) {
    let sessions: SharedSessions = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let mut session_counter: u64 = 0;

    while let Some(msg) = transport.recv().await {
        match msg {
            IncomingMessage::Request { id, method, params } => {
                if method == "session/prompt" {
                    // Spawn long-running prompt execution so the server loop
                    // continues processing $/cancel_request notifications.
                    let sessions = sessions.clone();
                    let transport = Arc::clone(&transport);
                    let provider = cfg.provider.clone();
                    let peri_config = cfg.peri_config.clone();
                    let permission_mode = cfg.permission_mode.clone();
                    let cron_scheduler = cfg.cron_scheduler.clone();
                    let plugin_skill_dirs = cfg.plugin_skill_dirs.clone();
                    let plugin_agent_dirs = cfg.plugin_agent_dirs.clone();
                    let hook_groups = cfg.hook_groups.clone();
                    let mcp_pool = cfg.mcp_pool.clone();
                    let tool_search_index = cfg.tool_search_index.clone();
                    let shared_tools = cfg.shared_tools.clone();
                    let plugin_lsp_servers = cfg.plugin_lsp_servers.clone();
                    tokio::spawn(async move {
                        let result = execute_prompt(
                            params,
                            &sessions,
                            &provider,
                            &peri_config,
                            &permission_mode,
                            cron_scheduler,
                            &plugin_skill_dirs,
                            &plugin_agent_dirs,
                            &hook_groups,
                            mcp_pool,
                            tool_search_index,
                            shared_tools,
                            &plugin_lsp_servers,
                            &transport,
                        )
                        .await;
                        let _ = transport.send_response(id, result).await;
                    });
                } else {
                    let mut sessions = sessions.lock().await;
                    let result =
                        handle_request(&method, &params, &cfg, &mut sessions, &mut session_counter)
                            .await;
                    let _ = transport.send_response(id, result).await;
                }
            }
            IncomingMessage::Notification { method, params } => {
                let sessions = sessions.lock().await;
                handle_notification(&method, &params, &sessions);
            }
            IncomingMessage::Response { .. } => {
                // Responses are routed internally by the transport's pending map.
            }
        }
    }
}

// ── Request dispatch (quick handlers only) ───────────────────────────────────

async fn handle_request(
    method: &str,
    params: &Value,
    cfg: &AcpServerConfig,
    sessions: &mut HashMap<String, SessionState>,
    counter: &mut u64,
) -> Result<Value, AcpError> {
    match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            info!(protocol_version = %version, "ACP initialize");
            let resp = InitializeResponse::new(ProtocolVersion::V1)
                .agent_capabilities(AgentCapabilities::new());
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/new" => {
            let cwd = params
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or(".")
                .to_string();
            *counter += 1;
            let session_id = format!("session-{}", counter);
            sessions.insert(
                session_id.clone(),
                SessionState {
                    session_id: session_id.clone(),
                    cwd,
                    history: Vec::new(),
                    cancel_token: None,
                },
            );
            info!(session_id = %session_id, "ACP session created");
            let modes = build_mode_state(&cfg.permission_mode);
            let models = {
                let p = cfg.provider.read();
                let c = cfg.peri_config.read();
                build_model_state(&p, &c)
            };
            let config_options = {
                let c = cfg.peri_config.read();
                build_config_options(&c)
            };
            let resp = NewSessionResponse::new(SessionId::new(&*session_id))
                .modes(modes)
                .models(models)
                .config_options(config_options);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_model" => {
            let model_id = params.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
            let new_provider = {
                let cfg = cfg.peri_config.read();
                LlmProvider::from_config_for_alias(&cfg, model_id)
            };
            if let Some(new_provider) = new_provider {
                info!(model_id = %model_id, model = %new_provider.model_name(), "Model changed");
                *cfg.provider.write() = new_provider;
            }
            let resp = SetSessionModelResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_mode" => {
            let mode_id = params
                .get("modeId")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            let mode = parse_permission_mode(mode_id);
            cfg.permission_mode.store(mode);
            info!(mode_id = %mode_id, "Permission mode changed");
            let resp = SetSessionModeResponse::new();
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_config_option" => {
            let config_id = params
                .get("configId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");
            match config_id {
                "thinking_effort" => {
                    apply_thinking_effort(&cfg.peri_config, value);
                    info!(effort = %value, "Thinking effort changed via configOption");
                }
                _ => {
                    debug!(config_id = %config_id, "Unknown config option");
                }
            }
            let config_options = {
                let c = cfg.peri_config.read();
                build_config_options(&c)
            };
            let resp = SetSessionConfigOptionResponse::new(config_options);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        "session/set_thinking" => {
            let effort = params
                .get("effort")
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            apply_thinking_effort(&cfg.peri_config, effort);
            {
                let mut cfg_guard = cfg.peri_config.write();
                if let Some(ref mut thinking) = cfg_guard.config.thinking {
                    thinking.enabled = enabled;
                }
            }
            info!(effort = %effort, enabled = %enabled, "Thinking config changed");
            let config_options = {
                let c = cfg.peri_config.read();
                build_config_options(&c)
            };
            let resp = SetSessionConfigOptionResponse::new(config_options);
            serde_json::to_value(resp)
                .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
        }

        _ => Err(AcpError::new(-32601, format!("Method not found: {method}"))),
    }
}

// ── Notification dispatch ────────────────────────────────────────────────────

fn handle_notification(method: &str, params: &Value, sessions: &HashMap<String, SessionState>) {
    if method == "$/cancel_request" {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(state) = sessions.get(session_id) {
            if let Some(ref token) = state.cancel_token {
                token.cancel();
                info!(session_id = %session_id, "Cancel requested");
            }
        }
    } else {
        debug!(method = %method, "Unhandled notification");
    }
}

// ── Prompt execution (spawned into background task) ──────────────────────────

#[allow(clippy::too_many_arguments)]
async fn execute_prompt(
    params: Value,
    sessions: &SharedSessions,
    provider: &Arc<RwLock<LlmProvider>>,
    peri_config: &Arc<RwLock<PeriConfig>>,
    permission_mode: &Arc<SharedPermissionMode>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    plugin_skill_dirs: &[std::path::PathBuf],
    plugin_agent_dirs: &[std::path::PathBuf],
    hook_groups: &[Vec<peri_middlewares::hooks::RegisteredHook>],
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
    plugin_lsp_servers: &[peri_lsp::config::LspServerConfig],
    transport: &Arc<dyn peri_acp::transport::AcpTransport>,
) -> Result<Value, AcpError> {
    let session_id = params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?
        .to_string();
    let message = params
        .get("message")
        .ok_or_else(|| AcpError::new(-32602, "missing message"))?;
    let content = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Create cancel token and register in sessions.
    let cancel = AgentCancellationToken::new();
    {
        let mut sessions = sessions.lock().await;
        let state = sessions
            .get_mut(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        state.cancel_token = Some(cancel.clone());
    }

    // Read session data under lock, then release immediately.
    let (cwd, history, is_empty) = {
        let sessions = sessions.lock().await;
        let state = sessions
            .get(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.history.is_empty(),
        )
    };

    let broker: Arc<dyn peri_agent::interaction::UserInteractionBroker> = Arc::new(
        AcpTransportBroker::new(Arc::clone(transport), session_id.clone().into()),
    );
    let event_sink = Arc::new(TransportEventSink::new(Arc::clone(transport)));

    let provider_snapshot = provider.read().clone();
    let peri_config_snapshot = Arc::new(peri_config.read().clone());

    let result = executor::execute_prompt(
        &provider_snapshot,
        peri_config_snapshot,
        &cwd,
        content,
        history,
        is_empty,
        permission_mode.clone(),
        event_sink,
        cancel,
        broker,
        plugin_skill_dirs.to_vec(),
        plugin_agent_dirs.to_vec(),
        hook_groups.to_vec(),
        cron_scheduler,
        session_id.clone(),
        mcp_pool,
        tool_search_index,
        shared_tools,
        plugin_lsp_servers.to_vec(),
    )
    .await;

    // Update session history and clear cancel token.
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            if result.ok {
                info!(session_id = %session_id, messages = result.messages.len(), "Agent execution completed");
            }
            state.history = result.messages;
            state.cancel_token = None;
        }
    }

    let resp = PromptResponse::new(StopReason::EndTurn);
    serde_json::to_value(resp).map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}

// ── ACP standard state builders ────────────────────────────────────────────────

pub fn parse_permission_mode(mode_id: &str) -> PermissionMode {
    match mode_id {
        "dont_ask" => PermissionMode::DontAsk,
        "accept_edit" => PermissionMode::AcceptEdit,
        "auto" => PermissionMode::AutoMode,
        "bypass" => PermissionMode::Bypass,
        _ => PermissionMode::Default,
    }
}

pub fn apply_thinking_effort(peri_config: &RwLock<PeriConfig>, effort: &str) {
    let mut cfg = peri_config.write();
    let thinking = cfg
        .config
        .thinking
        .get_or_insert_with(|| crate::config::ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: "medium".to_string(),
            max_tokens: 32000,
        });
    thinking.enabled = true;
    thinking.effort = effort.to_string();
}

pub fn build_mode_state(pm: &SharedPermissionMode) -> SessionModeState {
    let current = pm.load();
    let current_id = match current {
        PermissionMode::Default => "default",
        PermissionMode::DontAsk => "dont_ask",
        PermissionMode::AcceptEdit => "accept_edit",
        PermissionMode::AutoMode => "auto",
        PermissionMode::Bypass => "bypass",
    };
    let all_modes = vec![
        SessionMode::new(SessionModeId::new("default"), "Default")
            .description("All sensitive tools require approval"),
        SessionMode::new(SessionModeId::new("dont_ask"), "Don't Ask")
            .description("Default deny all bash"),
        SessionMode::new(SessionModeId::new("accept_edit"), "Accept Edit")
            .description("Allow filesystem edits"),
        SessionMode::new(SessionModeId::new("auto"), "Auto Mode")
            .description("LLM decides approval"),
        SessionMode::new(SessionModeId::new("bypass"), "Bypass").description("Allow everything"),
    ];
    SessionModeState::new(SessionModeId::new(current_id), all_modes)
}

pub fn build_model_state(provider: &LlmProvider, peri_config: &PeriConfig) -> SessionModelState {
    let active_alias = peri_config.config.active_alias.clone();

    let active_provider = peri_config.config.providers.iter().find(|prov| {
        prov.id == peri_config.config.active_provider_id
            || peri_config.config.active_provider_id.is_empty()
    });

    let mut available = Vec::new();
    if let Some(prov) = active_provider {
        for alias in ["opus", "sonnet", "haiku"] {
            if let Some(model_name) = prov.models.get_model(alias) {
                if !model_name.is_empty() {
                    available.push(ModelInfo::new(
                        ModelId::new(alias.to_string()),
                        format!("{} ({})", alias, model_name),
                    ));
                }
            }
        }
    }
    if available.is_empty() {
        available.push(ModelInfo::new(
            ModelId::new("current".to_string()),
            provider.model_name().to_string(),
        ));
    }

    SessionModelState::new(ModelId::new(active_alias), available)
}

pub fn build_config_options(peri_config: &PeriConfig) -> Vec<SessionConfigOption> {
    let effort = peri_config
        .config
        .thinking
        .as_ref()
        .map(|t| t.effort.as_str())
        .unwrap_or("medium");

    let thinking_options = vec![
        SessionConfigSelectOption::new(SessionConfigValueId::new("low"), "Low".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("medium"), "Medium".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("high"), "High".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("xhigh"), "XHigh".to_string()),
        SessionConfigSelectOption::new(SessionConfigValueId::new("max"), "Max".to_string()),
    ];

    vec![SessionConfigOption::select(
        SessionConfigId::new("thinking_effort"),
        "Thinking Effort",
        SessionConfigValueId::new(effort),
        SessionConfigSelectOptions::Ungrouped(thinking_options),
    )
    .category(SessionConfigOptionCategory::ThoughtLevel)]
}
