use std::sync::Arc;

use agent_client_protocol::schema::{
    ClientNotification, ClientRequest, CloseSessionRequest, CloseSessionResponse, ContentBlock,
    ForkSessionRequest, ForkSessionResponse, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, ModelId, ModelInfo, NewSessionRequest,
    NewSessionResponse, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptRequest,
    PromptResponse, SessionConfigId, SessionConfigKind, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelect, SessionConfigSelectOption, SessionConfigValueId,
    SessionId, SessionInfo, SessionMode, SessionModeId, SessionModeState, SessionModelState,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
    SetSessionModelRequest, SetSessionModelResponse, StopReason,
};
use agent_client_protocol::{Client, ConnectionTo, Dispatch, Handled};
use rust_agent_middlewares::prelude::PermissionMode;
use rust_agent_middlewares::tools::{TodoItem, TodoStatus};
use rust_create_agent::agent::events::{AgentEvent as ExecutorEvent, FnEventHandler};
use rust_create_agent::agent::react::AgentInput;
use rust_create_agent::agent::state::AgentState;
use rust_create_agent::agent::AgentCancellationToken;
use rust_create_agent::messages::BaseMessage;
use tokio::sync::OnceCell;

use crate::app::agent::LlmProvider;
use crate::config::ZenConfig;

use super::agent_assembler;
use super::broker::AcpInteractionBroker;
use super::event_mapper;
use super::session::{AcpSession, SessionManager};

static SESSION_MANAGER: OnceCell<SessionManager> = OnceCell::const_new();

/// 初始化全局 SessionManager（必须在 Agent::builder().connect_to() 之前调用）
pub fn init_session_manager(mgr: SessionManager) {
    let _ = SESSION_MANAGER.set(mgr);
}

fn mgr() -> &'static SessionManager {
    SESSION_MANAGER
        .get()
        .expect("SessionManager not initialized")
}

// ─── Helper: 构建 session 元数据 ─────────────────────────────────────────────

fn build_session_mode_state(session: &AcpSession) -> SessionModeState {
    let current = match session.permission_mode.load() {
        PermissionMode::Default => "default",
        PermissionMode::DontAsk => "dontAsk",
        PermissionMode::AcceptEdit => "acceptEdits",
        PermissionMode::AutoMode => "auto",
        PermissionMode::Bypass => "bypass",
    };
    let mut state = SessionModeState::new(
        SessionModeId::new(current),
        vec![
            SessionMode::new(SessionModeId::new("auto"), "Auto")
                .description("LLM classifier decides approval"),
            SessionMode::new(SessionModeId::new("default"), "Default")
                .description("Approval for sensitive tools"),
            SessionMode::new(SessionModeId::new("acceptEdits"), "Accept Edits")
                .description("Allow file edits without approval"),
            SessionMode::new(SessionModeId::new("dontAsk"), "Don't Ask")
                .description("Agent answers only, no tool execution"),
            SessionMode::new(SessionModeId::new("bypass"), "Bypass")
                .description("Full tool access, no approval needed"),
        ],
    );
    let _ = &mut state; // silence non-exhaustive warnings
    state
}

fn build_session_model_state(session: &AcpSession, zen_config: &ZenConfig) -> SessionModelState {
    let provider = zen_config
        .config
        .providers
        .iter()
        .find(|p| p.id == zen_config.config.active_provider_id);
    let current = session.model_alias.clone();
    let mut models = vec![];
    for alias in &["opus", "sonnet", "haiku"] {
        if let Some(name) =
            provider.and_then(|p| p.models.get_model(alias).filter(|m| !m.is_empty()))
        {
            models.push(ModelInfo::new(ModelId::new(*alias), name));
        }
    }
    SessionModelState::new(ModelId::new(current), models)
}

fn build_config_options(
    session: &AcpSession,
) -> Vec<agent_client_protocol::schema::SessionConfigOption> {
    let zen_config = mgr().zen_config();

    // 1. Mode selector
    let current_mode = match session.permission_mode.load() {
        PermissionMode::Default => "default",
        PermissionMode::DontAsk => "dontAsk",
        PermissionMode::AcceptEdit => "acceptEdits",
        PermissionMode::AutoMode => "auto",
        PermissionMode::Bypass => "bypass",
    };
    let mode_option = agent_client_protocol::schema::SessionConfigOption::select(
        SessionConfigId::new("mode"),
        "Mode",
        SessionConfigValueId::new(current_mode),
        vec![
            SessionConfigSelectOption::new(SessionConfigValueId::new("auto"), "Auto"),
            SessionConfigSelectOption::new(SessionConfigValueId::new("default"), "Default"),
            SessionConfigSelectOption::new(
                SessionConfigValueId::new("acceptEdits"),
                "Accept Edits",
            ),
            SessionConfigSelectOption::new(SessionConfigValueId::new("dontAsk"), "Don't Ask"),
            SessionConfigSelectOption::new(SessionConfigValueId::new("bypass"), "Bypass"),
        ],
    )
    .category(SessionConfigOptionCategory::Mode)
    .description("Permission mode for tool execution");

    // 2. Model selector
    let provider = zen_config
        .config
        .providers
        .iter()
        .find(|p| p.id == zen_config.config.active_provider_id);
    let mut model_options = vec![];
    for alias in &["opus", "sonnet", "haiku"] {
        if let Some(name) =
            provider.and_then(|p| p.models.get_model(alias).filter(|m| !m.is_empty()))
        {
            model_options.push(SessionConfigSelectOption::new(
                SessionConfigValueId::new(*alias),
                name,
            ));
        }
    }
    let model_option = agent_client_protocol::schema::SessionConfigOption::select(
        SessionConfigId::new("model"),
        "Model",
        SessionConfigValueId::new(session.model_alias.as_str()),
        model_options,
    )
    .category(SessionConfigOptionCategory::Model)
    .description("AI model for this session");

    // 3. Thinking effort selector
    let effort_val = session
        .thinking
        .as_ref()
        .map(|t| t.effort.as_str())
        .unwrap_or("high");
    let thinking_option = agent_client_protocol::schema::SessionConfigOption::new(
        SessionConfigId::new("thinking_effort"),
        "Thinking Effort",
        SessionConfigKind::Select(SessionConfigSelect::new(
            SessionConfigValueId::new(effort_val),
            vec![
                SessionConfigSelectOption::new(SessionConfigValueId::new("low"), "Low"),
                SessionConfigSelectOption::new(SessionConfigValueId::new("medium"), "Medium"),
                SessionConfigSelectOption::new(SessionConfigValueId::new("high"), "High"),
            ],
        )),
    )
    .category(SessionConfigOptionCategory::ThoughtLevel)
    .description("Controls reasoning depth");

    vec![mode_option, model_option, thinking_option]
}

/// 填充 NewSessionResponse 元数据
fn fill_new_session_resp(session: &AcpSession, mut resp: NewSessionResponse) -> NewSessionResponse {
    resp = resp.modes(Some(build_session_mode_state(session)));
    resp = resp.config_options(Some(build_config_options(session)));
    resp = resp.models(Some(build_session_model_state(session, mgr().zen_config())));
    resp
}

/// 填充 LoadSessionResponse 元数据
fn fill_load_session_resp(
    session: &AcpSession,
    mut resp: LoadSessionResponse,
) -> LoadSessionResponse {
    resp = resp.modes(Some(build_session_mode_state(session)));
    resp = resp.config_options(Some(build_config_options(session)));
    resp = resp.models(Some(build_session_model_state(session, mgr().zen_config())));
    resp
}

// ─── session/new handler ─────────────────────────────────────────────────────

pub async fn handle_new_session(
    req: NewSessionRequest,
    responder: agent_client_protocol::Responder<NewSessionResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let cwd = req.cwd.to_string_lossy().to_string();

    match mgr().new_session(&cwd).await {
        Ok((session_id, _thread_id)) => {
            let resp = mgr()
                .get_session(&session_id)
                .map(|s| fill_new_session_resp(&s, NewSessionResponse::new(session_id.clone())))
                .unwrap_or_else(|| NewSessionResponse::new(session_id.clone()));

            tracing::info!(
                session_id = %session_id,
                response = %serde_json::to_string(&resp).unwrap_or_default(),
                "ACP session/new response"
            );

            let _ = responder.respond(resp);
        }
        Err(e) => {
            tracing::error!("Failed to create session: {e}");
            let _ = responder.respond(NewSessionResponse::new(""));
        }
    }
    Ok(())
}

// ─── session/close handler ────────────────────────────────────────────────────

pub async fn handle_close_session(
    req: CloseSessionRequest,
    responder: agent_client_protocol::Responder<CloseSessionResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let session_id = req.session_id.0.as_ref();
    let _ = mgr().close_session(session_id).await;
    let _ = responder.respond(CloseSessionResponse::default());
    tracing::info!(session_id = %session_id, "ACP session closed");
    Ok(())
}

// ─── session/list handler ────────────────────────────────────────────────────

pub async fn handle_list_sessions(
    req: ListSessionsRequest,
    responder: agent_client_protocol::Responder<ListSessionsResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let cwd_filter = req.cwd.as_ref().map(|p| p.to_string_lossy().to_string());

    match mgr().list_sessions().await {
        Ok(threads) => {
            let sessions: Vec<SessionInfo> = threads
                .into_iter()
                .filter(|t| cwd_filter.as_ref().is_none_or(|cwd| t.cwd == *cwd))
                .map(|t| {
                    SessionInfo::new(SessionId::from(t.id), &t.cwd)
                        .title(t.title.unwrap_or_default())
                        .updated_at(t.updated_at.to_rfc3339())
                })
                .collect();
            let _ = responder.respond(ListSessionsResponse::new(sessions));
        }
        Err(e) => {
            tracing::error!("Failed to list sessions: {e}");
            let _ = responder.respond(ListSessionsResponse::new(vec![]));
        }
    }
    Ok(())
}

// ─── session/prompt handler ──────────────────────────────────────────────────

pub async fn handle_prompt(
    req: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
    conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let session_id_str = req.session_id.0.clone();
    let session_id_acp = req.session_id.clone();

    // 从 prompt 中提取文本
    let user_text: String = req
        .prompt
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text(tc) = block {
                Some(tc.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if user_text.is_empty() {
        let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
        return Ok(());
    }

    // 获取 session 元数据
    let (
        thread_id,
        cwd,
        cancel_token,
        session_model_alias,
        session_permission_mode,
        _session_thinking,
    ) = {
        match mgr().get_session(&session_id_str) {
            Some(s) => (
                s.thread_id.clone(),
                s.cwd.clone(),
                s.cancel_token.clone(),
                s.model_alias.clone(),
                s.permission_mode.clone(),
                s.thinking.clone(),
            ),
            None => {
                tracing::warn!(session_id = %session_id_str, "Session not found for prompt");
                let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
                return Ok(());
            }
        }
    };

    tracing::info!(session_id = %session_id_str, text_len = user_text.len(), "ACP prompt received");

    // 从 session 级 model_alias 构建 LlmProvider
    let provider = LlmProvider::from_config_for_alias(mgr().zen_config(), &session_model_alias)
        .unwrap_or_else(|| mgr().provider().clone());

    let mgr_zen_config = mgr().zen_config().clone();
    let mgr_thread_store = mgr().thread_store().clone();

    // 将 Responder 和 conn 移入 spawned task，避免阻塞事件循环
    tokio::spawn(async move {
        // 加载线程历史
        let history = match mgr().load_thread_messages(&thread_id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "Failed to load thread history");
                let _ = responder.respond(PromptResponse::new(StopReason::EndTurn));
                return;
            }
        };

        // 构建系统提示词
        let features = crate::prompt::PromptFeatures::detect();
        let system_prompt = crate::prompt::build_system_prompt(None, &cwd, features);

        // 创建 CancellationToken（关联 session cancel_token）
        let cancel = AgentCancellationToken::new();
        let cancel_for_link = cancel.clone();
        let cancel_token_for_link = cancel_token.clone();
        tokio::spawn(async move {
            cancel_token_for_link.cancelled().await;
            cancel_for_link.cancel();
        });

        // 事件处理器：ExecutorEvent → SessionUpdate → SessionNotification → conn.send_notification()
        let conn_for_handler = conn.clone();
        let sid_for_handler = session_id_acp.clone();
        let handler: Arc<dyn rust_create_agent::agent::events::AgentEventHandler> =
            Arc::new(FnEventHandler(move |event: ExecutorEvent| {
                let updates = event_mapper::map_executor_to_updates(&event);
                for update in updates {
                    let notif = SessionNotification::new(sid_for_handler.clone(), update);
                    let _ = conn_for_handler.send_notification(notif);
                }
            }));

        // 创建 ACP 权限桥接 broker + 权限转发循环
        let (perm_tx, perm_rx) = tokio::sync::mpsc::channel(16);
        let broker = Arc::new(AcpInteractionBroker::new(perm_tx));

        // 权限转发：perm_rx → RequestPermissionRequest → conn.send_request() → map → response_tx
        let conn_for_perm = conn.clone();
        let sid_for_perm = session_id_acp.clone();
        tokio::spawn(async move {
            super::broker::permission_forwarding_loop(perm_rx, conn_for_perm, sid_for_perm).await;
        });

        // 组装 Agent
        let config = agent_assembler::AgentAssembleConfig {
            provider,
            cwd: cwd.clone(),
            system_prompt,
            broker,
            permission_mode: session_permission_mode,
            zen_config: mgr_zen_config,
            preload_skills: vec![],
            event_handler: handler,
            cancel: cancel.clone(),
            cron_scheduler: None,
        };
        let (executor, mut todo_rx) = agent_assembler::assemble_agent(config);

        // 转发 Todo 更新为 SessionUpdate
        let conn_for_todo = conn.clone();
        let sid_for_todo = session_id_acp.clone();
        tokio::spawn(async move {
            while let Some(todos) = todo_rx.recv().await {
                let entries: Vec<_> = todos
                    .iter()
                    .map(|t: &TodoItem| {
                        PlanEntry::new(
                            t.content.clone(),
                            PlanEntryPriority::Medium,
                            match t.status {
                                TodoStatus::Completed => PlanEntryStatus::Completed,
                                TodoStatus::InProgress => PlanEntryStatus::InProgress,
                                TodoStatus::Pending => PlanEntryStatus::Pending,
                            },
                        )
                    })
                    .collect();
                let notif = SessionNotification::new(
                    sid_for_todo.clone(),
                    SessionUpdate::Plan(Plan::new(entries)),
                );
                let _ = conn_for_todo.send_notification(notif);
            }
        });

        // 创建 AgentState（带历史 + 持久化）
        let history_len = history.len();
        let mut state =
            AgentState::with_messages(cwd, history).with_persistence(mgr_thread_store, thread_id);

        let input = AgentInput::text(user_text);
        let result = executor.execute(input, &mut state, Some(cancel)).await;

        // new_msgs 通过 AgentState 的 with_persistence 已自动持久化
        tracing::info!(
            new_msgs = state.into_messages().len().saturating_sub(history_len),
            "ACP prompt execution finished"
        );

        let stop_reason = match &result {
            Ok(_) => StopReason::EndTurn,
            Err(rust_create_agent::error::AgentError::Interrupted) => StopReason::Cancelled,
            Err(e) => {
                tracing::error!(error = %e, "ACP prompt execution error");
                StopReason::EndTurn
            }
        };

        let _ = responder.respond(PromptResponse::new(stop_reason));
    });

    Ok(())
}

// ─── session/load handler ────────────────────────────────────────────────────

pub async fn handle_load_session(
    req: LoadSessionRequest,
    responder: agent_client_protocol::Responder<LoadSessionResponse>,
    conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let thread_id_str = req.session_id.0.as_ref().to_string();
    let cwd = req.cwd.to_string_lossy().to_string();
    let session_id_acp = req.session_id.clone();

    tracing::info!(thread_id = %thread_id_str, "ACP session/load request");

    // 加载线程历史
    let thread_id = rust_create_agent::thread::ThreadId::from(thread_id_str.clone());
    let messages = match mgr().load_thread_messages(&thread_id).await {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::error!(error = %e, "Failed to load session");
            let _ = responder.respond(LoadSessionResponse::new());
            return Ok(());
        }
    };

    // 创建 AcpSession 注册到 SessionManager
    let _ = mgr().new_session_with_id(&thread_id_str, &cwd).await;

    // 回放历史消息为 SessionNotification
    for msg in &messages {
        let updates = map_message_to_updates(msg);
        for update in updates {
            let notif = SessionNotification::new(session_id_acp.clone(), update);
            let _ = conn.send_notification(notif);
        }
    }

    tracing::info!(
        msg_count = messages.len(),
        "ACP session loaded and replayed"
    );

    let resp = mgr()
        .get_session(&thread_id_str)
        .map(|s| fill_load_session_resp(&s, LoadSessionResponse::new()))
        .unwrap_or_default();

    let _ = responder.respond(resp);
    Ok(())
}

// ─── session/resume handler ──────────────────────────────────────────────────

pub async fn handle_resume_session(
    req: agent_client_protocol::schema::ResumeSessionRequest,
    responder: agent_client_protocol::Responder<
        agent_client_protocol::schema::ResumeSessionResponse,
    >,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let thread_id_str = req.session_id.0.as_ref().to_string();
    let cwd = req.cwd.to_string_lossy().to_string();

    tracing::info!(thread_id = %thread_id_str, "ACP session/resume request");

    // 创建 AcpSession 注册到 SessionManager（不回放消息）
    let _ = mgr().new_session_with_id(&thread_id_str, &cwd).await;

    let resp = mgr()
        .get_session(&thread_id_str)
        .map(|s| {
            let mut resp = agent_client_protocol::schema::ResumeSessionResponse::default();
            resp = resp.modes(Some(build_session_mode_state(&s)));
            resp = resp.config_options(Some(build_config_options(&s)));
            resp = resp.models(Some(build_session_model_state(&s, mgr().zen_config())));
            resp
        })
        .unwrap_or_default();

    let _ = responder.respond(resp);
    Ok(())
}

// ─── session/set_mode handler ────────────────────────────────────────────────

pub async fn handle_set_mode(
    req: SetSessionModeRequest,
    responder: agent_client_protocol::Responder<SetSessionModeResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let mode_id = req.mode_id.0.as_ref();
    let session_id = req.session_id.0.as_ref();

    let mode = match mode_id {
        "bypass" => PermissionMode::Bypass,
        "default" => PermissionMode::Default,
        "acceptEdits" => PermissionMode::AcceptEdit,
        "dontAsk" => PermissionMode::DontAsk,
        "auto" => PermissionMode::AutoMode,
        other => {
            tracing::warn!(mode_id = other, "Unknown mode, ignoring");
            let _ = responder.respond(SetSessionModeResponse::default());
            return Ok(());
        }
    };

    if let Some(session) = mgr().get_session(session_id) {
        session.permission_mode.store(mode);
        tracing::info!(session_id, mode_id, "Session mode changed");
    } else {
        tracing::warn!(session_id, "Session not found for set_mode");
    }

    let _ = responder.respond(SetSessionModeResponse::default());
    Ok(())
}

// ─── session/set_model handler ────────────────────────────────────────────────

pub async fn handle_set_model(
    req: SetSessionModelRequest,
    responder: agent_client_protocol::Responder<SetSessionModelResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let model_id = req.model_id.0.as_ref().to_string();
    let session_id_str = req.session_id.0.as_ref();

    if let Some(mut session) = mgr().inner_sessions().get_mut(session_id_str) {
        session.model_alias = model_id.clone();
        tracing::info!(
            session_id = session_id_str,
            model_id,
            "Session model changed"
        );
    } else {
        tracing::warn!(
            session_id = session_id_str,
            "Session not found for set_model"
        );
    }

    let _ = responder.respond(SetSessionModelResponse::default());
    Ok(())
}

// ─── session/set_config_option handler ───────────────────────────────────────

pub async fn handle_set_config_option(
    req: SetSessionConfigOptionRequest,
    responder: agent_client_protocol::Responder<SetSessionConfigOptionResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let session_id = req.session_id.0.as_ref();
    let config_id = req.config_id.0.as_ref();

    // 提取 value
    let value_id = match &req.value {
        SessionConfigOptionValue::ValueId { value } => value.0.as_ref(),
        _ => {
            let _ = responder.respond(SetSessionConfigOptionResponse::new(vec![]));
            return Ok(());
        }
    };

    if let Some(mut session) = mgr().inner_sessions().get_mut(session_id) {
        match config_id {
            "mode" => {
                let mode = match value_id {
                    "bypass" => PermissionMode::Bypass,
                    "default" => PermissionMode::Default,
                    "acceptEdits" => PermissionMode::AcceptEdit,
                    "dontAsk" => PermissionMode::DontAsk,
                    "auto" => PermissionMode::AutoMode,
                    other => {
                        tracing::warn!(mode_id = other, "Unknown mode in config_option");
                        drop(session);
                        let _ = responder.respond(SetSessionConfigOptionResponse::new(vec![]));
                        return Ok(());
                    }
                };
                session.permission_mode.store(mode);
                tracing::info!(
                    session_id,
                    mode_id = value_id,
                    "Session mode changed via config_option"
                );
            }
            "model" => {
                session.model_alias = value_id.to_string();
                tracing::info!(
                    session_id,
                    model_id = value_id,
                    "Session model changed via config_option"
                );
            }
            "thinking_effort" => {
                let thinking =
                    session
                        .thinking
                        .get_or_insert_with(|| crate::config::ThinkingConfig {
                            enabled: true,
                            budget_tokens: 8000,
                            effort: "high".to_string(),
                        });
                thinking.effort = value_id.to_string();
                tracing::info!(session_id, effort = value_id, "Thinking effort changed");
            }
            other => {
                tracing::warn!(config_id = other, "Unknown config option");
                drop(session);
                let _ = responder.respond(SetSessionConfigOptionResponse::new(vec![]));
                return Ok(());
            }
        }
    }

    // 返回更新后的 config_options
    let config_options = mgr()
        .get_session(session_id)
        .map(|s| build_config_options(&s))
        .unwrap_or_default();
    let _ = responder.respond(SetSessionConfigOptionResponse::new(config_options));
    Ok(())
}

// ─── session/fork handler ────────────────────────────────────────────────────

pub async fn handle_fork_session(
    req: ForkSessionRequest,
    responder: agent_client_protocol::Responder<ForkSessionResponse>,
    _conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let parent_id = req.session_id.0.as_ref();
    let cwd = req.cwd.to_string_lossy().to_string();

    // 从父 session 继承设置
    let (model_alias, thinking) = mgr()
        .get_session(parent_id)
        .map(|s| (s.model_alias.clone(), s.thinking.clone()))
        .unwrap_or_else(|| {
            (
                mgr().zen_config().config.active_alias.clone(),
                mgr().zen_config().config.thinking.clone(),
            )
        });

    // 创建新 session
    match mgr()
        .new_session_with_settings(&cwd, model_alias, thinking)
        .await
    {
        Ok((new_session_id, _)) => {
            let resp = mgr()
                .get_session(&new_session_id)
                .map(|s| {
                    let mut resp = ForkSessionResponse::new(new_session_id.clone());
                    resp = resp.modes(Some(build_session_mode_state(&s)));
                    resp = resp.config_options(Some(build_config_options(&s)));
                    resp = resp.models(Some(build_session_model_state(&s, mgr().zen_config())));
                    resp
                })
                .unwrap_or_else(|| ForkSessionResponse::new(new_session_id.clone()));

            let _ = responder.respond(resp);
            tracing::info!(parent_id, new_session_id = %new_session_id, "Session forked");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to fork session");
            let _ = responder.respond(ForkSessionResponse::new(""));
        }
    }

    Ok(())
}

/// 将持久化的 BaseMessage 映射为 SessionUpdate（用于 session/load 回放）
fn map_message_to_updates(msg: &BaseMessage) -> Vec<SessionUpdate> {
    use agent_client_protocol::schema::{Content, ContentChunk, TextContent};

    match msg {
        BaseMessage::Human { content, .. } => {
            let text = content.text_content();
            vec![SessionUpdate::UserMessageChunk(ContentChunk::new(
                ContentBlock::Text(TextContent::new(text)),
            ))]
        }
        BaseMessage::Ai {
            content,
            tool_calls,
            ..
        } => {
            let mut updates = Vec::new();

            // AI 文本消息
            let text = content.text_content();
            if !text.is_empty() {
                updates.push(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::Text(TextContent::new(text)),
                )));
            }

            // 工具调用
            for tc in tool_calls {
                use agent_client_protocol::schema::{ToolCall, ToolCallContent, ToolCallStatus};
                updates.push(SessionUpdate::ToolCall(
                    ToolCall::new(tc.id.clone(), tc.name.clone())
                        .status(ToolCallStatus::Completed)
                        .content(vec![ToolCallContent::Content(Content::new(
                            ContentBlock::Text(TextContent::new(truncate_str(
                                &tc.arguments.to_string(),
                                500,
                            ))),
                        ))]),
                ));
            }

            updates
        }
        BaseMessage::Tool {
            content,
            tool_call_id,
            is_error,
            ..
        } => {
            use agent_client_protocol::schema::{
                ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
            };
            vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                tool_call_id.clone(),
                ToolCallUpdateFields::new()
                    .status(if *is_error {
                        ToolCallStatus::Failed
                    } else {
                        ToolCallStatus::Completed
                    })
                    .content(vec![ToolCallContent::Content(Content::new(
                        ContentBlock::Text(TextContent::new(truncate_str(
                            &content.text_content(),
                            500,
                        ))),
                    ))]),
            ))]
        }
        _ => vec![],
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s.floor_char_boundary(max_len);
        format!("{}...", &s[..boundary])
    }
}

// ─── dispatch handler（通知 + 未匹配请求）────────────────────────────────────

pub async fn handle_dispatch(
    msg: Dispatch<ClientRequest, ClientNotification>,
    _conn: ConnectionTo<Client>,
) -> Result<Handled<Dispatch<ClientRequest, ClientNotification>>, agent_client_protocol::Error> {
    match msg {
        Dispatch::Notification(notif) => match notif {
            ClientNotification::CancelNotification(cancel) => {
                let session_id = cancel.session_id.0.as_ref();
                mgr().cancel_session(session_id);
                tracing::info!(session_id = %session_id, "ACP session cancelled");
                Ok(Handled::Yes)
            }
            _ => Ok(Handled::Yes),
        },
        // 未匹配的请求传递给下一个 handler
        other => Ok(Handled::No {
            message: other,
            retry: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_response_serialization() {
        let modes = SessionModeState::new(
            SessionModeId::new("auto"),
            vec![
                SessionMode::new(SessionModeId::new("auto"), "Auto"),
                SessionMode::new(SessionModeId::new("default"), "Default"),
            ],
        );

        let resp = NewSessionResponse::new("test-session-1")
            .modes(Some(modes))
            .config_options(Some(vec![
                agent_client_protocol::schema::SessionConfigOption::new(
                    SessionConfigId::new("thinking_effort"),
                    "Thinking Effort",
                    SessionConfigKind::Select(SessionConfigSelect::new(
                        SessionConfigValueId::new("high"),
                        vec![
                            SessionConfigSelectOption::new(SessionConfigValueId::new("low"), "Low"),
                            SessionConfigSelectOption::new(
                                SessionConfigValueId::new("high"),
                                "High",
                            ),
                        ],
                    )),
                ),
            ]));

        let json = serde_json::to_string_pretty(&resp).unwrap();
        eprintln!("NewSessionResponse JSON:\n{}", json);

        assert!(
            json.contains("\"modes\""),
            "modes field should be present in JSON"
        );
        assert!(
            json.contains("\"currentModeId\""),
            "currentModeId should be present"
        );
        assert!(
            json.contains("\"availableModes\""),
            "availableModes should be present"
        );
        assert!(
            json.contains("\"configOptions\""),
            "configOptions should be present"
        );
    }

    #[test]
    fn test_session_model_state_serialization() {
        let state = SessionModelState::new(
            ModelId::new("sonnet"),
            vec![
                ModelInfo::new(ModelId::new("opus"), "Claude Opus"),
                ModelInfo::new(ModelId::new("sonnet"), "Claude Sonnet"),
            ],
        );

        let resp = NewSessionResponse::new("test-session-1").models(Some(state));

        let json = serde_json::to_string_pretty(&resp).unwrap();
        eprintln!("NewSessionResponse with models JSON:\n{}", json);

        assert!(
            json.contains("\"models\""),
            "models field should be present in JSON"
        );
        assert!(
            json.contains("\"currentModelId\""),
            "currentModelId should be present"
        );
    }
}
