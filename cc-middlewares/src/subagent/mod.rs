mod agent_result;
mod background;
mod built_in_agents;
mod fork;
mod skill_preload;
mod tool;
pub use agent_result::AgentResultTool;
pub use background::{
    BackgroundTask, BackgroundTaskRegistry, BackgroundTaskSnapshot, BackgroundTaskStatus,
};
pub use built_in_agents::{get_built_in_agent, list_built_in_agents, BuiltInAgent};
pub use skill_preload::SkillPreloadMiddleware;
pub use tool::SubAgentTool;

use parking_lot::RwLock;

/// 父 Agent 的**冻结指引** + 它渲染时依据的 cwd（cc-acp 构造后交给子 Agent 链）。
///
/// `cwd` 必须一起带上，不能只传字符串：`Agent` 工具的 `cwd` 是 **LLM 可传参**
/// （默认继承父 cwd），子 Agent 可能跑在另一个项目里——此时父的指引既串味，
/// 又会因「冻结优先」短路让子 Agent **读不到目标项目自己的** `AGENTS.md` / `CLAUDE.md`。
/// 把前提编进类型后，`build_subagent_middlewares` 里用 cwd 相等做闸门。
#[derive(Debug, Clone)]
pub struct InheritedInstructions {
    /// 渲染该指引时的 cwd（父 Agent 的 cwd）
    pub cwd: Arc<str>,
    /// 已渲染的指引整段
    pub rendered: Arc<str>,
}

/// SubAgent 中间件链构造配置
///
/// 链顺序固定：`AgentsMd -> Skills -> [SkillPreload] -> Todo -> SubAgentPermission`；
/// 仅 `skill_names` / `cwd` / `inherited_instructions` 在不同执行路径间变化。
pub(crate) struct SubAgentMiddlewareConfig {
    /// 需要预加载的 skill 名称列表，为空时跳过 SkillPreloadMiddleware
    pub skill_names: Vec<String>,
    /// 与主 Agent 一致的插件技能目录。
    pub extra_skill_dirs: Vec<PathBuf>,
    pub skill_tool: Option<crate::tools::SkillTool>,
    /// 工作目录，用于解析 skill 文件路径
    pub cwd: String,
    /// 父 Agent 的冻结指引（**含父 cwd**；仅当子 Agent cwd 与之一致时才套用）。
    /// `None` = 现场重读磁盘。
    pub inherited_instructions: Option<InheritedInstructions>,
    /// 指引加载配置覆盖（`None` = [`crate::agents_md::AgentsMdConfig::default`]）。
    ///
    /// 非继承路径（子 Agent cwd ≠ 父 cwd）要用它加载指引；宿主可借此钉死用户全局层等，
    /// 使加载结果不依赖运行环境（测试尤其需要——默认配置会去读 `~/.cc-code/AGENTS.md`）。
    pub instruction_config: Option<crate::agents_md::AgentsMdConfig>,
    pub permissions: Arc<crate::hitl::HumanInTheLoopMiddleware>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub available_tools: Vec<String>,
    /// 继承工具真正执行的目录；与子 Agent 读取指引的 cwd 可以不同。
    pub execution_cwd: String,
}

impl SubAgentMiddlewareConfig {
    /// Fork 路径配置（无 skill 预加载）
    pub fn for_fork(cwd: &str) -> Self {
        Self {
            skill_names: Vec::new(),
            extra_skill_dirs: Vec::new(),
            skill_tool: None,
            cwd: cwd.to_string(),
            inherited_instructions: None,
            instruction_config: None,
            permissions: Arc::new(crate::hitl::HumanInTheLoopMiddleware::subagent_default()),
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            available_tools: Vec::new(),
            execution_cwd: cwd.to_string(),
        }
    }
    /// Agent 定义路径配置
    ///
    /// `skills` 来自 `agent_def.frontmatter.skills`，为空时跳过 SkillPreloadMiddleware
    pub fn for_agent_def(skills: Vec<String>, cwd: &str) -> Self {
        Self {
            skill_names: skills,
            extra_skill_dirs: Vec::new(),
            skill_tool: None,
            cwd: cwd.to_string(),
            inherited_instructions: None,
            instruction_config: None,
            permissions: Arc::new(crate::hitl::HumanInTheLoopMiddleware::subagent_default()),
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            available_tools: Vec::new(),
            execution_cwd: cwd.to_string(),
        }
    }
    /// 继承父 Agent 的冻结指引（只在其 cwd 与子 Agent cwd 一致时生效）
    pub fn with_inherited_instructions(
        mut self,
        instructions: Option<InheritedInstructions>,
    ) -> Self {
        self.inherited_instructions = instructions;
        self
    }
}
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use cc_agent::{
    agent::{events::AgentEventHandler, react::ReactLLM, state::State, AgentCancellationToken},
    error::AgentResult,
    messages::BaseMessage,
    middleware::r#trait::Middleware,
    tools::BaseTool,
};

use cc_agent::thread::ThreadStore;

use crate::{agent_define::AgentOverrides, parse_agent_file, tools::BoxToolWrapper};

/// SubAgentMiddleware - injects `Agent` tool into the parent agent
///
/// In the `before_agent` phase, provides `SubAgentTool` to the parent agent via `collect_tools`,
/// enabling the LLM to call the `Agent` tool to delegate sub-tasks to specialized sub-agents.
///
/// # Usage Example
///
/// ```rust,ignore
/// let parent_tools: Vec<Box<dyn BaseTool>> = vec![
///     Box::new(ReadFileTool::new(cwd)),
/// ];
/// let llm_factory = Arc::new(move |_: Option<&str>| {
///     Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
/// });
/// // Optional: system prompt builder, making sub-agent's tone/proactiveness visible in Langfuse
/// let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
///     build_system_prompt(overrides, cwd)
/// });
/// let middleware = SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
///     .with_system_builder(system_builder);
/// let agent = ReActAgent::new(llm).add_middleware(Box::new(middleware));
/// ```
pub struct SubAgentMiddleware {
    /// Parent agent tool set (Arc shared, passed to child agent for use)
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// Parent agent event handler (transparent forwarding of child agent events)
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM factory function, creates independent LLM instance for each child agent
    /// Parameter is optional model alias (e.g., "haiku"/"sonnet"/"opus"), None means use parent model
    #[allow(clippy::type_complexity)]
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    /// When set, child agent injects system prompt via with_system_prompt() (visible in Langfuse)
    #[allow(clippy::type_complexity)]
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Parent agent cancellation token (passed to child agent, supports user interruption)
    cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot, written in before_agent, read by Fork child agent
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    /// 后台任务注册中心（通过 build_tool 传递给 SubAgentTool）
    background_registry: Option<Arc<BackgroundTaskRegistry>>,
    /// Registered hooks for SubagentStart/SubagentStop lifecycle events
    registered_hooks: Arc<Vec<crate::hooks::types::RegisteredHook>>,
    /// Per-child agent event handler factory: takes agent_id → returns handler for that child.
    /// When set, child agents use this factory instead of wrapping the parent's event_handler,
    /// avoiding shared Lock (e.g., Langfuse Mutex) contention in concurrent execution.
    #[allow(clippy::type_complexity)]
    child_handler_factory: Option<Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>>,
    /// 后台任务完成事件的���立发送通道（不随 executor 生命周期销毁）
    bg_event_sender:
        Option<tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::AgentEvent>>,
    /// Thread persistence store for child threads
    thread_store: Option<Arc<dyn ThreadStore>>,
    /// Parent thread ID for child thread hierarchy
    parent_thread_id: Option<String>,
    /// Register callback: (thread_id, cancel_token, cancel_policy_str) → inserts into active_agents map
    #[allow(clippy::type_complexity)]
    register_runtime: Option<Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>>,
    /// Deregister callback: removes from active_agents map by thread_id
    deregister_runtime: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// 父 Agent 的冻结指引（含父 cwd），透传给子 Agent 链（见 `SubAgentMiddlewareConfig`）。
    /// `None` = 子 Agent 自行读盘（会话中途改文件会让其 prompt 前缀抖动 → #360）。
    inherited_instructions: Option<InheritedInstructions>,
    extra_skill_dirs: Vec<PathBuf>,
    /// 插件 agent 搜索目录（`SubAgentTool::load_agent_def` 在项目级与用户级
    /// 之后、内置之前查找；见 #759）。
    extra_agent_dirs: Vec<PathBuf>,
    permissions: Arc<crate::hitl::HumanInTheLoopMiddleware>,
}

impl SubAgentMiddleware {
    #[allow(clippy::type_complexity)]
    pub fn new(
        parent_tools: Vec<Box<dyn BaseTool>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    ) -> Self {
        let tools: Vec<Arc<dyn BaseTool>> = parent_tools
            .into_iter()
            .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
            .collect();
        Self {
            parent_tools: Arc::new(tools),
            event_handler,
            llm_factory,
            system_builder: None,
            cancel: None,
            parent_messages: None,
            background_registry: None,
            registered_hooks: Arc::new(Vec::new()),
            child_handler_factory: None,
            bg_event_sender: None,
            thread_store: None,
            parent_thread_id: None,
            register_runtime: None,
            deregister_runtime: None,
            inherited_instructions: None,
            extra_skill_dirs: Vec::new(),
            extra_agent_dirs: Vec::new(),
            permissions: Arc::new(crate::hitl::HumanInTheLoopMiddleware::subagent_default()),
        }
    }

    /// Set system prompt builder, child agent injects system prompt via `with_system_prompt()` during execution
    #[allow(clippy::type_complexity)]
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    /// Set parent agent cancellation token (passed to child agent, supports user interruption of child agent execution)
    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Set shared parent message reference for Fork child agent inheritance
    pub fn with_parent_messages(mut self, messages: Arc<RwLock<Vec<BaseMessage>>>) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    /// Set background task registry for run_in_background mode
    pub fn with_background_registry(mut self, registry: Arc<BackgroundTaskRegistry>) -> Self {
        self.background_registry = Some(registry);
        self
    }

    /// Set registered hooks for SubagentStart/SubagentStop lifecycle events
    pub fn with_registered_hooks(
        mut self,
        hooks: Vec<crate::hooks::types::RegisteredHook>,
    ) -> Self {
        self.registered_hooks = Arc::new(hooks);
        self
    }

    /// 继承父 Agent 的**冻结**指引（`session/new` 已渲染好）。
    ///
    /// 既省掉每轮重读磁盘，也避免会话中途改 `AGENTS.md` / `CLAUDE.md` 导致子 Agent 的
    /// System 消息变化（prompt cache 前缀抖动 + 行为漂移，见 #360）。
    /// **仅当子 Agent cwd 与指引渲染时的 cwd 一致**才套用（`Agent` 工具的 cwd 可由 LLM 指定）。
    pub fn with_inherited_instructions(
        mut self,
        instructions: Option<InheritedInstructions>,
    ) -> Self {
        self.inherited_instructions = instructions;
        self
    }

    /// 子工具共享父权限配置；派生实例不持有用户审批 broker。
    pub fn with_permissions(mut self, parent: &crate::hitl::HumanInTheLoopMiddleware) -> Self {
        self.permissions = Arc::new(parent.for_subagent());
        self
    }

    /// Set per-child agent event handler factory.
    /// When set, `SubAgentTool::invoke` uses `factory(agent_id)` to create a dedicated
    /// event handler for each child agent, instead of wrapping the parent's shared handler.
    /// This avoids Lock contention (e.g., Langfuse Mutex) when multiple SubAgents run concurrently.
    #[allow(clippy::type_complexity)]
    pub fn with_child_handler_factory(
        mut self,
        factory: Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>,
    ) -> Self {
        self.child_handler_factory = Some(factory);
        self
    }

    /// Set background task event sender.
    /// The sender survives executor lifecycle, allowing bg task results to reach TUI
    /// even after the main agent finishes.
    pub fn with_bg_event_sender(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::AgentEvent>,
    ) -> Self {
        self.bg_event_sender = Some(sender);
        self
    }

    /// Set thread persistence store for child thread creation
    pub fn with_thread_store(mut self, store: Arc<dyn ThreadStore>) -> Self {
        self.thread_store = Some(store);
        self
    }

    /// Set parent thread ID for child thread hierarchy
    pub fn with_parent_thread_id(mut self, id: String) -> Self {
        self.parent_thread_id = Some(id);
        self
    }

    /// Set register callback: called when a child agent thread starts executing.
    /// Parameters: (thread_id, cancel_token, cancel_policy_str)
    #[allow(clippy::type_complexity)]
    pub fn with_register_runtime(
        mut self,
        cb: Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>,
    ) -> Self {
        self.register_runtime = Some(cb);
        self
    }

    /// Set deregister callback: called when a child agent thread finishes (ok/error/cancel).
    /// Parameters: &str (thread_id)
    pub fn with_deregister_runtime(mut self, cb: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.deregister_runtime = Some(cb);
        self
    }

    pub fn with_extra_skill_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_skill_dirs = dirs;
        self
    }

    pub fn with_extra_agent_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.extra_agent_dirs = dirs;
        self
    }

    /// Build SubAgentTool instance (clone Arc fields, do not transfer ownership)
    pub fn build_tool(&self, cwd: &str) -> SubAgentTool {
        let mut tool = SubAgentTool::new(
            Arc::clone(&self.parent_tools),
            self.event_handler.clone(),
            Arc::clone(&self.llm_factory),
            cwd.to_string(),
        );
        tool = tool.with_permissions(Arc::clone(&self.permissions));
        tool = tool.with_extra_skill_dirs(self.extra_skill_dirs.clone());
        tool = tool.with_extra_agent_dirs(self.extra_agent_dirs.clone());
        if let Some(ref builder) = self.system_builder {
            tool = tool.with_system_builder(Arc::clone(builder));
        }
        if let Some(ref cancel) = self.cancel {
            tool = tool.with_cancel(cancel.clone());
        }
        if let Some(ref pm) = self.parent_messages {
            tool = tool.with_parent_messages(Arc::clone(pm));
        }
        if let Some(ref registry) = self.background_registry {
            tool = tool.with_background_registry(Arc::clone(registry));
        }
        if !self.registered_hooks.is_empty() {
            tool = tool.with_registered_hooks(self.registered_hooks.to_vec());
        }
        if let Some(ref factory) = self.child_handler_factory {
            tool = tool.with_child_handler_factory(Arc::clone(factory));
        }
        if let Some(ref sender) = self.bg_event_sender {
            tool = tool.with_bg_event_sender(sender.clone());
        }
        if let Some(ref store) = self.thread_store {
            tool = tool.with_thread_store(Arc::clone(store));
        }
        if let Some(ref instructions) = self.inherited_instructions {
            tool = tool.with_inherited_instructions(Some(instructions.clone()));
        }
        if let Some(ref id) = self.parent_thread_id {
            tool = tool.with_parent_thread_id(id.clone());
        }
        if let Some(ref register) = self.register_runtime {
            tool = tool.with_register_runtime(Arc::clone(register));
        }
        if let Some(ref deregister) = self.deregister_runtime {
            tool = tool.with_deregister_runtime(Arc::clone(deregister));
        }
        tool
    }
}

/// Scan one agents directory, appending unseen agent_ids to `out`.
///
/// Accepts both `{id}.md` files and `{id}/agent.md` directories. Entries
/// whose frontmatter fails to parse are skipped without reserving the id,
/// so a broken higher-priority file cannot shadow a valid lower-priority
/// definition. Callers scan directories in priority order; the first
/// occurrence of an id wins.
fn scan_one_dir(
    dir: &Path,
    seen_ids: &mut std::collections::HashSet<String>,
    out: &mut Vec<(String, String, String)>,
) {
    if !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();

        let (agent_id, file_path): (String, PathBuf) = if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (id, path)
        } else if path.is_dir() {
            let nested = path.join("agent.md");
            if !nested.is_file() {
                continue;
            }
            let id = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (id, nested)
        } else {
            continue;
        };

        // First occurrence wins (callers scan in priority order).
        if seen_ids.contains(&agent_id) {
            continue;
        }

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(agent) = parse_agent_file(&content) {
            let name = if agent.frontmatter.name.is_empty() {
                agent_id.clone()
            } else {
                agent.frontmatter.name.clone()
            };
            let description = agent.frontmatter.description.clone();
            // Reserve the id only on successful parse (see doc comment above).
            seen_ids.insert(agent_id.clone());
            out.push((agent_id, name, description));
        }
    }
}

/// User-level agents directory (`~/.claude/agents/`), if a home dir exists.
pub fn user_agents_dir() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".claude").join("agents"))
}

/// Append built-in agents not already seen (lowest priority).
fn append_built_ins(
    seen_ids: &mut std::collections::HashSet<String>,
    out: &mut Vec<(String, String, String)>,
) {
    for built_in in list_built_in_agents() {
        if seen_ids.insert(built_in.agent_id.to_string()) {
            if let Some(agent) = parse_agent_file(built_in.content) {
                let name = if agent.frontmatter.name.is_empty() {
                    built_in.agent_id.to_string()
                } else {
                    agent.frontmatter.name.clone()
                };
                let description = agent.frontmatter.description.clone();
                out.push((built_in.agent_id.to_string(), name, description));
            }
        }
    }
}

/// Scan the given directories in order (first occurrence of an id wins),
/// then append built-ins. `scan_agents` / `scan_agents_with_extra_dirs`
/// build the directory list (project, user, extras) and delegate here so
/// the priority logic stays testable without depending on `$HOME`.
fn scan_ordered(dirs: &[PathBuf]) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for dir in dirs {
        scan_one_dir(dir, &mut seen_ids, &mut result);
    }
    append_built_ins(&mut seen_ids, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

/// Scan agent directories in priority order, return `(agent_id, name, description)` list.
/// Priority: project (`{cwd}/.claude/agents/`) > user (`~/.claude/agents/`) > built-in.
/// Same-ID duplicates keep the first (highest-priority) occurrence.
pub fn scan_agents(cwd: &str) -> Vec<(String, String, String)> {
    let mut dirs = vec![Path::new(cwd).join(".claude").join("agents")];
    if let Some(user_dir) = user_agents_dir() {
        dirs.push(user_dir);
    }
    scan_ordered(&dirs)
}

/// 扫描 agent 目录，支持额外的插件 agent 搜索路径。
/// 优先级：项目级 > 用户级 > 插件 > 内置，同名 agent_id 去重时保留先出现的。
/// 与 `SubAgentTool::load_agent_def` 的查找顺序一致，列表里有的调用时一定找得到。
pub fn scan_agents_with_extra_dirs(
    cwd: &str,
    extra_dirs: &[PathBuf],
) -> Vec<(String, String, String)> {
    let mut dirs = vec![Path::new(cwd).join(".claude").join("agents")];
    if let Some(user_dir) = user_agents_dir() {
        dirs.push(user_dir);
    }
    dirs.extend(extra_dirs.iter().cloned());
    scan_ordered(&dirs)
}

#[async_trait]
impl<S: State> Middleware<S> for SubAgentMiddleware {
    fn name(&self) -> &str {
        "SubAgentMiddleware"
    }

    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        let mut tools: Vec<Box<dyn BaseTool>> = vec![Box::new(self.build_tool(cwd))];
        if self.background_registry.is_some() {
            tools.push(Box::new(AgentResultTool::new()));
        }
        tools
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // Snapshot current state.messages to shared reference for Fork child agent inheritance
        if let Some(ref pm) = self.parent_messages {
            *pm.write() = state.messages().to_vec();
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
