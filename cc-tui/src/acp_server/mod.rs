//! ACP Server — transport-agnostic request handler.
//!
//! Accepts any [`AcpTransport`] implementation (mpsc for TUI, stdio for IDE),
//! builds and executes ReAct agents, and pushes [`SessionUpdate`] notifications
//! back through the transport.
//!
//! **Cancel architecture**: `session/prompt` execution is spawned into a
//! background tokio task so the main server loop remains responsive to
//! `session/cancel` notifications. Sessions are shared via
//! `Arc<tokio::sync::Mutex<HashMap>>`.

use std::{collections::HashMap, sync::Arc};

pub use cc_acp::session::state_builders::{
    apply_thinking_effort, build_config_options, build_mode_state, build_model_state,
    parse_permission_mode,
};
use cc_acp::transport::types::{AcpError, IncomingMessage};
use cc_agent::{agent::AgentCancellationToken, interaction::ChannelState, messages::BaseMessage};
use cc_middlewares::prelude::*;
use serde_json::{json, Value};

use crate::{app::agent::LlmProvider, config::PeriConfig};

mod notify;
mod prompt;
mod requests;

pub(crate) use notify::{extract_session_id, handle_notification, send_session_info_update};
pub(crate) use prompt::execute_prompt;
pub(crate) use requests::handle_request;

/// MCP-over-ACP (unstable, #25)：Agent → Client 方向。
/// Agent 需调用客户端托管的 MCP 服务器时，经 ACP 通道把 JSON-RPC 消息
/// 发给客户端，由客户端中继给实际的 MCP 服务器并返回响应。
#[allow(dead_code)] // 预留给 Agent 侧 MCP 工具调用集成
pub(crate) async fn send_mcp_message(
    transport: &dyn cc_acp::transport::AcpTransport,
    session_id: &str,
    server_name: &str,
    message: Value,
) -> Result<Value, cc_acp::transport::types::AcpError> {
    transport
        .send_request(
            "mcp/message",
            json!({
                "sessionId": session_id,
                "serverName": server_name,
                "message": message,
            }),
        )
        .await
}

// ── Session state ────────────────────────────────────────────────────────────

pub(crate) struct SessionState {
    #[allow(dead_code)] // session 标识字段，保留供调试
    session_id: String,
    thread_id: String,
    cwd: String,
    history: Vec<BaseMessage>,
    cancel_token: Option<AgentCancellationToken>,
    steering: Option<cc_agent::agent::steering::SteeringQueue>,
    // ── Frozen session data (populated at creation, immutable thereafter) ──
    pub(crate) frozen: Option<cc_acp::session::executor::FrozenSessionData>,
    /// Recall items from previous turn (injected as <system-reminder> in next user message).
    pub(crate) recall_items: Vec<String>,
    /// Session-scoped agent component pool for reusing heavy objects across prompts.
    pub(crate) agent_pool: cc_acp::session::agent_pool::AgentPool,
    /// 会话级审批记忆（路径级）：用户在弹窗选「本次会话同意」后，
    /// 同一 (工具, 路径) 后续免问。随会话销毁自动丢弃。
    pub(crate) approval_memory: Arc<cc_middlewares::hitl::ApprovalMemory>,
    /// MCP-over-ACP：客户端（IDE）托管的 MCP 服务器列表（#25）。
    /// key 为 server name，value 为 connect 时客户端声明的描述信息。
    pub(crate) mcp_over_acp_servers: HashMap<String, Value>,
}

// ── Server config ────────────────────────────────────────────────────────────

/// All cross-session configuration needed by the ACP server.
pub struct AcpServerConfig {
    pub provider: Arc<parking_lot::RwLock<LlmProvider>>,
    pub peri_config: Arc<parking_lot::RwLock<PeriConfig>>,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    pub mcp_pool: Option<Arc<cc_middlewares::mcp::McpClientPool>>,
    pub channel_state: Option<Arc<ChannelState>>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub plugin_hooks: Vec<cc_middlewares::hooks::RegisteredHook>,
    pub hook_groups: Vec<Vec<cc_middlewares::hooks::RegisteredHook>>,
    pub plugin_lsp_servers: Vec<cc_lsp::config::LspServerConfig>,
    pub tool_search_index: Arc<cc_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools:
        Arc<parking_lot::RwLock<HashMap<String, Arc<dyn cc_agent::tools::BaseTool>>>>,
    pub thread_store: Arc<dyn cc_agent::thread::ThreadStore>,
    pub langfuse_session: Option<Arc<cc_acp::langfuse::LangfuseSession>>,
    pub config_path: std::path::PathBuf,
    /// Shell 执行器（注入 BashTool，支持 Ctrl+B 后台化）。
    /// None = 使用默认 InlineShellExecutor（保持原 cmd.output() 同步行为）。
    pub shell_executor: Option<Arc<dyn cc_agent::shell::ShellExecutor>>,
}

// ── Main server loop ────────────────────────────────────────────────────────

type SharedSessions = Arc<tokio::sync::Mutex<HashMap<String, SessionState>>>;

/// Main ACP server loop. Accepts any `AcpTransport` (mpsc for TUI, stdio for IDE).
///
/// `session/prompt` is spawned into a background task so the loop stays
/// responsive to `session/cancel` and other incoming messages.
pub async fn run_acp_server(
    transport: Arc<dyn cc_acp::transport::AcpTransport>,
    cfg: AcpServerConfig,
) {
    let sessions: SharedSessions = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    // Per-session prompt serialization lock: ensures that when a prompt completes
    // (state.history updated) the next prompt for the same session sees the updated history.
    let prompt_locks: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    while let Some(msg) = transport.recv().await {
        match msg {
            IncomingMessage::Request { id, method, params } => {
                if method == "session/prompt" {
                    // Spawn long-running prompt execution so the server loop
                    // continues processing session/cancel notifications.
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
                    let channel_state = cfg.channel_state.clone();
                    let tool_search_index = cfg.tool_search_index.clone();
                    let shared_tools = cfg.shared_tools.clone();
                    let plugin_lsp_servers = cfg.plugin_lsp_servers.clone();
                    let thread_store = cfg.thread_store.clone();
                    let prompt_session_id = extract_session_id(&params, "").to_string();
                    let shell_executor = cfg.shell_executor.clone();
                    let langfuse_session = cfg.langfuse_session.clone();

                    // Extract AgentPool from session, wrap in Arc<Mutex> for
                    // in-place modification inside executor.
                    let pool_arc = {
                        let mut sessions = sessions.lock().await;
                        let pool = sessions
                            .get_mut(&prompt_session_id)
                            .map(|s| {
                                std::mem::replace(
                                    &mut s.agent_pool,
                                    cc_acp::session::agent_pool::AgentPool::new(),
                                )
                            })
                            .unwrap_or_default();
                        Arc::new(parking_lot::Mutex::new(pool))
                    };

                    let prompt_lock = {
                        let mut locks = prompt_locks.lock().await;
                        locks
                            .entry(prompt_session_id.clone())
                            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                            .clone()
                    };

                    tokio::spawn(async move {
                        // Serialize prompts per session: wait for any in-flight prompt to finish
                        // so that state.history is up-to-date when this prompt reads it.
                        let _guard = prompt_lock.lock().await;
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
                            channel_state,
                            tool_search_index,
                            shared_tools,
                            &plugin_lsp_servers,
                            &transport,
                            &thread_store,
                            langfuse_session,
                            pool_arc.clone(),
                            shell_executor,
                        )
                        .await;

                        // Restore AgentPool back into session
                        if let Ok(mutex) = Arc::try_unwrap(pool_arc) {
                            let mut sessions = sessions.lock().await;
                            if let Some(state) = sessions.get_mut(&prompt_session_id) {
                                state.agent_pool = mutex.into_inner();
                            }
                        }

                        let _ = transport.send_response(id, result).await;
                        if !prompt_session_id.is_empty() {
                            send_session_info_update(transport.as_ref(), &prompt_session_id).await;
                        }
                    });
                } else if method == "peri/session/steer" {
                    // 等待消费确认不能持有 sessions 锁或阻塞取消/审批请求。
                    let receipt = {
                        let sessions = sessions.lock().await;
                        let session_id = extract_session_id(&params, "");
                        let queue = sessions
                            .get(session_id)
                            .and_then(|state| state.steering.as_ref());
                        cc_acp::session::steering::enqueue(queue, &params)
                    };
                    let transport = Arc::clone(&transport);
                    tokio::spawn(async move {
                        let result = cc_acp::session::steering::confirm(receipt).await;
                        let _ = transport.send_response(id, result).await;
                    });
                } else if method == "session/append_history" {
                    // 静默追加合成消息到会话 history（不触发推理轮次）。
                    // 用于前台 `!` 命令结果回流：TUI 侧 fire-and-forget 调用，
                    // 服务端存入 history 供下一次 prompt 使用。
                    //
                    // 与 session/prompt 共用 prompt_locks 串行：prompt 结束时以整体赋值
                    // 回写 history（prompt.rs:187），若并发写入会被覆盖丢失。
                    let sid = extract_session_id(&params, "").to_string();
                    let lock = {
                        let mut locks = prompt_locks.lock().await;
                        locks
                            .entry(sid.clone())
                            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                            .clone()
                    };
                    let sessions = Arc::clone(&sessions);
                    let transport = Arc::clone(&transport);
                    let thread_store = cfg.thread_store.clone();
                    tokio::spawn(async move {
                        let _guard = lock.lock().await;
                        let result = append_history_to_session(
                            &sid,
                            &params,
                            &sessions,
                            thread_store.as_ref(),
                        )
                        .await;
                        let _ = transport.send_response(id, result).await;
                    });
                } else {
                    let mut sessions = sessions.lock().await;
                    let result =
                        handle_request(&method, &params, &cfg, &mut sessions, transport.as_ref())
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

/// 处理 `session/append_history`：把合成消息追加到目标会话的 history。
///
/// 用于前台 `!` 命令结果回流（`shell_context_messages` 产出 caveat + 片段），
/// **不触发推理轮次** —— 仅写入 history，下一次 `session/prompt` 时模型可见。
///
/// 调用方须持有该 session 的 `prompt_lock`，与 `session/prompt` 串行（见调用点注释）。
async fn append_history_to_session(
    session_id: &str,
    params: &serde_json::Value,
    sessions: &SharedSessions,
    thread_store: &dyn cc_agent::thread::ThreadStore,
) -> Result<serde_json::Value, AcpError> {
    let messages: Vec<BaseMessage> = params
        .get("messages")
        .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
        .unwrap_or_default();

    if messages.is_empty() {
        return Err(AcpError::new(-32602, "missing messages"));
    }

    let mut guard = sessions.lock().await;
    // session 不存在（可能已关闭）→ 静默忽略，避免 TUI 侧产生噪音错误
    if let Some(state) = guard.get_mut(session_id) {
        state.history.extend(messages.iter().cloned());
        let tid = cc_agent::thread::ThreadId::from(state.thread_id.clone());
        if let Err(e) = thread_store.append_messages(&tid, &messages).await {
            tracing::warn!(error = %e, "append_history: 落盘失败（内存已更新）");
        }
    } else {
        tracing::debug!(session_id, "append_history: session 不存在，忽略");
    }

    Ok(json!({ "accepted": true }))
}
