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
        let tagged = match event {
            AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                ..
            } => AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                ..
            } => AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::TextChunk {
                message_id, chunk, ..
            } => AgentEvent::TextChunk {
                message_id,
                chunk,
                source_agent_id: Some(self.agent_id.clone()),
            },
            other => other,
        };
        self.inner.on_event(tagged);
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
        _ => AgentsMdMiddleware::new().with_config(
            config
                .instruction_config
                .clone()
                .unwrap_or_default(),
        ),
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
    middlewares
}

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

mod build_agent;
mod define;
mod execute_bg;
mod execute_fork;
pub use define::SubAgentTool;

#[cfg(test)]
#[path = "tool_test.rs"]
mod tests;
