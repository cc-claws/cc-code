use std::{collections::HashMap, sync::Arc};

use cc_agent::{
    agent::{
        events::{AgentEvent, AgentEventHandler},
        state::AgentState,
    },
    middleware::r#trait::Middleware,
};

use crate::{
    agents_md::AgentsMdMiddleware,
    hooks::types::{HookEvent, RegisteredHook},
    middleware::todo::TodoMiddleware,
    skills::SkillsMiddleware,
    subagent::{skill_preload::SkillPreloadMiddleware, SubAgentMiddlewareConfig},
};
use tokio::sync::mpsc;

/// 事件处理器包装器：为子 Agent 事件注入 source_agent_id
struct SourceAgentIdHandler {
    inner: Arc<dyn AgentEventHandler>,
    agent_id: String,
}

impl SourceAgentIdHandler {
    fn new(inner: Arc<dyn AgentEventHandler>, agent_id: String) -> Self {
        Self { inner, agent_id }
    }
}

impl AgentEventHandler for SourceAgentIdHandler {
    fn on_event(&self, event: AgentEvent) {
        let tagged = AgentEvent::SubagentEvent {
            source_agent_id: self.agent_id.clone(),
            event: Box::new(event),
        };
        self.inner.on_event(tagged);
    }

    /// #369：包装型 handler 委托内部 handler 的载荷需求声明。
    /// 子 Agent 事件最终汇入同一事件泵（Langfuse 是唯一载荷消费者），
    /// 不委托会让子 Agent 的 LlmCallStart 丢失 Langfuse 输入快照。
    fn wants_llm_call_payload(&self) -> bool {
        self.inner.wants_llm_call_payload()
    }
}

/// 构造 SubAgent 标准中间件链
pub(crate) fn build_subagent_middlewares(
    config: SubAgentMiddlewareConfig,
) -> Vec<Box<dyn Middleware<AgentState>>> {
    let mut middlewares: Vec<Box<dyn Middleware<AgentState>>> = Vec::new();
    // 父的冻结指引只在**子 Agent cwd 与父 cwd 一致**时套用：`Agent` 工具的 cwd 是
    // LLM 可传参，跨项目时用父的指引会串味，还会因冻结短路让子 Agent 读不到目标项目的指引。
    let agents_md = match config.inherited_instructions.as_ref() {
        Some(inh) if inh.cwd.as_ref() == config.cwd => {
            AgentsMdMiddleware::new().with_frozen_instructions(inh.rendered.to_string())
        }
        // 非继承（含跨 cwd）→ 现场加载，用宿主可覆盖的配置
        _ => AgentsMdMiddleware::new()
            .with_config(config.instruction_config.clone().unwrap_or_default()),
    };
    middlewares.push(Box::new(agents_md));
    middlewares.push(Box::new(SkillsMiddleware::new().with_global_config()));
    if !config.skill_names.is_empty() {
        middlewares.push(Box::new(SkillPreloadMiddleware::new(
            config.skill_names,
            &config.cwd,
        )));
    }
    middlewares.push(Box::new(TodoMiddleware::new({
        let (tx, _rx) = mpsc::channel(8);
        tx
    })));
    let mut available_tools = config.available_tools;
    for middleware in &middlewares {
        available_tools.extend(
            middleware
                .collect_tools(&config.cwd)
                .into_iter()
                .map(|tool| tool.name().to_string()),
        );
    }
    middlewares.push(Box::new(permission::SubAgentPermissionMiddleware::new(
        config.permissions,
        config.allowed_tools,
        config.disallowed_tools,
        available_tools,
        config.execution_cwd,
    )));
    middlewares
}

mod permission;

/// 独立（非方法）版本的 SubagentStart/SubagentStop hook 触发逻辑
async fn fire_subagent_lifecycle_hooks_static(
    registered_hooks: &[RegisteredHook],
    event: HookEvent,
    cwd: &str,
    subagent_name: &str,
    result: Option<&str>,
) {
    let matching: Vec<&RegisteredHook> = registered_hooks
        .iter()
        .filter(|h| h.event == event)
        .collect();
    if matching.is_empty() {
        return;
    }

    let input = match &event {
        HookEvent::SubagentStart => {
            crate::hooks::types::HookInput::subagent_start("", "", cwd, subagent_name)
        }
        HookEvent::SubagentStop => crate::hooks::types::HookInput::subagent_stop(
            "",
            "",
            cwd,
            subagent_name,
            result.unwrap_or(""),
        ),
        _ => return,
    };

    for registered in &matching {
        let _action = match &registered.hook {
            crate::hooks::types::HookType::Command { .. } => {
                crate::hooks::executor::execute_command_hook(&registered.hook, &input, registered)
                    .await
            }
            crate::hooks::types::HookType::Http { .. } => {
                crate::hooks::executor::execute_http_hook(&registered.hook, &input).await
            }
            _ => crate::hooks::types::HookAction::Allow,
        };
    }
}

/// Format sub-agent execution result as a summary string returned to the parent agent.
fn format_subagent_result(output: &cc_agent::agent::react::AgentOutput) -> String {
    if output.tool_calls.is_empty() {
        return output.text.clone();
    }

    let mut tool_counts: HashMap<&str, usize> = HashMap::new();
    for (call, _) in &output.tool_calls {
        *tool_counts.entry(call.name.as_str()).or_insert(0) += 1;
    }

    let mut tools: Vec<_> = tool_counts.into_iter().collect();
    tools.sort_by_key(|b| std::cmp::Reverse(b.1));

    let tool_summary = tools
        .into_iter()
        .map(|(name, count)| format!("{} {} times", name, count))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "[Sub-agent executed {} tool calls: {}]\n\n{}",
        output.tool_calls.len(),
        tool_summary,
        output.text
    )
}

/// 子 Agent 的「任务预览」——抽屉行、启动摘要、状态栏都用它代表这个子任务。
///
/// 优先用 Agent 工具显式传入的 `description`（schema 里写明 "used for UI display"），
/// 它才是模型写给用户看的一句话；缺失时才退回 prompt，且必须跳过
/// `【必须全程用中文思考和回复】` 这类**纯指令行**——否则摘要标题会把注入给子 Agent
/// 的约束当成任务名（真机症状：两个后台任务都显示成同一句中文指令）。
pub(crate) fn subagent_task_preview(description: Option<&str>, prompt: &str) -> String {
    let text = description
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .or_else(|| {
            prompt
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !is_instruction_only(line))
        })
        .unwrap_or("");
    text.chars().take(40).collect()
}

/// 整行只有一个 `【…】` / `[…]` 包裹的短语 → 是指令，不是任务描述。
fn is_instruction_only(line: &str) -> bool {
    (line.starts_with('【') && line.ends_with('】')) || (line.starts_with('[') && line.ends_with(']'))
}

mod build_agent;
mod define;
mod execute_bg;
mod execute_fork;
pub use define::SubAgentTool;

#[cfg(test)]
#[path = "tool_test.rs"]
mod tests;

/// None/0 永不自动超时；取消时 drop 执行 future，ManagedChild 负责跨平台杀整棵进程树。
async fn execute_with_timeout(
    agent: &cc_agent::agent::ReActAgent<Box<dyn cc_agent::agent::react::ReactLLM + Send + Sync>, AgentState>,
    input: cc_agent::agent::react::AgentInput,
    state: &mut AgentState,
    cancel: cc_agent::agent::AgentCancellationToken,
    timeout_secs: Option<u64>,
) -> cc_agent::error::AgentResult<cc_agent::agent::react::AgentOutput> {
    let deadline = async {
        match timeout_secs.filter(|seconds| *seconds > 0) {
            Some(seconds) => tokio::time::sleep(std::time::Duration::from_secs(seconds)).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(cc_agent::error::AgentError::Interrupted),
        _ = deadline => {
            cancel.cancel();
            Err(cc_agent::error::AgentError::Other(anyhow::anyhow!("Task timed out after {}s as configured by caller.", timeout_secs.unwrap_or_default())))
        }
        result = agent.execute(input, state, Some(cancel.clone())) => result,
    }
}
