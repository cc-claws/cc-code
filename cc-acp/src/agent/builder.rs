//! Shared Agent builder（ACP 和 TUI 共用）
//!
//! 提供 `AcpAgentConfig` 配置结构和 `build_agent()` 构建函数，
//! 组装完整的中间件链和 ReActAgent 实例。
//!
//! 本模块从 cc-tui/src/app/agent.rs:build_bare_agent() 迁移而来，
//! 删除 TUI 特有依赖（AgentEvent channel、map_executor_event），
//! 改为通过 `child_handler_factory` 参数从外部注入。

use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

use cc_agent::{
    agent::{
        compact::CompactConfig,
        events::{AgentEvent as ExecutorEvent, AgentEventHandler},
        token::ContextBudget,
    },
    llm::BaseModel,
};

/// 子 Agent 事件 handler 工厂类型
pub type ChildHandlerFactory = Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>;
/// Register callback: (thread_id, cancel_token, cancel_policy_str) → ()
pub type RegisterRuntimeFn =
    Arc<dyn Fn(String, cc_agent::agent::AgentCancellationToken, String) + Send + Sync>;
/// Deregister callback: &str (thread_id) → ()
pub type DeregisterRuntimeFn = Arc<dyn Fn(&str) + Send + Sync>;
/// System prompt 构建器类型
pub type SystemPromptBuilder = Arc<
    dyn Fn(Option<&cc_middlewares::agent_define::AgentOverrides>, &str) -> String + Send + Sync,
>;
use cc_agent::{
    agent::{state::AgentState, AgentCancellationToken, ReActAgent},
    interaction::{ChannelBroker, ChannelState, MultiplexBroker, UserInteractionBroker},
    llm::BaseModelReactLLM,
};
use cc_middlewares::{
    compact_middleware::CompactMiddleware,
    prelude::*,
    tools::{AskUserTool, TodoItem},
};

use crate::{
    provider::{config::PeriConfig, LlmProvider},
    session::agent_pool::{AgentPool, CachedLlmInstances},
};

// ── 共享 Agent 构建（ACP 和 TUI 共用）─────────────────────────────────────────

/// 共享 Agent 构建配置（ACP 和 TUI 共用）
pub struct AcpAgentConfig {
    pub provider: LlmProvider,
    pub cwd: String,
    pub system_prompt: String,
    /// Frozen **rendered** instruction set (merged + deduped + provenance-tagged,
    /// produced by `agents_md::load_instructions` at session/new).
    /// None = read from disk each turn (legacy, e.g. sub-agents).
    pub frozen_instructions: Option<String>,
    /// Session-scoped lazy Jev rule loader (distils CLAUDE.md rules on first gate use).
    /// None = 门不携带 CLAUDE.md 策略。
    pub jev_rule_loader: Option<Arc<cc_middlewares::hitl::jev::JevRuleLoader>>,
    /// Frozen skills summary (None = scan each turn).
    pub frozen_skill_summary: Option<String>,
    /// Frozen session date in YYYY-MM-DD (None = compute fresh each turn).
    pub frozen_date: Option<String>,
    pub event_handler: Arc<dyn AgentEventHandler>,
    pub cancel: AgentCancellationToken,
    pub permission_mode: Arc<SharedPermissionMode>,
    /// 会话级审批记忆：用户选择「本次会话同意」，文件按路径、命令按完整调用复用。
    pub approval_memory: Arc<cc_middlewares::hitl::ApprovalMemory>,
    pub peri_config: Arc<PeriConfig>,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    pub agent_overrides: Option<cc_middlewares::agent_define::AgentOverrides>,
    pub preload_skills: Vec<String>,
    pub session_id: Option<String>,
    pub broker: Arc<dyn UserInteractionBroker>,
    /// Shell 执行器（注入 BashTool，支持 Ctrl+B 后台化）。
    /// None = 使用默认 InlineShellExecutor（保持原 cmd.output() 同步行为，无后台化）。
    pub shell_executor: Option<Arc<dyn cc_agent::shell::ShellExecutor>>,
    pub plugin_skill_dirs: Vec<std::path::PathBuf>,
    pub plugin_agent_dirs: Vec<std::path::PathBuf>,
    pub hook_groups: Vec<Vec<RegisteredHook>>,
    pub hook_session_start: bool,
    pub mcp_pool: Option<Arc<cc_middlewares::mcp::McpClientPool>>,
    /// Channel 共享状态（None = 不启用 channel 功能，不使用 MultiplexBroker）
    pub channel_state: Option<Arc<ChannelState>>,
    pub tool_search_index: Arc<cc_middlewares::tool_search::ToolSearchIndex>,
    pub shared_tools: Arc<RwLock<HashMap<String, Arc<dyn cc_agent::tools::BaseTool>>>>,
    /// 子 Agent 专用事件 handler factory（由调用方提供，取代 TUI 的 child_event_tx）
    pub child_handler_factory: Option<ChildHandlerFactory>,
    /// LSP 服务器配置（由调用方从 settings.json + 插件配置组装）
    pub lsp_servers: Vec<cc_lsp::config::LspServerConfig>,
    /// Compact 中间件配置（None = 不启用自动 compact）
    pub compact_config: Option<CompactConfig>,
    /// 上下文窗口预算（CompactMiddleware 需要）
    pub compact_budget: Option<ContextBudget>,
    /// LLM 模型（CompactMiddleware 用于 full compact 摘要生成）
    pub compact_model: Option<Arc<dyn BaseModel>>,
    /// 事件通道（CompactMiddleware 发送 compact 事件）
    pub compact_event_tx:
        Option<Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>>>,
    /// Thread persistence store for child thread creation (None = non-persistent)
    pub thread_store: Option<Arc<dyn cc_agent::thread::ThreadStore>>,
    /// Parent thread ID for child thread hierarchy (None = top-level agent)
    pub parent_thread_id: Option<String>,
    /// Register callback: called when a child agent starts executing.
    pub register_runtime: Option<RegisterRuntimeFn>,
    /// Deregister callback: called when a child agent finishes.
    pub deregister_runtime: Option<DeregisterRuntimeFn>,
}

pub struct AcpAgentOutput {
    pub executor: ReActAgent<cc_agent::llm::RetryableLLM<BaseModelReactLLM>, AgentState>,
    pub todo_rx: tokio::sync::mpsc::Receiver<Vec<TodoItem>>,
    /// 后台任务完成事件的独立接收端（不随 executor 生命周期销毁）
    pub bg_event_rx: tokio::sync::mpsc::UnboundedReceiver<ExecutorEvent>,
}

/// 构建可复用的 Agent（ACP 和 TUI 共用核心构建逻辑）
///
/// 迁移自 cc-tui/src/app/agent.rs:build_bare_agent()。
/// 中间件链和 builder 配置与原函数完全一致。
///
/// `cached_llm` 允许跨 prompt 复用 LLM 实例（compact_model、auto_classifier_model），
/// 避免每轮重建 reqwest::Client（~1-2 MB/实例）。首次调用传 `None`，
/// 后续调用传上一次返回的 `Some(CachedLlmInstances)`。
///
/// `pool` 提供 SubAgent LLM 缓存，跨 SubAgent 调用复用 `Arc<dyn BaseModel>`
/// （含共享的 `reqwest::Client`）。首次同模型 SubAgent 调用时创建新实例并插入缓存，
/// 后续调用直接命中缓存，避免每 SubAgent 分配 ~1-2 MB 的 HTTP client。
pub fn build_agent(
    cfg: AcpAgentConfig,
    cached_llm: Option<&CachedLlmInstances>,
    pool: &Arc<parking_lot::Mutex<AgentPool>>,
) -> (AcpAgentOutput, Option<CachedLlmInstances>) {
    let AcpAgentConfig {
        provider,
        cwd,
        system_prompt,
        frozen_instructions,
        jev_rule_loader,
        frozen_skill_summary,
        frozen_date,
        event_handler,
        cancel,
        permission_mode,
        approval_memory,
        peri_config,
        cron_scheduler,
        agent_overrides,
        preload_skills,
        session_id,
        broker: permission_broker,
        shell_executor,
        plugin_skill_dirs,
        plugin_agent_dirs,
        hook_groups,
        hook_session_start,
        mcp_pool,
        channel_state,
        tool_search_index,
        shared_tools,
        child_handler_factory,
        lsp_servers,
        compact_config: mw_compact_config,
        compact_budget: mw_compact_budget,
        compact_model: mw_compact_model,
        compact_event_tx: mw_compact_event_tx,
        thread_store,
        parent_thread_id,
        register_runtime,
        deregister_runtime,
    } = cfg;

    // TerminalMiddleware：注入 shell_executor（None = InlineShellExecutor，保持原行为）。
    // 同一实例同时用于 parent_tools 构造和中间件链，保证 BashTool 与链共享同一 executor。
    let terminal_middleware = shell_executor
        .map(TerminalMiddleware::with_executor)
        .unwrap_or_else(TerminalMiddleware::new);

    // 应用 agent overrides 到系统提示词
    let system_prompt = agent_overrides.as_ref().map_or_else(
        || system_prompt.clone(),
        |ov| {
            let features = crate::prompt::PromptFeatures::detect();
            crate::prompt::build_system_prompt(
                Some(ov),
                &cwd,
                features,
                &plugin_agent_dirs,
                None,
                None,
            )
        },
    );

    let provider_for_factory = provider.clone();
    let model_name = provider.model_name().to_string();
    let provider_name = provider.display_name().to_string();

    // LLM 模型
    let mut base_llm = BaseModelReactLLM::new(provider.into_model());
    if let Some(ref sid) = session_id {
        base_llm = base_llm.with_session_id(sid);
    }
    let model = cc_agent::llm::RetryableLLM::new(base_llm, cc_agent::llm::RetryConfig::default())
        .with_event_handler(Arc::clone(&event_handler));

    // Todo channel
    let (todo_tx, todo_rx) = tokio::sync::mpsc::channel::<Vec<TodoItem>>(8);

    // HITL middleware — reuse auto_classifier model from cache when available
    let auto_classifier_model: Arc<tokio::sync::Mutex<Box<dyn BaseModel>>> = cached_llm
        .map(|c| c.auto_classifier_model.clone())
        .unwrap_or_else(|| {
            Arc::new(tokio::sync::Mutex::new(
                provider_for_factory.clone().into_model(),
            ))
        });
    let auto_classifier: Option<Arc<dyn AutoClassifier>> = Some(Arc::new(LlmAutoClassifier::new(
        auto_classifier_model.clone(),
    )));
    // 构造 permission broker（当 channel_state 存在时用 MultiplexBroker 包装）
    let effective_broker: Arc<dyn UserInteractionBroker> = match (&channel_state, &mcp_pool) {
        (Some(cs), Some(pool)) => {
            let channel_broker = Arc::new(ChannelBroker::new(cs.clone(), pool.clone()));
            Arc::new(MultiplexBroker::new(vec![
                ("tui".to_string(), permission_broker.clone()),
                (
                    "channel".to_string(),
                    channel_broker as Arc<dyn UserInteractionBroker>,
                ),
            ]))
        }
        _ => permission_broker.clone(),
    };

    // Jev 语义门（Auto 模式首选）。仅在**配置了 API key** 时启用；否则返回 None，
    // Auto 模式回退到 LLM 分类器（兜底）。端点不可达不影响构造——判定时 fail-closed。
    // Jev 语义门（Auto 模式的判定内核）。
    //
    // **无论有没有 API key 都要构造**：门里同时承载**确定性层**（硬黑名单、
    // 人写的规则、只读白名单）。若因为"没配 key"就不构造门，Auto 模式会连
    // 确定性防护一起丢掉——那是零成本的、与判定服务无关的防线。
    // 语义层则由 `has_judge()` 单独把关：没有凭据时不发请求，落到分类器兜底。
    // 优先级（门内保证）：人写下的配置 > CLAUDE.md 提炼。
    let jev_config = cc_middlewares::hitl::jev::config::JevConfig::from_env();
    let jev_gate: Option<std::sync::Arc<cc_middlewares::hitl::JevGate>> =
        match cc_middlewares::hitl::JevGate::with_loader(jev_config.clone(), jev_rule_loader) {
            Ok(gate) => {
                let has_key = jev_config.api_key().is_some();
                tracing::info!(
                    endpoint = %jev_config.endpoint,
                    model = %jev_config.model,
                    scope = ?jev_config.gate_scope,
                    judge_enabled = has_key,
                    "Jev 门已启用（确定性层始终生效；语义层{}）",
                    if has_key {
                        "已启用"
                    } else {
                        "无凭据，将回退分类器"
                    }
                );
                if !has_key {
                    tracing::warn!(
                        env = %jev_config.api_key_env,
                        "未配置 Jev API key（{}）：确定性层仍生效，语义判定回退到 LLM 分类器",
                        jev_config.api_key_env
                    );
                }
                Some(gate)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Jev 门初始化失败，Auto 模式回退到 LLM 分类器");
                None
            }
        };

    let hitl = HumanInTheLoopMiddleware::with_shared_mode_and_memory(
        effective_broker.clone(),
        default_requires_approval,
        permission_mode.clone(),
        auto_classifier,
        jev_gate,
        approval_memory.clone(),
    );

    // AskUser 工具：使用原始 TUI broker（permission_broker），不使用 MultiplexBroker。
    // ChannelBroker 对 Questions 立即返回空答案，MultiplexBroker 竞速时 Channel 总是先返回，
    // 导致 AskUserQuestion 弹窗被绕过。
    let ask_user_tool = AskUserTool::new(permission_broker.clone());

    // 父工具集（供子 agent 继承）
    let mut parent_tools: Vec<Box<dyn cc_agent::tools::BaseTool>> =
        FilesystemMiddleware::build_tools(&cwd);
    parent_tools.extend(terminal_middleware.build_tools(&cwd));
    if let Some(ref pool) = mcp_pool {
        let mcp_tools = cc_middlewares::mcp::build_tool_bridges(pool);
        for tool in mcp_tools {
            parent_tools.push(tool);
        }
        if pool.has_resources() {
            parent_tools.push(Box::new(cc_middlewares::mcp::McpResourceTool::new(
                Arc::clone(pool),
            )));
        }
    }

    // 子 agent LLM 工厂（支持 SubAgent LLM 缓存复用）
    let provider_clone = provider_for_factory;
    let config_for_factory = peri_config.clone();
    let session_id_for_factory = session_id.clone();
    let pool_for_subagent = Arc::clone(pool);
    #[allow(clippy::type_complexity)]
    let llm_factory: Arc<
        dyn Fn(Option<&str>) -> Box<dyn cc_agent::agent::react::ReactLLM + Send + Sync>
            + Send
            + Sync,
    > = Arc::new(move |model_alias: Option<&str>| {
        let sid = session_id_for_factory.as_deref();
        // 解析 provider 并构建 fingerprint
        let (p, fp) = if let Some(alias) = model_alias {
            match LlmProvider::from_config_for_alias(&config_for_factory, alias) {
                Some(p) => {
                    let fp = format!("{}:{}", p.display_name(), p.model_name());
                    (Some(p), fp)
                }
                None => {
                    let fp = format!(
                        "{}:{}",
                        provider_clone.display_name(),
                        provider_clone.model_name()
                    );
                    (None, fp)
                }
            }
        } else {
            let fp = format!(
                "{}:{}",
                provider_clone.display_name(),
                provider_clone.model_name()
            );
            (None, fp)
        };

        // 尝试 SubAgent 缓存
        let model: Arc<dyn BaseModel> =
            crate::session::agent_pool::AgentPool::get_or_create_subagent_llm(
                &pool_for_subagent,
                &fp,
                || match &p {
                    Some(provider) => provider.clone().into_model(),
                    None => provider_clone.clone().into_model(),
                },
            );

        let mut llm = BaseModelReactLLM::from_arc(model);
        if let Some(s) = sid {
            llm = llm.with_session_id(s);
        }
        Box::new(cc_agent::llm::RetryableLLM::new(
            llm,
            cc_agent::llm::RetryConfig::default(),
        ))
    });

    // 系统提示构建器
    let frozen_language_for_sub = peri_config.config.language.clone();
    let frozen_date_for_sub = frozen_date.clone();
    let system_builder: SystemPromptBuilder = Arc::new(move |overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(
            overrides,
            cwd_dir,
            features,
            &[],
            frozen_date_for_sub.as_deref(),
            frozen_language_for_sub.as_deref(),
        )
    });

    // Parent message snapshot
    let parent_messages: Arc<RwLock<Vec<cc_agent::messages::BaseMessage>>> =
        Arc::new(RwLock::new(Vec::new()));

    // 后台任务通知通道
    let (bg_notification_tx, bg_notification_rx) = tokio::sync::mpsc::unbounded_channel();
    let background_registry = Arc::new(cc_middlewares::BackgroundTaskRegistry::new(
        bg_notification_tx,
    ));

    // 后台任务完成事件的独立通道（不随 executor 生命周期销毁）
    let (bg_event_tx, bg_event_rx) = tokio::sync::mpsc::unbounded_channel();

    let claude_md_excludes = peri_config
        .config
        .claude_md_excludes
        .clone()
        .unwrap_or_default();

    // 把父的**冻结**指引（连同它渲染时的 cwd）交给子 Agent 链：既省掉每轮重读磁盘，
    // 也避免会话中途改 AGENTS.md/CLAUDE.md 让子 Agent 的 System 消息变化
    // （prompt cache 前缀抖动 + 行为漂移，见 #360）。cwd 一起带上是因为 `Agent` 工具的
    // cwd 是 LLM 可传参——只有子 Agent cwd 与父一致时才能套用这份指引。
    let inherited_instructions = frozen_instructions.as_deref().map(|rendered| {
        cc_middlewares::subagent::InheritedInstructions {
            cwd: Arc::from(cwd.as_str()),
            rendered: Arc::from(rendered),
        }
    });

    // SubAgent middleware
    let mut subagent = SubAgentMiddleware::new(
        parent_tools,
        Some(Arc::clone(&event_handler) as Arc<dyn AgentEventHandler>),
        llm_factory.clone(),
    )
    .with_system_builder(system_builder)
    .with_cancel(cancel.clone())
    .with_parent_messages(parent_messages)
    .with_background_registry(Arc::clone(&background_registry))
    .with_bg_event_sender(bg_event_tx)
    .with_registered_hooks(vec![])
    .with_inherited_instructions(inherited_instructions)
    .with_permissions(&hitl);
    if let Some(ts) = thread_store {
        subagent = subagent.with_thread_store(ts);
    }
    if let Some(pti) = parent_thread_id {
        subagent = subagent.with_parent_thread_id(pti);
    }
    if let Some(factory) = child_handler_factory {
        subagent = subagent.with_child_handler_factory(factory);
    }
    if let Some(register) = register_runtime {
        subagent = subagent.with_register_runtime(register);
    }
    if let Some(deregister) = deregister_runtime {
        subagent = subagent.with_deregister_runtime(deregister);
    }

    // 上下文预算
    let mut context_window = model.context_window();
    let context_1m = peri_config.config.context_1m.unwrap_or(false);
    if context_1m {
        context_window = 1_000_000;
    }
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
    let context_budget = cc_agent::agent::token::ContextBudget::new(context_window)
        .with_auto_compact_threshold(compact_config.auto_compact_threshold)
        .with_warning_threshold(compact_config.micro_compact_threshold);

    // 将 Git Attribution 追加到系统提示词末尾（动态区域，不影响缓存前缀）
    let attribution = cc_middlewares::GitAttributionMiddleware::attribution_text(&model_name);
    let system_prompt = format!(
        "{}\n\n## Git Attribution\n\nWhen creating git commits, append the following line to the commit message:\n\n```\n{}\n```\n\nThis tracks AI contributions and should always be included in commit messages for code you authored.",
        system_prompt, attribution
    );

    // 构建 ReActAgent
    let executor = ReActAgent::new(model)
        .max_iterations(500)
        .with_context_budget(context_budget)
        .with_compact_config(compact_config)
        .with_notification_rx(bg_notification_rx)
        .with_system_prompt(system_prompt)
        .with_tool_filter(cc_middlewares::tool_search::is_deferred_tool)
        .with_shared_tools(Arc::clone(&shared_tools))
        .add_middleware(Box::new({
            let mut mw = AgentsMdMiddleware::new().with_excludes(claude_md_excludes);
            if let Some(rendered) = frozen_instructions {
                mw = mw.with_frozen_instructions(rendered);
            }
            mw
        }))
        .add_middleware(Box::new(AgentDefineMiddleware::new()))
        .add_middleware(Box::new({
            let mut mw = SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs);
            if let Some(summary) = frozen_skill_summary {
                mw = mw.with_frozen_summary(summary);
            }
            mw
        }))
        .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
        .add_middleware(Box::new(cc_middlewares::AtMentionMiddleware::new(
            cwd.clone().into(),
        )))
        .add_middleware(Box::new(FilesystemMiddleware::new()))
        .add_middleware(Box::new(cc_middlewares::GitAttributionMiddleware::new(
            &model_name,
        )))
        .add_middleware(Box::new(terminal_middleware))
        .add_middleware(Box::new(WebMiddleware::new()))
        .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
        .add_middleware(Box::new(CronMiddleware::new(
            cron_scheduler.unwrap_or_else(|| {
                Arc::new(parking_lot::Mutex::new(CronScheduler::new(
                    tokio::sync::mpsc::unbounded_channel().0,
                )))
            }),
        )));

    // Hook middleware groups
    // 收集所有 hooks（在 hook_groups 被 move 之前，供 CompactMiddleware 和 HookMiddleware 共用）
    let all_hooks: Vec<RegisteredHook> = hook_groups.iter().flatten().cloned().collect();
    let mut executor = executor;
    if !hook_groups.is_empty() {
        let hook_llm_factory: Arc<
            dyn Fn() -> Box<dyn cc_agent::agent::react::ReactLLM + Send + Sync> + Send + Sync,
        > = Arc::new({
            let factory = llm_factory.clone();
            move || factory(None)
        });
        for (i, group) in hook_groups.into_iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let mw = cc_middlewares::hooks::HookMiddleware::with_session_start(
                group,
                hook_llm_factory.clone(),
                &cwd,
                "",
                "",
                permission_mode.clone(),
                provider_name.clone(),
                // 首个 hook 组在 session 首次 prompt 时触发 SessionStart，
                // source 默认为 "startup"。resume/clear/compact 场景的细分
                // 由调用方（execute_prompt 之上）后续传入，当前保持向后兼容。
                if hook_session_start && i == 0 {
                    Some("startup".to_string())
                } else {
                    None
                },
            );
            executor = executor.add_middleware(Box::new(mw));
        }
    }

    let executor = executor.add_middleware(Box::new(hitl));
    let executor = executor.add_middleware(Box::new(subagent));

    // MCP 中间件
    let executor = if let Some(pool) = mcp_pool {
        executor.add_middleware(Box::new(cc_middlewares::mcp::McpMiddleware::new(pool)))
    } else {
        executor
    };

    // ToolSearch 中间件
    let executor = executor.add_middleware(Box::new(cc_middlewares::ToolSearchMiddleware::new(
        Arc::clone(&tool_search_index),
        Arc::clone(&shared_tools),
    )));

    let executor = executor
        .with_event_handler(Arc::clone(&event_handler))
        .register_tool(Box::new(ask_user_tool));

    // LSP 中间件（条件注册，当有 LSP 服务器配置时）
    let executor = if !lsp_servers.is_empty() {
        let lsp_config = cc_lsp::config::LspConfigFile {
            lsp_servers: lsp_servers
                .into_iter()
                .map(|s| (s.name.clone(), s))
                .collect(),
        };
        tracing::info!(
            target: "lsp",
            servers = lsp_config.lsp_servers.len(),
            "LSP 中间件已注册"
        );
        executor.add_middleware(Box::new(cc_middlewares::LspMiddleware::new(
            cwd.clone(),
            lsp_config,
        )))
    } else {
        executor
    };

    // CompactMiddleware（条件注册，当 compact 配置+模型+事件通道均可用时）
    // 注意：mw_compact_model 可能来自 cache（通过 executor.rs），此时复用同一 Arc
    let compact_model_for_cache: Option<Arc<dyn BaseModel>> = mw_compact_model.clone();
    let executor = if let (Some(config), Some(budget), Some(model), Some(event_tx)) = (
        mw_compact_config,
        mw_compact_budget,
        mw_compact_model,
        mw_compact_event_tx,
    ) {
        let compact_mw = CompactMiddleware::new(
            Some(model),
            config,
            budget,
            cwd.clone(),
            event_tx,
            cancel.clone(),
            all_hooks,
            session_id.unwrap_or_default(),
            provider_name.clone(),
        );
        executor.add_middleware(Box::new(compact_mw))
    } else {
        executor
    };

    // 构建 CachedLlmInstances 供跨 prompt 复用
    let new_cache = compact_model_for_cache.map(|model| CachedLlmInstances {
        compact_model: model,
        auto_classifier_model,
        fingerprint: format!("{}:{}", provider_name, model_name),
    });

    (
        AcpAgentOutput {
            executor,
            todo_rx,
            bg_event_rx,
        },
        new_cache,
    )
}
