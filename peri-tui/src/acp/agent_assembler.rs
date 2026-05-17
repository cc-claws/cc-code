// ACP 路径的 Agent 组装：复用 TUI 路径的 build_bare_agent()

use crate::app::agent::{build_bare_agent, BareAgentConfig, BareAgentOutput, LlmProvider};
use parking_lot::RwLock;
use peri_agent::agent::events::AgentEventHandler;
use peri_agent::agent::state::AgentState;
use peri_agent::agent::{AgentCancellationToken, ReActAgent};
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::llm::{BaseModelReactLLM, RetryableLLM};
use peri_middlewares::agent_define::AgentOverrides;
use peri_middlewares::prelude::*;
use peri_middlewares::tools::TodoItem;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::PeriConfig;

pub type PeriLlm = RetryableLLM<BaseModelReactLLM>;
pub type PeriReActAgent = ReActAgent<PeriLlm, AgentState>;

/// ACP 路径的 Agent 组装配置（精简版，仅 ACP 特有参数）
pub struct AgentAssembleConfig {
    pub provider: LlmProvider,
    pub cwd: String,
    pub system_prompt: String,
    pub broker: Arc<dyn UserInteractionBroker>,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub peri_config: Arc<PeriConfig>,
    pub event_handler: Arc<dyn AgentEventHandler>,
    pub cancel: AgentCancellationToken,
    pub cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    pub agent_overrides: Option<AgentOverrides>,
    pub preload_skills: Vec<String>,
    pub session_id: Option<String>,
}

/// 组装 ACP Agent —— 直接复用共享构建逻辑
pub fn assemble_agent(
    cfg: AgentAssembleConfig,
) -> (PeriReActAgent, tokio::sync::mpsc::Receiver<Vec<TodoItem>>) {
    // 创建会话级共享资源（ACP 每次 prompt 独立创建）
    let tool_search_index = Arc::new(peri_middlewares::tool_search::ToolSearchIndex::default());
    let shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    let shared_cfg = BareAgentConfig {
        provider: cfg.provider,
        cwd: cfg.cwd,
        system_prompt: cfg.system_prompt,
        event_handler: cfg.event_handler,
        cancel: cfg.cancel,
        permission_mode: cfg.permission_mode,
        peri_config: cfg.peri_config,
        cron_scheduler: cfg.cron_scheduler,
        agent_overrides: cfg.agent_overrides,
        preload_skills: cfg.preload_skills,
        session_id: cfg.session_id,
        broker: cfg.broker,
        plugin_skill_dirs: vec![],
        plugin_agent_dirs: vec![],
        hook_groups: vec![],
        hook_session_start: false,
        mcp_pool: None,
        tool_search_index,
        shared_tools,
    };

    let BareAgentOutput {
        executor, todo_rx, ..
    } = build_bare_agent(shared_cfg);
    (executor, todo_rx)
}
