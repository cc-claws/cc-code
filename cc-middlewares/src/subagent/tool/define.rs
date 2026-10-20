use std::sync::Arc;

use async_trait::async_trait;
use cc_agent::{
    agent::{events::AgentEventHandler, react::ReactLLM, AgentCancellationToken},
    messages::BaseMessage,
    thread::ThreadStore,
    tools::BaseTool,
};

use crate::tool_search::core_tools::TOOL_AGENT;
use crate::{
    agent_define::{AgentDefineMiddleware, AgentOverrides},
    claude_agent_parser::{parse_agent_file, ClaudeAgent, ToolsValue},
    hooks::types::RegisteredHook,
    subagent::{background::BackgroundTaskRegistry, built_in_agents::get_built_in_agent},
};
use parking_lot::RwLock;

/// RAII guard that calls deregister on drop (panic-safe cleanup).
pub(crate) struct DeregisterGuard {
    pub(crate) thread_id: String,
    pub(crate) deregister: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl Drop for DeregisterGuard {
    fn drop(&mut self) {
        if let Some(ref deregister) = self.deregister {
            deregister(&self.thread_id);
        }
    }
}

/// SubAgentTool - implements the `Agent` tool, allowing LLM to delegate sub-tasks to specialized sub-agents
const AGENT_DESCRIPTION: &str = r#"Delegate a bounded task to a sub-agent when specialized context or independent parallel work helps complete the user's request.

Usage:
- subagent_type is REQUIRED unless fork=true. Use an ID from the available agent definitions, or set fork=true without subagent_type.
- Normal agents have independent message state. Supply the objective, relevant context, constraints, allowed files, and expected result in prompt.
- fork=true uses an inherited conversation snapshot. Include recent constraints or results needed for this task if the snapshot may lack them; it does not guarantee the parent's current full history or system prompt.
- Tools derive from the parent's available set with Agent excluded, and may be restricted by the agent definition. Delegation grants no new permissions.
- Agents share filesystem access and normally inherit cwd. Assign separate write scopes and avoid concurrent edits to the same files. The isolation parameter is reserved and does not create a worktree.
- Request a concise handoff covering Scope, Result, Key files, Files changed, and verification evidence or blockers. Tool-use summaries may precede the final response.
- Sub-agents always run in the background: the bottom drawer tracks each one, your turn continues immediately, and each result is delivered as a completion notification. At most 3 run concurrently.
- There is no foreground mode: a sub-agent's internals never render into the main conversation.
- timeout_secs optionally caps how long the sub-agent may run; omit it for long-running work (no timeout by default).
- To cancel a running sub-agent (e.g. the user changes their mind, or you need to free a concurrency slot), call this tool again with action="stop" and the task_id from the spawn result (`Background task bg-<uuid> started ...`). Stop by task_id only — never guess or match by task description, since sibling tasks often read alike. The user can also stop tasks themselves in the bottom drawer (x / ctrl+x ctrl+k)."#;

#[derive(Clone)]
pub struct SubAgentTool {
    /// Parent agent tool set (Arc shared, read-only)
    pub(crate) parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// Parent agent event handler (transparent forwarding of sub-agent events)
    pub(crate) event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// Parent agent working directory (inherited when LLM does not specify cwd)
    pub(crate) parent_cwd: String,
    /// LLM factory function, creates independent LLM instance for each sub-agent (no system, injected via with_system_prompt())
    #[allow(clippy::type_complexity)]
    pub(crate) llm_factory:
        Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    #[allow(clippy::type_complexity)]
    pub(crate) system_builder:
        Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Optional cancellation token for interrupting sub-agent execution
    pub(crate) cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot (used by Fork path)
    pub(crate) parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
    /// 后台任务注册中心（run_in_background 模式使用）
    pub(crate) background_registry: Option<Arc<BackgroundTaskRegistry>>,
    /// 子 agent 生命周期 hook（SubagentStart/SubagentStop）
    pub(crate) registered_hooks: Arc<Vec<RegisteredHook>>,
    /// Per-child event handler factory
    #[allow(clippy::type_complexity)]
    pub(crate) child_handler_factory:
        Option<Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>>,
    /// 后台任务完成事件的独立发送通道（不随 executor 生命周期销毁）
    pub(crate) bg_event_sender:
        Option<tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::AgentEvent>>,
    /// Thread persistence store for child threads
    pub(crate) thread_store: Option<Arc<dyn ThreadStore>>,
    /// Parent thread ID for child thread hierarchy
    pub(crate) parent_thread_id: Option<String>,
    /// Register callback: (thread_id, cancel_token, cancel_policy_str) → inserts into active_agents map
    #[allow(clippy::type_complexity)]
    pub(crate) register_runtime:
        Option<Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>>,
    /// Deregister callback: removes from active_agents map by thread_id
    pub(crate) deregister_runtime: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// 父 Agent 的冻结指引（含父 cwd），供子 Agent 链继承（避免每轮重读，见 #360）
    pub(crate) inherited_instructions: Option<super::super::InheritedInstructions>,
    pub(crate) permissions: Arc<crate::hitl::HumanInTheLoopMiddleware>,
}

impl SubAgentTool {
    /// `action="stop"`：按 `task_id` 精确停止一个正在运行的后台子 Agent。
    ///
    /// **只接受精确的 `task_id`（`bg-<uuid>`）**，不支持按任务描述/名称模糊匹配：
    /// 多个子任务的描述常高度相似（如「查询广州天气」/「查询厦门天气」），模糊匹配
    /// 会停错对象。宁可让模型多查一次，也不误停。
    ///
    /// 底层复用 `BackgroundTaskRegistry::cancel()`：它**不 abort 任务**，而是触发执行器的
    /// `CancellationToken`，让执行器正常发出终态、完成持久化与清理（另有前台化 shell 的
    /// 精确中断路径）。工具层不另写终止逻辑。
    async fn invoke_stop(
        &self,
        input: &serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let task_id = match input.get("task_id").and_then(|v| v.as_str()) {
            Some(id) if !id.trim().is_empty() => id.trim().to_string(),
            _ => return Err("Error: action=stop requires the `task_id` parameter".into()),
        };
        let registry = self.background_registry.as_ref().ok_or(
            "Error: sub-agents run in the background only, but no background registry is configured in this session",
        )?;
        // 先取摘要用于可读回执：停止后快照会转终态，摘要必须先读。取不到不影响停止，
        // 只是回执里省略摘要（例如已被并发完成）。
        let preview = registry
            .status(&task_id)
            .ok()
            .map(|snapshot| snapshot.task_preview)
            .filter(|preview| !preview.is_empty());
        registry
            .cancel(&task_id)
            .map_err(|error| format!("Error: {error}"))?;
        Ok(match preview {
            Some(preview) => format!("Stopped background task {task_id} ({preview})."),
            None => format!("Stopped background task {task_id}."),
        })
    }

    #[allow(clippy::type_complexity)]
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        parent_cwd: String,
    ) -> Self {
        Self {
            parent_tools,
            event_handler,
            llm_factory,
            parent_cwd,
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
            permissions: Arc::new(crate::hitl::HumanInTheLoopMiddleware::subagent_default()),
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_parent_messages(mut self, messages: Arc<RwLock<Vec<BaseMessage>>>) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    pub fn with_background_registry(mut self, registry: Arc<BackgroundTaskRegistry>) -> Self {
        self.background_registry = Some(registry);
        self
    }

    pub fn with_registered_hooks(mut self, hooks: Vec<RegisteredHook>) -> Self {
        self.registered_hooks = Arc::new(hooks);
        self
    }

    #[allow(clippy::type_complexity)]
    pub fn with_child_handler_factory(
        mut self,
        factory: Arc<dyn Fn(String) -> Arc<dyn AgentEventHandler> + Send + Sync>,
    ) -> Self {
        self.child_handler_factory = Some(factory);
        self
    }

    pub fn with_bg_event_sender(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<cc_agent::agent::events::AgentEvent>,
    ) -> Self {
        self.bg_event_sender = Some(sender);
        self
    }

    pub fn with_thread_store(mut self, store: Arc<dyn ThreadStore>) -> Self {
        self.thread_store = Some(store);
        self
    }

    pub fn with_parent_thread_id(mut self, id: String) -> Self {
        self.parent_thread_id = Some(id);
        self
    }

    #[allow(clippy::type_complexity)]
    pub fn with_register_runtime(
        mut self,
        cb: Arc<dyn Fn(String, AgentCancellationToken, String) + Send + Sync>,
    ) -> Self {
        self.register_runtime = Some(cb);
        self
    }

    pub fn with_deregister_runtime(mut self, cb: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        self.deregister_runtime = Some(cb);
        self
    }

    /// 继承父 Agent 的冻结指引（仅在子 Agent cwd 与父 cwd 一致时生效）
    pub fn with_inherited_instructions(
        mut self,
        instructions: Option<super::super::InheritedInstructions>,
    ) -> Self {
        self.inherited_instructions = instructions;
        self
    }

    pub(crate) fn with_permissions(
        mut self,
        permissions: Arc<crate::hitl::HumanInTheLoopMiddleware>,
    ) -> Self {
        self.permissions = permissions;
        self
    }

    pub(crate) fn permission_config(
        &self,
        mut config: crate::subagent::SubAgentMiddlewareConfig,
        allowed: &ToolsValue,
        disallowed: &ToolsValue,
    ) -> crate::subagent::SubAgentMiddlewareConfig {
        config.permissions = Arc::clone(&self.permissions);
        config.allowed_tools = allowed.to_vec();
        config.disallowed_tools = disallowed.to_vec();
        config.available_tools = self
            .parent_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect();
        config.execution_cwd = self.parent_cwd.clone();
        config
    }

    pub(crate) fn load_agent_def(&self, agent_id: &str, cwd: &str) -> Result<ClaudeAgent, String> {
        let agent_path = AgentDefineMiddleware::candidate_paths(cwd, agent_id)
            .into_iter()
            .find(|p| p.is_file());

        if let Some(path) = agent_path {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Error: failed to read agent definition file: {}", e))?;
            return parse_agent_file(&content).ok_or_else(|| {
                format!(
                    "Error: failed to parse agent definition file '{}'",
                    path.display()
                )
            });
        }

        let built_in = get_built_in_agent(agent_id)
            .ok_or_else(|| format!("Error: cannot find agent definition '{}'. Check .claude/agents/ directory or use a built-in agent (explore, plan, general-purpose, verification)", agent_id))?;
        parse_agent_file(built_in.content).ok_or_else(|| {
            format!(
                "Error: failed to parse built-in agent definition '{}'",
                agent_id
            )
        })
    }

    pub(crate) fn overrides_from_agent_def(
        system_prompt: &str,
        tone: &Option<String>,
        proactiveness: &Option<String>,
    ) -> Option<AgentOverrides> {
        crate::subagent::fork::overrides_from_agent_def(system_prompt, tone, proactiveness)
    }

    pub(crate) fn filter_tools(
        &self,
        allowed: &ToolsValue,
        disallowed: &ToolsValue,
    ) -> Vec<Box<dyn BaseTool>> {
        crate::subagent::fork::filter_tools(&self.parent_tools, allowed, disallowed)
    }
}

#[async_trait]
impl BaseTool for SubAgentTool {
    fn name(&self) -> &str {
        TOOL_AGENT
    }

    fn description(&self) -> &str {
        AGENT_DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            // 注意：**不能**把 prompt 写死为 required。action=stop 时无需 prompt，
            // 若 schema 强制必填，模型会为通过校验而凭空编造一个 prompt。
            // 必填性在 invoke 内按 action 分支校验（spawn 需 prompt，stop 需 task_id）。
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["spawn", "stop"],
                    "description": "spawn (default) launches a new sub-agent; stop cancels a running one by task_id. Omit for spawn."
                },
                "task_id": {
                    "type": "string",
                    "description": "Required when action=stop. The `bg-<uuid>` id returned by the spawn call's result text. Exact match only — task descriptions are not accepted as targets."
                },
                "prompt": {
                    "type": "string",
                    "description": "REQUIRED when action=spawn (the default). Bounded task, context, constraints, allowed write scope, and expected result. For forks, include essential recent facts that may be absent from the inherited snapshot. Not needed for action=stop"
                },
                "description": {
                    "type": "string",
                    "description": "A short description of the task (3-5 words), used for UI display and logging"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "REQUIRED unless fork=true. Exact ID from the available agents list, including built-in or configured agents (e.g. 'explore', 'plan', 'general-purpose', 'verification')"
                },
                "name": {
                    "type": "string",
                    "description": "A short alias for the sub-agent, used for UI identification"
                },
                "isolation": {
                    "type": "string",
                    "description": "Reserved for future use; currently does not provide filesystem or worktree isolation"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the sub-agent. Defaults to inheriting the parent agent's current working directory if not specified"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional hard timeout in seconds for this background sub-agent. Omit (or pass 0) for long-running work: there is no timeout by default. When it elapses the task is cancelled and reported as timed out"
                },
                "fork": {
                    "type": "boolean",
                    "description": "Use an inherited conversation snapshot instead of an agent definition. Omit subagent_type in this mode; include any essential missing context in prompt"
                }
            }
        })
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // action 缺省为 spawn：兼容既有调用方与历史会话，不依赖模型显式传参。
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("spawn");
        if action == "stop" {
            return self.invoke_stop(&input).await;
        }
        if action != "spawn" {
            return Err(
                format!("Error: unknown action `{action}` (expected `spawn` or `stop`)").into(),
            );
        }
        // spawn 路径必须带 prompt；此处校验而非依赖 schema 的 required，
        // 因为 required 会同时作用于 action=stop（见 parameters 注释）。
        let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return Err("Error: missing required parameter prompt".into()),
        };
        let subagent_type = input
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let _name = input.get("name").and_then(|v| v.as_str());
        let _isolation = input.get("isolation").and_then(|v| v.as_str());
        // 子 Agent 只有后台模式（抽屉是唯一呈现，结果经完成通知回流）。
        // 前台同步已被移除：它会把子 Agent 内部过程塞进主 Agent 的消息区，
        // 正是引入抽屉要解决的问题。
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.parent_cwd)
            .to_string();
        let is_fork = input.get("fork").and_then(|v| v.as_bool()).unwrap_or(false)
            || subagent_type.as_deref() == Some("fork");
        let timeout_secs = input.get("timeout_secs").and_then(|v| v.as_u64());

        self.invoke_background(
            prompt,
            subagent_type,
            cwd,
            is_fork,
            timeout_secs,
            description,
        )
        .await
    }
}
