//! ACP Stdio 模式：通过 stdin/stdout JSON-RPC 与 IDE client 通信

use std::sync::Arc;

// ─── ACP 协议类型（别名与 peri 内部同名类型区分）─────────────────────────
use agent_client_protocol::schema::{
    ContentBlock as AcpBlock, ContentChunk, PermissionOption, PermissionOptionKind,
    RequestPermissionOutcome, RequestPermissionRequest, SessionId as AcpSessionId,
    SessionNotification, SessionUpdate, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields,
};
use agent_client_protocol::{Client, ConnectionTo};
use cc_agent::messages::{BaseMessage, ContentBlock as PeriContentBlock, MessageContent};

// ─── ACP Stdio 类型 ──────────────────────────────────────────────────────

struct SessionInfo {
    #[allow(dead_code)] // session 标识字段，保留供调试
    session_id: String,
    thread_id: String,
    cwd: String,
    history: Vec<cc_agent::messages::BaseMessage>,
    cancel_token: Option<cc_agent::agent::AgentCancellationToken>,
    /// Frozen session data (built once at session/new).
    frozen: Option<cc_acp::session::executor::FrozenSessionData>,
    /// Session-scoped agent pool for LLM instance reuse.
    agent_pool: cc_acp::session::agent_pool::AgentPool,
}

struct StdioContext {
    provider: parking_lot::RwLock<cc_tui::app::agent::LlmProvider>,
    peri_config: parking_lot::RwLock<cc_tui::config::PeriConfig>,
    permission_mode: Arc<cc_middlewares::prelude::SharedPermissionMode>,
    cron_scheduler: Arc<parking_lot::Mutex<cc_middlewares::cron::CronScheduler>>,
    mcp_pool: Option<Arc<cc_middlewares::mcp::McpClientPool>>,
    channel_state: Option<Arc<cc_agent::interaction::ChannelState>>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<cc_middlewares::hooks::RegisteredHook>>,
    plugin_lsp_servers: Vec<cc_lsp::config::LspServerConfig>,
    tool_search_index: Arc<cc_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn cc_agent::tools::BaseTool>>,
        >,
    >,
    sessions: parking_lot::RwLock<std::collections::HashMap<String, SessionInfo>>,
    thread_store: Arc<dyn cc_agent::thread::ThreadStore>,
    langfuse_session: Option<Arc<cc_acp::langfuse::LangfuseSession>>,
}

fn apply_stdio_model_selection(
    ctx: &StdioContext,
    model_id: &str,
) -> Option<cc_tui::app::agent::LlmProvider> {
    let (provider_id, alias) = cc_acp::provider::parse_model_selection_value(model_id);
    {
        let mut cfg = ctx.peri_config.write();
        if let Some(provider_id) = provider_id {
            if cfg.config.providers.iter().any(|p| p.id == provider_id) {
                cfg.config.active_provider_id = provider_id.to_string();
            } else {
                tracing::warn!(
                    provider_id = %provider_id,
                    model_id = %model_id,
                    "Model selection provider not found"
                );
            }
        }
        cfg.config.active_alias = alias.to_string();
    }

    let cfg = ctx.peri_config.read();
    cc_tui::app::agent::LlmProvider::from_config(&cfg)
}

/// ACP stdio 模式的权限 broker：把工具审批请求转发给 IDE 客户端
/// （`session/request_permission`），由用户在编辑器里点允许/拒绝。
///
/// 替代之前的 `StdioBroker`（直接放行所有请求）：fail-open 的默认行为让
/// ACP 客户端永远收不到审批请求，HITL 名存实亡。
struct AcpPermissionBroker {
    /// 到 IDE 客户端的连接（发 `session/request_permission` 用）
    cx: ConnectionTo<Client>,
    /// 当前 ACP 会话 id
    session_id: AcpSessionId,
}

impl AcpPermissionBroker {
    /// 审批选项 id（对外稳定：客户端回传时按此匹配）
    const ALLOW_ONCE: &'static str = "allow-once";
    const ALLOW_ALWAYS: &'static str = "allow-always";
    const REJECT_ONCE: &'static str = "reject-once";
    const REJECT_ALWAYS: &'static str = "reject-always";

    fn new(cx: ConnectionTo<Client>, session_id: AcpSessionId) -> Self {
        Self { cx, session_id }
    }

    /// 发给客户端的四个标准选项
    fn permission_options() -> Vec<PermissionOption> {
        vec![
            PermissionOption::new(Self::ALLOW_ONCE, "Allow once", PermissionOptionKind::AllowOnce),
            PermissionOption::new(
                Self::ALLOW_ALWAYS,
                "Always allow",
                PermissionOptionKind::AllowAlways,
            ),
            PermissionOption::new(
                Self::REJECT_ONCE,
                "Reject once",
                PermissionOptionKind::RejectOnce,
            ),
            PermissionOption::new(
                Self::REJECT_ALWAYS,
                "Always reject",
                PermissionOptionKind::RejectAlways,
            ),
        ]
    }

    /// 向客户端发一次 `session/request_permission`，等用户决策。
    ///
    /// 在 prompt 的后台任务里调用（非连接事件循环），`block_task()` 不会死锁。
    /// 任何失败（客户端未实现该方法、传输中断）一律按拒绝处理（fail-closed）。
    async fn ask_client(
        &self,
        item: &cc_agent::interaction::ApprovalItem,
    ) -> cc_agent::interaction::ApprovalDecision {
        use cc_agent::interaction::ApprovalDecision;

        let tool_call = ToolCallUpdate::new(
            item.tool_call_id.clone(),
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Pending)
                .title(permission_title(&item.tool_name, &item.tool_input))
                .raw_input(Some(item.tool_input.clone())),
        );
        let req = RequestPermissionRequest::new(
            self.session_id.clone(),
            tool_call,
            Self::permission_options(),
        );
        let resp = match self.cx.send_request(req).block_task().await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    tool = %item.tool_name,
                    "session/request_permission 调用失败，按拒绝处理"
                );
                return ApprovalDecision::Reject {
                    reason: format!(
                        "无法向客户端请求审批（{e}），已默认拒绝；无人值守场景可用 session/set_mode 切换到 bypass"
                    ),
                    source: None,
                };
            }
        };
        decision_from_outcome(&resp.outcome)
    }
}

/// 把客户端的 `session/request_permission` 回复映射为内部审批决策（纯函数，便于单测）。
///
/// 映射规则（fail-closed：任何含糊的情况都按拒绝处理）：
/// - allow-once → 本次放行（不记忆）
/// - allow-always → 本次放行且记为会话级（`source: "session"`，复用审批记忆）
/// - reject-* / cancelled / 未知选项 → 拒绝
fn decision_from_outcome(
    outcome: &RequestPermissionOutcome,
) -> cc_agent::interaction::ApprovalDecision {
    use cc_agent::interaction::ApprovalDecision;
    match outcome {
        RequestPermissionOutcome::Cancelled => ApprovalDecision::Reject {
            reason: "用户取消了审批".to_string(),
            source: None,
        },
        RequestPermissionOutcome::Selected(sel) => match sel.option_id.to_string().as_str() {
            AcpPermissionBroker::ALLOW_ONCE => ApprovalDecision::Approve { source: None },
            AcpPermissionBroker::ALLOW_ALWAYS => ApprovalDecision::Approve {
                source: Some("session".to_string()),
            },
            AcpPermissionBroker::REJECT_ONCE | AcpPermissionBroker::REJECT_ALWAYS => {
                ApprovalDecision::Reject {
                    reason: "用户拒绝".to_string(),
                    source: None,
                }
            }
            other => {
                tracing::warn!(
                    option_id = %other,
                    "session/request_permission 返回了未知选项，按拒绝处理"
                );
                ApprovalDecision::Reject {
                    reason: format!("未知的审批选项：{other}，已按拒绝处理"),
                    source: None,
                }
            }
        },
        // `RequestPermissionOutcome` 是 `#[non_exhaustive]`：协议未来新增 outcome 时保持 fail-closed
        _ => ApprovalDecision::Reject {
            reason: "未知的审批结果，已按拒绝处理".to_string(),
            source: None,
        },
    }
}

/// 审批请求里展示的标题：`工具名: 关键参数`，一眼能看懂在干什么。
fn permission_title(tool_name: &str, input: &serde_json::Value) -> String {
    let detail = match tool_name {
        "Bash" => input.get("command").and_then(|v| v.as_str()),
        _ => input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str()),
    };
    match detail {
        Some(d) => {
            let short: String = d.chars().take(80).collect();
            let short = short.replace(['\n', '\r'], " ");
            let ellipsis = if d.chars().count() > 80 { "…" } else { "" };
            format!("{tool_name}: {short}{ellipsis}")
        }
        None => tool_name.to_string(),
    }
}

#[async_trait::async_trait]
impl cc_agent::interaction::UserInteractionBroker for AcpPermissionBroker {
    async fn request(
        &self,
        context: cc_agent::interaction::InteractionContext,
    ) -> cc_agent::interaction::InteractionResponse {
        use cc_agent::interaction::{InteractionContext, InteractionResponse, QuestionAnswer};
        match context {
            InteractionContext::Approval { items } => {
                // ACP 的 request_permission 是单工具调用粒度的，逐项询问客户端
                let mut decisions = Vec::with_capacity(items.len());
                for item in &items {
                    decisions.push(self.ask_client(item).await);
                }
                InteractionResponse::Decisions(decisions)
            }
            InteractionContext::Questions { requests } => {
                // stdio 下暂无向用户提问的通道，保持原有行为：返回空答案
                InteractionResponse::Answers(
                    requests
                        .into_iter()
                        .map(|q| QuestionAnswer {
                            id: q.id,
                            selected: vec![],
                            text: Some(String::new()),
                        })
                        .collect(),
                )
            }
        }
    }
}

// ─── run_acp_stdio ───────────────────────────────────────────────────────

pub async fn run_acp_stdio(cwd: String) -> anyhow::Result<()> {
    let _telemetry = cc_agent::telemetry::init_tracing("cc-acp");

    // 解析工作目录
    let cwd = std::path::Path::new(&cwd)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&cwd))
        .to_string_lossy()
        .to_string();

    // 加载配置
    let peri_config = cc_tui::config::load().unwrap_or_default();
    let provider = cc_tui::app::agent::LlmProvider::from_config(&peri_config)
        .or_else(cc_tui::app::agent::LlmProvider::from_env)
        .ok_or_else(|| anyhow::anyhow!("No LLM provider configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY, or configure ~/.cc-code/settings.json"))?;

    tracing::info!(
        provider = %provider.display_name(),
        model = %provider.model_name(),
        cwd = %cwd,
        "ACP stdio mode starting"
    );

    // 初始化 cron scheduler
    let cron_scheduler = {
        let scheduler =
            cc_middlewares::cron::CronScheduler::new(tokio::sync::mpsc::unbounded_channel().0);
        Arc::new(parking_lot::Mutex::new(scheduler))
    };

    // 初始化 MCP 连接池（后台）
    let mcp_pool = {
        use cc_middlewares::mcp::{McpClientPool, McpInitStatus};
        let pool = Arc::new(McpClientPool::new_pending());
        let pool_clone = pool.clone();
        let (init_tx, _init_rx) = tokio::sync::watch::channel(McpInitStatus::Pending);
        let cwd_clone = cwd.clone();
        let claude_home = dirs_next::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".claude");
        tokio::spawn(async move {
            McpClientPool::run_initialize(
                pool_clone,
                std::path::Path::new(&cwd_clone),
                &claude_home,
                init_tx,
                None,
                None,
            )
            .await;
        });
        Some(pool)
    };

    // 加载插件数据
    let claude_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude");
    let plugin_data = cc_middlewares::plugin::load_enabled_plugins_aggregated(&claude_dir);

    let plugin_skill_dirs = plugin_data.all_skill_dirs.clone();
    let plugin_agent_dirs = plugin_data.all_agent_dirs.clone();
    let plugin_lsp_servers = plugin_data.all_lsp_servers.clone();
    let plugin_hooks = plugin_data.all_hooks.clone();

    // 组装 hook groups
    let mut hook_groups: Vec<Vec<cc_middlewares::hooks::RegisteredHook>> = Vec::new();
    if !plugin_hooks.is_empty() {
        hook_groups.push(plugin_hooks);
    }
    let global_hooks = cc_middlewares::hooks::loader::load_global_settings_hooks();
    if !global_hooks.is_empty() {
        hook_groups.push(global_hooks);
    }
    let local_hooks = cc_middlewares::hooks::loader::load_settings_local_hooks(&cwd);
    if !local_hooks.is_empty() {
        hook_groups.push(local_hooks);
    }

    // ACP stdio 模式默认 AutoMode：敏感工具调用先过 Jev 门/分类器判定，
    // 不确定的经 AcpPermissionBroker 以 session/request_permission 交给 IDE
    // 客户端审批。之前默认 Bypass 等价于全放行，HITL 形同虚设；无人值守
    // 场景仍可用 session/set_mode 显式切换到 bypass。
    let permission_mode = cc_middlewares::prelude::SharedPermissionMode::new(
        cc_middlewares::prelude::PermissionMode::AutoMode,
    );
    let tool_search_index = Arc::new(cc_middlewares::tool_search::ToolSearchIndex::new());
    let shared_tools = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    // 初始化 thread 存储（失败时 fallback 到临时目录）
    let thread_store: Arc<dyn cc_agent::thread::ThreadStore> =
        match cc_tui::thread::SqliteThreadStore::default_path().await {
            Ok(store) => Arc::new(store),
            // #318: 双路径都失败时降级为内存模式，而非 panic
            Err(_) => {
                match cc_tui::thread::SqliteThreadStore::new(
                    std::env::temp_dir().join("zen-threads.db"),
                )
                .await
                {
                    Ok(store) => Arc::new(store),
                    Err(e) => {
                        tracing::warn!(
                            "SQLite 持久化不可用（{}），降级为内存模式运行",
                            e
                        );
                        Arc::new(
                            cc_tui::thread::SqliteThreadStore::new(":memory:")
                                .await
                                .expect("内存 SQLite 初始化不应失败"),
                        )
                    }
                }
            }
        };

    // 初始化 Langfuse
    let langfuse_session = if let Some(config) = cc_acp::langfuse::LangfuseConfig::from_env() {
        cc_acp::langfuse::LangfuseSession::new(config)
            .await
            .map(Arc::new)
    } else {
        None
    };
    if langfuse_session.is_some() {
        tracing::info!("Langfuse tracing enabled (stdio mode)");
    }

    // 构建共享的 ServerContext，所有请求处理器通过 Arc 共享
    let ctx = Arc::new(StdioContext {
        provider: parking_lot::RwLock::new(provider),
        peri_config: parking_lot::RwLock::new(peri_config),
        permission_mode,
        cron_scheduler,
        mcp_pool,
        channel_state: None,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        plugin_lsp_servers,
        tool_search_index,
        shared_tools,
        sessions: parking_lot::RwLock::new(std::collections::HashMap::new()),
        thread_store,
        langfuse_session,
    });

    use agent_client_protocol::{
        schema::{
            AvailableCommandsUpdate, CancelNotification, CloseSessionRequest, CloseSessionResponse,
            ConfigOptionUpdate, ForkSessionRequest, ForkSessionResponse, InitializeRequest,
            ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, LoadSessionResponse,
            NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
            ResumeSessionRequest, ResumeSessionResponse, SessionId, SessionInfoUpdate,
            SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
            SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
            SetSessionModelRequest, SetSessionModelResponse, StopReason,
        },
        Agent, Client, ConnectionTo,
    };
    use agent_client_protocol_tokio::Stdio;
    use cc_acp::{
        dispatch,
        session::{
            event_sink::StdioEventSink,
            executor,
            state_builders::{
                apply_thinking_effort, build_mode_state, build_model_state, parse_permission_mode,
            },
        },
    };
    use cc_agent::agent::AgentCancellationToken;

    let ctx_clone = ctx.clone();

    Agent
        .builder()
        .name("cc-acp")
        // ── initialize ──
        .on_receive_request(
            async move |_req: InitializeRequest, responder, _cx| {
                tracing::info!("ACP initialize");
                responder.respond(dispatch::build_initialize_response())
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/new ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                    let cwd_str = req.cwd.to_string_lossy().to_string();
                    let meta = cc_agent::thread::ThreadMeta::new(&cwd_str);
                    let thread_id = match ctx.thread_store.create_thread(meta).await {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!(error = %e, "Thread creation failed");
                            // 协议要求以 JSON-RPC 错误结束请求；返回伪造的 sessionId 会让
                            // 客户端把无效会话当成合法会话继续使用。
                            let _ = responder.respond_with_error(
                                agent_client_protocol::Error::internal_error()
                                    .data(format!("Thread creation failed: {e}")),
                            );
                            return Ok(());
                        }
                    };
                    let sid = thread_id.clone();
                    // ── Freeze system prompt data at session creation ──
                    let frozen_date =
                        chrono::Local::now().format("%Y-%m-%d").to_string();

                    let frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                        &cwd_str,
                        &ctx.peri_config.read().config,
                        &ctx.plugin_skill_dirs,
                        &ctx.plugin_agent_dirs,
                        &frozen_date,
                        cc_acp::session::frozen::rule_model_from(&ctx.provider.read()),
                    );

                    // Scan skills for AvailableCommands
                    let skill_dirs = cc_middlewares::SkillsMiddleware::resolve_dirs_static(
                        &cwd_str,
                        &ctx.plugin_skill_dirs,
                    );
                    let skills = cc_middlewares::skills::list_skills(&skill_dirs);

                    {
                        let mut sessions = ctx.sessions.write();
                        sessions.insert(
                            sid.clone(),
                            SessionInfo {
                                session_id: sid.clone(),
                                thread_id: thread_id.clone(),
                                cwd: cwd_str,
                                history: Vec::new(),
                                cancel_token: None,
                                frozen: Some(frozen_data),
                                agent_pool: cc_acp::session::agent_pool::AgentPool::new(),
                            },
                        );
                    }
                    tracing::info!(session_id = %sid, skill_count = skills.len(), "ACP session created with ThreadStore");
                    let modes = build_mode_state(&ctx.permission_mode);
                    let models = {
                        let p = ctx.provider.read();
                        let c = ctx.peri_config.read();
                        build_model_state(&p, &c)
                    };
                    let config_options = {
                        let c = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&c, &p, ctx.permission_mode.load())
                    };
                    let _ = responder.respond(
                        NewSessionResponse::new(SessionId::new(&*sid))
                            .modes(modes)
                            .models(models)
                            .config_options(config_options),
                    );
                    // Push AvailableCommandsUpdate notification
                    let cmds = dispatch::build_available_commands(&skills);
                    let ac_notif = SessionNotification::new(
                        SessionId::new(&*sid),
                        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(cmds)),
                    );
                    let _ = cx.send_notification(ac_notif);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/list ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: ListSessionsRequest, responder, _cx: ConnectionTo<Client>| {
                    let cwd_filter = req
                        .cwd
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string());
                    let entries = dispatch::list_sessions_as_info(
                        ctx.thread_store.as_ref(),
                        cwd_filter.as_deref(),
                    )
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "session/list: failed to list threads");
                        Vec::new()
                    });
                    let _ = responder.respond(ListSessionsResponse::new(entries));
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/prompt ──
        // Execution is spawned into a background task to avoid blocking the
        // event loop.  This is required so that session/cancel (and
        // {"type":"cancel"}) can interrupt an in-progress agent execution.
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                    let sid = req.session_id.0.to_string();
                    // Convert ACP SDK ContentBlocks to cc-agent MessageContent.
                    // Text / Image / ResourceLink / Resource 均有映射；未声明能力的类型
                    // 直接报错，避免「整条 prompt 被过滤成空消息」。
                    let content = match prompt_content_from_acp(&req.prompt) {
                        Ok(content) => content,
                        Err(e) => {
                            let _ = responder.respond_with_error(
                                agent_client_protocol::Error::invalid_params().data(e),
                            );
                            return Ok(());
                        }
                    };

                    // --- capture session-scoped data under the read lock ---
                    let (agent_cwd, history, is_empty_history, thread_id, frozen) = {
                        let sessions = ctx.sessions.read();
                        match sessions.get(&sid) {
                            Some(s) => (
                                s.cwd.clone(),
                                s.history.clone(),
                                s.history.is_empty(),
                                s.thread_id.clone(),
                                s.frozen.clone(),
                            ),
                            None => {
                                // 未知会话必须报错，返回 EndTurn 会让客户端以为「已回复」。
                                let _ = responder.respond_with_error(
                                    agent_client_protocol::Error::invalid_params()
                                        .data(format!("session not found: {sid}")),
                                );
                                return Ok(());
                            }
                        }
                    };
                    let history_len = history.len();

                    let cancel = AgentCancellationToken::new();
                    {
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.cancel_token = Some(cancel.clone());
                        }
                    }

                    // Extract AgentPool from session for cross-prompt LLM reuse
                    let pool_arc = {
                        let mut sessions = ctx.sessions.write();
                        let pool = sessions
                            .get_mut(&sid)
                            .map(|s| {
                                std::mem::replace(
                                    &mut s.agent_pool,
                                    cc_acp::session::agent_pool::AgentPool::new(),
                                )
                            })
                            .unwrap_or_default();
                        Arc::new(parking_lot::Mutex::new(pool))
                    };

                    // --- capture everything the background task needs ---
                    let ctx_for_task = Arc::clone(&ctx);
                    let cx_for_task = cx.clone();
                    let session_id = req.session_id.clone();

                    // Spawn the heavy work to keep the event loop responsive.
                    // responder is moved into the task; the response is sent
                    // when execution completes (or is cancelled).
                    tokio::spawn(async move {
                        let broker: Arc<dyn cc_agent::interaction::UserInteractionBroker> =
                            Arc::new(AcpPermissionBroker::new(
                                cx_for_task.clone(),
                                session_id.clone(),
                            ));

                        let event_sink = Arc::new(StdioEventSink::new(
                            cx_for_task.clone(),
                            session_id.clone(),
                        ));
                        let event_sink_for_notif = Arc::clone(&event_sink);

                        // Snapshot provider / config (release guards before await).
                        let provider_snapshot = ctx_for_task.provider.read().clone();
                        let peri_config_snapshot = Arc::new(ctx_for_task.peri_config.read().clone());

                        let result = executor::execute_prompt(
                            &provider_snapshot,
                            peri_config_snapshot,
                            &agent_cwd,
                            content,
                            frozen,
                            history,
                            vec![], // incoming_recalls
                            is_empty_history,
                            ctx_for_task.permission_mode.clone(),
                            cc_middlewares::hitl::ApprovalMemory::new(),
                            event_sink,
                            cancel,
                            broker,
                            None, // shell_executor（stdio 无 TUI shell 池，用 InlineShellExecutor）
                            ctx_for_task.plugin_skill_dirs.clone(),
                            ctx_for_task.plugin_agent_dirs.clone(),
                            ctx_for_task.hook_groups.clone(),
                            Some(ctx_for_task.cron_scheduler.clone()),
                            sid.clone(),
                            ctx_for_task.mcp_pool.clone(),
                            ctx_for_task.channel_state.clone(),
                            ctx_for_task.tool_search_index.clone(),
                            ctx_for_task.shared_tools.clone(),
                            ctx_for_task.plugin_lsp_servers.clone(),
                            ctx_for_task.langfuse_session.clone(),
                            pool_arc.clone(),
                            Some(Arc::clone(&ctx_for_task.thread_store)),
                            Some(thread_id.clone()),
                            None, // session_manager（stdio 使用自定义 SessionInfo，不走 SessionManager）
                            vec![], // bg_results（stdio 无后台任务）
                            None, // steering（自定义 TUI 扩展，标准 ACP 暂不提供）
                        )
                        .await;

                        // Restore AgentPool back into session
                        if let Ok(mutex) = Arc::try_unwrap(pool_arc) {
                            let mut sessions = ctx_for_task.sessions.write();
                            if let Some(s) = sessions.get_mut(&sid) {
                                s.agent_pool = mutex.into_inner();
                            }
                        }

                        // Persist new messages to ThreadStore.
                        if result.ok && history_len < result.messages.len() {
                            let new_msgs = &result.messages[history_len..];
                            if let Err(e) = ctx_for_task.thread_store.append_messages(&thread_id, new_msgs).await {
                                tracing::warn!(error = %e, "Failed to persist messages to ThreadStore");
                            }
                        }
                        // Update in-memory state.
                        {
                            let mut sessions = ctx_for_task.sessions.write();
                            if let Some(s) = sessions.get_mut(&sid) {
                                s.history = result.messages;
                                s.cancel_token = None;
                            }
                        }

                        let acp_stop_reason = match result.stop_reason {
                            executor::PromptStopReason::Cancelled => StopReason::Cancelled,
                            executor::PromptStopReason::MaxTurnRequests => StopReason::MaxTurnRequests,
                            executor::PromptStopReason::EndTurn => StopReason::EndTurn,
                        };
                        let _ = responder.respond(PromptResponse::new(acp_stop_reason));

                        // Send SessionInfoUpdate after prompt completes.
                        let info = SessionInfoUpdate::new()
                            .updated_at(chrono::Utc::now().to_rfc3339());
                        event_sink_for_notif.send_update(SessionUpdate::SessionInfoUpdate(info));
                    });

                    // Return immediately — the event loop stays free to
                    // process session/cancel and {"type":"cancel"}.
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_mode ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionModeRequest, responder, cx: ConnectionTo<Client>| {
                    let mode_id = req.mode_id.0.as_ref();
                    let mode = parse_permission_mode(mode_id);
                    ctx.permission_mode.store(mode);
                    tracing::info!(mode_id = %mode_id, "Permission mode changed");
                    let config_options = {
                        let c = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&c, &p, ctx.permission_mode.load())
                    };
                    let notif = SessionNotification::new(
                        req.session_id.clone(),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options)),
                    );
                    let _ = cx.send_notification(notif);
                    responder.respond(SetSessionModeResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_model ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionModelRequest, responder, cx: ConnectionTo<Client>| {
                    let model_id = req.model_id.0.to_string();
                    let new_provider = apply_stdio_model_selection(&ctx, &model_id);
                    if let Some(new_provider) = new_provider {
                        tracing::info!(model_id = %model_id, model = %new_provider.model_name(), "Model changed");
                        *ctx.provider.write() = new_provider;
                    }
                    // Model switch → invalidate cached LLM instances for the session
                    {
                        let sid = req.session_id.0.to_string();
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.agent_pool.invalidate();
                        }
                    }
                    let config_options = {
                        let c = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&c, &p, ctx.permission_mode.load())
                    };
                    let notif = SessionNotification::new(
                        req.session_id.clone(),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options)),
                    );
                    let _ = cx.send_notification(notif);
                    responder.respond(SetSessionModelResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/set_config_option ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: SetSessionConfigOptionRequest, responder, cx: ConnectionTo<Client>| {
                    let config_id = req.config_id.0.as_ref();
                    match &req.value {
                        agent_client_protocol_schema::SessionConfigOptionValue::ValueId { value } => {
                            let v = value.0.as_ref();
                            match config_id {
                                "mode" => {
                                    let mode = parse_permission_mode(v);
                                    ctx.permission_mode.store(mode);
                                    tracing::info!(mode = %v, "Permission mode changed via configOption");
                                }
                                "model" => {
                                    let new_provider = apply_stdio_model_selection(&ctx, v);
                                    if let Some(new_provider) = new_provider {
                                        tracing::info!(model_id = %v, model = %new_provider.model_name(), "Model changed via configOption");
                                        *ctx.provider.write() = new_provider;
                                    }
                                    // Model switch → invalidate cached LLM instances
                                    {
                                        let sid = req.session_id.0.to_string();
                                        let mut sessions = ctx.sessions.write();
                                        if let Some(s) = sessions.get_mut(&sid) {
                                            s.agent_pool.invalidate();
                                        }
                                    }
                                }
                                "thinking_effort" => {
                                    apply_thinking_effort(&ctx.peri_config, v);
                                    tracing::info!(effort = %v, "Thinking effort changed via configOption");
                                }
                                _ => {
                                    tracing::debug!(config_id = %config_id, "Unknown config option");
                                }
                            }
                        }
                        agent_client_protocol_schema::SessionConfigOptionValue::Boolean { value: _ } => {
                            tracing::debug!(config_id = %config_id, "Boolean config option not handled");
                        }
                        _ => {
                            tracing::debug!(config_id = %config_id, "Unknown config option value type");
                        }
                    }
                    let config_options = {
                        let cfg = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&cfg, &p, ctx.permission_mode.load())
                    };
                    let notif = SessionNotification::new(
                        req.session_id.clone(),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options.clone())),
                    );
                    let _ = cx.send_notification(notif);
                    responder.respond(SetSessionConfigOptionResponse::new(config_options))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/cancel ──
        .on_receive_notification(
            {
                let ctx = ctx_clone.clone();
                async move |_notif: CancelNotification, _cx| {
                    let sid: &str = &_notif.session_id.0;
                    let sessions = ctx.sessions.read();
                    if let Some(s) = sessions.get(sid) {
                        if let Some(ref token) = s.cancel_token {
                            token.cancel();
                            tracing::info!(session_id = %sid, "Cancel requested");
                        }
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        // ── session/close ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: CloseSessionRequest, responder, _cx: ConnectionTo<Client>| {
                    let sid = req.session_id.0.to_string();
                    let mut sessions = ctx.sessions.write();
                    if let Some(s) = sessions.remove(&sid) {
                        if let Some(ref token) = s.cancel_token {
                            token.cancel();
                        }
                        tracing::info!(session_id = %sid, "Session closed");
                    }
                    let _ = responder.respond(CloseSessionResponse::new());
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/resume ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: ResumeSessionRequest, responder, _cx: ConnectionTo<Client>| {
                    let sid = req.session_id.0.to_string();
                    let cwd = req.cwd.to_string_lossy().to_string();
                    // Build frozen data for session
                    let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                        &cwd,
                        &ctx.peri_config.read().config,
                        &ctx.plugin_skill_dirs,
                        &ctx.plugin_agent_dirs,
                        &frozen_date,
                        cc_acp::session::frozen::rule_model_from(&ctx.provider.read()),
                    );
                    // 与 session/load 的区别：resume 必须恢复会话上下文但**不回放**历史。
                    // 读取必须先于加锁（parking_lot guard 不能跨 await 持有）。
                    let history = dispatch::load_session_messages(ctx.thread_store.as_ref(), &sid).await;
                    let mut sessions = ctx.sessions.write();
                    if !sessions.contains_key(&sid) {
                        sessions.insert(
                            sid.clone(),
                            SessionInfo {
                                session_id: sid.clone(),
                                thread_id: sid.clone(),
                                cwd,
                                history,
                                cancel_token: None,
                                frozen: Some(frozen_data),
                                agent_pool: cc_acp::session::agent_pool::AgentPool::new(),
                            },
                        );
                        tracing::info!(session_id = %sid, "Session resumed (new)");
                    } else if let Some(s) = sessions.get_mut(&sid) {
                        if s.history.is_empty() {
                            s.history = history;
                        }
                        tracing::info!(session_id = %sid, "Session resumed (existing)");
                    }
                    let _ = responder.respond(ResumeSessionResponse::new());
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/load ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
                    let sid = req.session_id.0.to_string();
                    let cwd = req.cwd.to_string_lossy().to_string();
                    let cwd_for_skills = cwd.clone();

                    // Build frozen data for session
                    let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                        &cwd,
                        &ctx.peri_config.read().config,
                        &ctx.plugin_skill_dirs,
                        &ctx.plugin_agent_dirs,
                        &frozen_date,
                        cc_acp::session::frozen::rule_model_from(&ctx.provider.read()),
                    );

                    // Load history from ThreadStore via dispatch function
                    let history = dispatch::load_session_messages(
                        ctx.thread_store.as_ref(),
                        &sid,
                    ).await;

                    // 规范要求：Agent 必须在响应 session/load 之前，用 session/update
                    // 通知把整个会话回放给 Client。顺序不能颠倒。
                    replay_history(&cx, &req.session_id, &history);

                    // Insert into sessions if not already present
                    {
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&sid) {
                            if s.history.is_empty() {
                                s.history = history;
                            }
                        } else {
                            sessions.insert(
                                sid.clone(),
                                SessionInfo {
                                    session_id: sid.clone(),
                                    thread_id: sid.clone(),
                                    cwd,
                                    history,
                                    cancel_token: None,
                                    frozen: Some(frozen_data),
                                    agent_pool: cc_acp::session::agent_pool::AgentPool::new(),
                                },
                            );
                        }
                    }

                    let modes = build_mode_state(&ctx.permission_mode);
                    let models = {
                        let p = ctx.provider.read();
                        let c = ctx.peri_config.read();
                        build_model_state(&p, &c)
                    };
                    let config_options = {
                        let c = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&c, &p, ctx.permission_mode.load())
                    };
                    let resp = LoadSessionResponse::new()
                        .modes(modes)
                        .models(models)
                        .config_options(config_options);
                    let _ = responder.respond(resp);

                    // Scan skills for AvailableCommands notification
                    let skill_dirs = cc_middlewares::SkillsMiddleware::resolve_dirs_static(
                        &cwd_for_skills,
                        &ctx.plugin_skill_dirs,
                    );
                    let skills = cc_middlewares::skills::list_skills(&skill_dirs);
                    let cmds = dispatch::build_available_commands(&skills);
                    let ac_notif = SessionNotification::new(
                        SessionId::new(&*sid),
                        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(cmds)),
                    );
                    let _ = cx.send_notification(ac_notif);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/fork ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: ForkSessionRequest, responder, _cx: ConnectionTo<Client>| {
                    let source_id = req.session_id.0.to_string();
                    let cwd_str = req.cwd.to_string_lossy().to_string();

                    // Get source history
                    let source_history = {
                        let sessions = ctx.sessions.read();
                        sessions.get(&source_id)
                            .map(|s| s.history.clone())
                            .ok_or_else(|| String::from("source session not found"))
                    };
                    let source_history = match source_history {
                        Ok(h) => h,
                        Err(e) => {
                            tracing::warn!(session_id = %source_id, error = %e, "session/fork: source session not found");
                            let _ = responder.respond_with_error(
                                agent_client_protocol::Error::invalid_params()
                                    .data(format!("source session not found: {source_id}")),
                            );
                            return Ok(());
                        }
                    };

                    if source_history.is_empty() {
                        let _ = responder.respond_with_error(
                            agent_client_protocol::Error::invalid_params()
                                .data(format!("source session has no history: {source_id}")),
                        );
                        return Ok(());
                    }

                    // Fork via dispatch function
                    let (new_thread_id, copied_history) = match dispatch::fork_session(
                        ctx.thread_store.as_ref(),
                        &source_id,
                        &source_history,
                        &cwd_str,
                    ).await {
                        Ok((id, msgs)) => (id, msgs),
                        Err(e) => {
                            tracing::error!(error = %e, "session/fork: fork failed");
                            let _ = responder.respond_with_error(
                                agent_client_protocol::Error::internal_error()
                                    .data(format!("fork failed: {e}")),
                            );
                            return Ok(());
                        }
                    };

                    // Insert new session
                    let new_session_id = new_thread_id.clone();
                    // Build frozen data for forked session
                    let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let frozen_data = cc_acp::session::frozen::build_frozen_session_data(
                        &cwd_str,
                        &ctx.peri_config.read().config,
                        &ctx.plugin_skill_dirs,
                        &ctx.plugin_agent_dirs,
                        &frozen_date,
                        cc_acp::session::frozen::rule_model_from(&ctx.provider.read()),
                    );
                    {
                        let mut sessions = ctx.sessions.write();
                        sessions.insert(
                            new_session_id.clone(),
                            SessionInfo {
                                session_id: new_session_id.clone(),
                                thread_id: new_thread_id.clone(),
                                cwd: cwd_str,
                                history: copied_history,
                                cancel_token: None,
                                frozen: Some(frozen_data),
                                agent_pool: cc_acp::session::agent_pool::AgentPool::new(),
                            },
                        );
                    }

                    let resp = ForkSessionResponse::new(SessionId::new(new_session_id));
                    let _ = responder.respond(resp);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── session/update_config (custom extension) ──
        .on_receive_request(
            {
                let ctx = ctx_clone.clone();
                async move |req: agent_client_protocol::UntypedMessage, responder, cx: ConnectionTo<Client>| {
                    // Only handle session/update_config; pass through all others
                    if req.method() != "session/update_config" {
                        return Ok(agent_client_protocol::Handled::No {
                            message: (req, responder),
                            retry: false,
                        });
                    }

                    let session_id = req.params()
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let config_val = req.params().get("config").cloned().unwrap_or_default();

                    let new_cfg: cc_tui::config::PeriConfig =
                        serde_json::from_value(config_val)
                            .map_err(|e| agent_client_protocol::Error::invalid_request()
                                .data(format!("Invalid config: {e}")))?;

                    // Validate providers
                    if new_cfg.config.providers.is_empty() {
                        return Err(agent_client_protocol::Error::invalid_request()
                            .data("providers cannot be empty"));
                    }
                    let active_pid = new_cfg.config.active_provider_id.as_str();
                    if !active_pid.is_empty()
                        && !new_cfg.config.providers.iter().any(|p| p.id == active_pid)
                    {
                        return Err(agent_client_protocol::Error::invalid_request()
                            .data(format!("active_provider_id '{active_pid}' not found")));
                    }

                    *ctx.peri_config.write() = new_cfg.clone();

                    if let Some(p) = cc_tui::app::agent::LlmProvider::from_config(&new_cfg) {
                        tracing::info!(model = %p.model_name(), "Provider updated via session/update_config");
                        *ctx.provider.write() = p;
                    }

                    // Model switch → invalidate cached LLM instances
                    if !session_id.is_empty() {
                        let mut sessions = ctx.sessions.write();
                        if let Some(s) = sessions.get_mut(&session_id) {
                            s.agent_pool.invalidate();
                        }
                    }

                    let config_options = {
                        let c = ctx.peri_config.read();
                        let p = ctx.provider.read();
                        dispatch::config_update::make_config_options(&c, &p, ctx.permission_mode.load())
                    };
                    let notif = SessionNotification::new(
                        SessionId::new(&*session_id),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options.clone())),
                    );
                    let _ = cx.send_notification(notif);
                    let resp = serde_json::to_value(SetSessionConfigOptionResponse::new(config_options))
                        .map_err(|e| agent_client_protocol::Error::internal_error()
                            .data(format!("Serialize failed: {e}")))?;
                    let _ = responder.respond(resp);
                    Ok(agent_client_protocol::Handled::Yes)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(
            Stdio::new().with_debug({
                let ctx_for_cancel = ctx_clone.clone();
                move |line: &str, _direction: agent_client_protocol_tokio::LineDirection| {
                    if line.trim() == r#"{"type":"cancel"}"# {
                        let guard = ctx_for_cancel.sessions.read();
                        for (sid, s) in guard.iter() {
                            if let Some(ref token) = s.cancel_token {
                                token.cancel();
                                tracing::info!(session_id = %sid, "Cancelled via type:cancel");
                            }
                        }
                    }
                }
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("ACP error: {e}"))
}

// ─── ACP 内容转换 / 历史回放辅助函数 ────────────────────────────────────────

/// 将 ACP `session/prompt` 的内容块转换为 peri 内部消息内容。
///
/// 规范基线要求 Agent 至少支持 `Text` 与 `ResourceLink`；`Image` 由
/// `promptCapabilities.image` 声明。未声明能力的类型返回错误而不是静默丢弃——
/// 静默丢弃会让「只带一个 resource link 的 prompt」退化成空消息。
fn prompt_content_from_acp(blocks: &[AcpBlock]) -> Result<MessageContent, String> {
    if blocks.is_empty() {
        return Ok(MessageContent::text(""));
    }

    let mut converted = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            AcpBlock::Text(t) => converted.push(PeriContentBlock::text(t.text.as_str())),
            AcpBlock::Image(img) => {
                converted.push(PeriContentBlock::image_base64(&img.mime_type, &img.data))
            }
            AcpBlock::ResourceLink(link) => {
                converted.push(PeriContentBlock::text(render_resource_link(link)))
            }
            AcpBlock::Resource(res) => {
                converted.push(PeriContentBlock::text(render_embedded_resource(res)))
            }
            AcpBlock::Audio(_) => {
                return Err(
                    "audio content block is not supported (promptCapabilities.audio is false)"
                        .to_string(),
                )
            }
            // ContentBlock 是 #[non_exhaustive]，未来新增类型同样显式报错
            other => return Err(format!("unsupported content block: {other:?}")),
        }
    }
    Ok(MessageContent::Blocks(converted))
}

/// 把 `ResourceLink` 渲染成引用文本，交由 agent 用 Read 工具自行取内容。
fn render_resource_link(link: &agent_client_protocol::schema::ResourceLink) -> String {
    let label = link.title.as_deref().unwrap_or(link.name.as_str());
    match link.mime_type.as_deref() {
        Some(mime) => format!("@{label} ({mime}, {})", link.uri),
        None => format!("@{label} ({})", link.uri),
    }
}

/// 把内嵌资源渲染成文本：文本资源直接内联，二进制资源只保留引用与类型说明。
fn render_embedded_resource(res: &agent_client_protocol::schema::EmbeddedResource) -> String {
    use agent_client_protocol::schema::EmbeddedResourceResource as Res;
    match &res.resource {
        Res::TextResourceContents(t) => format!("@{} (embedded)\n{}", t.uri, t.text),
        Res::BlobResourceContents(b) => {
            format!(
                "@{} (embedded binary omitted, base64 {} chars)",
                b.uri,
                b.blob.len()
            )
        }
        // EmbeddedResourceResource 是 #[non_exhaustive]
        other => format!("[embedded resource omitted: {other:?}]"),
    }
}

/// 把 ThreadStore 中的历史消息回放为 `session/update` 通知。
///
/// 规范要求 `session/load` 在响应之前把整个会话回放给 Client
/// （`/protocol/v1/session-setup#loading-a-session`）。
fn replay_history(cx: &ConnectionTo<Client>, session_id: &AcpSessionId, history: &[BaseMessage]) {
    for message in history {
        for update in history_message_updates(message) {
            let notif = SessionNotification::new(session_id.clone(), update);
            if let Err(e) = cx.send_notification(notif) {
                tracing::warn!(error = %e, "session/load: aborted history replay");
                return;
            }
        }
    }
}

/// 单条历史消息 → `session/update` 列表。
///
/// System 消息是内部提示词状态，不回放；图片/文档块不回放（文本足以恢复上下文语义）。
fn history_message_updates(message: &BaseMessage) -> Vec<SessionUpdate> {
    let mut updates: Vec<SessionUpdate> = Vec::new();
    match message {
        BaseMessage::Human { .. } => {
            for block in message.content_blocks() {
                if let PeriContentBlock::Text { text } = block {
                    if text.is_empty() {
                        continue;
                    }
                    updates.push(SessionUpdate::UserMessageChunk(ContentChunk::new(
                        AcpBlock::Text(TextContent::new(text.to_string())),
                    )));
                }
            }
        }
        BaseMessage::Ai { tool_calls, .. } => {
            // content 里的 ToolUse block 与 tool_calls 字段可能指向同一次调用，
            // 用 id 去重，避免回放出重复的工具卡片。
            let mut seen: Vec<String> = Vec::new();
            for block in message.content_blocks() {
                match block {
                    PeriContentBlock::Text { text } => {
                        if text.is_empty() {
                            continue;
                        }
                        updates.push(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                            AcpBlock::Text(TextContent::new(text.to_string())),
                        )));
                    }
                    PeriContentBlock::Reasoning { text, .. } => {
                        updates.push(SessionUpdate::AgentThoughtChunk(ContentChunk::new(
                            AcpBlock::Text(TextContent::new(text)),
                        )));
                    }
                    PeriContentBlock::ToolUse { id, name, input } => {
                        seen.push(id.clone());
                        updates.push(SessionUpdate::ToolCall(history_tool_call(
                            &id, &name, &input,
                        )));
                    }
                    _ => {}
                }
            }
            for call in tool_calls {
                if seen.contains(&call.id) {
                    continue;
                }
                updates.push(SessionUpdate::ToolCall(history_tool_call(
                    &call.id,
                    &call.name,
                    &call.arguments,
                )));
            }
        }
        BaseMessage::Tool {
            tool_call_id,
            content,
            is_error,
            ..
        } => {
            updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                tool_call_id.clone(),
                ToolCallUpdateFields::new()
                    .status(if *is_error {
                        ToolCallStatus::Failed
                    } else {
                        ToolCallStatus::Completed
                    })
                    .raw_output(Some(serde_json::Value::String(content.text_content()))),
            )));
        }
        BaseMessage::System { .. } => {}
    }
    updates
}

/// 历史工具调用 → `ToolCall`。回放的一定是已结束的调用，故状态取 `Completed`
/// （`kind` 留空，由 Client 按标题推断）。
fn history_tool_call(id: &str, name: &str, input: &serde_json::Value) -> ToolCall {
    ToolCall::new(id.to_string(), name.to_string())
        .status(ToolCallStatus::Completed)
        .raw_input(Some(input.clone()))
}

#[cfg(test)]
#[path = "acp_stdio_test.rs"]
mod tests;
