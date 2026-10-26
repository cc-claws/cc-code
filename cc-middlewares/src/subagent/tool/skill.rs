//! Skill 的 context: fork 是等待结果的隔离执行，不创建后台任务。

use std::sync::Arc;

use cc_agent::{
    agent::{events::{AgentEvent, AgentEventHandler}, react::AgentInput},
    tools::ToolInvocationContext,
};

use crate::skills::invocation::SkillError;

use super::{build_agent::AgentBuildOptions, define::DeregisterGuard, SubAgentTool};

/// 子执行器事件始终带来源，不得把子 StateSnapshot/Done 送入主会话。
struct SkillEventHandler {
    agent_id: String,
    inner: Arc<dyn AgentEventHandler>,
}

impl AgentEventHandler for SkillEventHandler {
    fn on_event(&self, event: AgentEvent) {
        self.inner.on_event(AgentEvent::SubagentEvent {
            source_agent_id: self.agent_id.clone(),
            event: Box::new(event),
        });
    }

    fn wants_llm_call_payload(&self) -> bool {
        self.inner.wants_llm_call_payload()
    }
}

impl SubAgentTool {
    pub async fn invoke_skill(
        &self,
        skill_name: &str,
        instruction: &str,
        agent: Option<&str>,
        model: Option<&str>,
    ) -> Result<(String, String), SkillError> {
        // 使用同一 Agent 定义、工具过滤、权限链和持久化入口；技能全文只作为 Human 输入。
        let mut definition = self.load_agent_def(agent.unwrap_or("general-purpose"), &self.parent_cwd)?;
        if let Some(model) = model {
            definition.frontmatter.model = Some(model.to_string());
        }
        // 技能本身已经展开，避免 agent 定义再次预加载同一技能。
        definition.frontmatter.skills.retain(|name| name != skill_name);
        let build = self.build_agent_from_def(
            &definition,
            skill_name,
            &self.parent_cwd,
            AgentBuildOptions {
                fork: false,
                snapshot_at_message_id: None,
                cascade_cancel: true,
            },
        ).await?;
        let agent_id = build.child_thread_id;
        let cancel = build.cancel_token.unwrap_or_default();
        let _cancel_on_drop = cancel.clone().drop_guard();
        if let Some(register) = &self.register_runtime {
            register(agent_id.clone(), cancel.clone(), "cascade".to_string());
        }
        let _runtime_guard = DeregisterGuard {
            thread_id: agent_id.clone(),
            deregister: self.deregister_runtime.clone(),
        };
        let mut executor = build.builder;
        if let Some(handler) = self.child_handler_factory.as_ref().map(|factory| factory(agent_id.clone()))
            .or_else(|| self.event_handler.clone()) {
            executor = executor.with_event_handler(Arc::new(SkillEventHandler {
                agent_id: agent_id.clone(),
                inner: handler,
            }));
        }
        let mut state = build.state;
        // 主调用身份不应泄漏给子工具；子执行器为每个真实工具分配独立身份。
        let call_id = ToolInvocationContext::current().map(|context| context.tool_call_id);
        tracing::debug!(skill = skill_name, %agent_id, parent_tool_call_id = ?call_id, "开始隔离 Skill 执行");
        let outcome = super::execute_with_timeout(
            &executor,
            AgentInput::text(instruction),
            &mut state,
            cancel.clone(),
            None,
        ).await;
        if let Some(store) = &self.thread_store {
            let _ = store.update_thread_status(
                &agent_id,
                if outcome.is_ok() { "done" } else if cancel.is_cancelled() { "cancelled" } else { "error" },
            ).await;
        }
        let output = outcome?;
        Ok((agent_id, output.text))
    }
}

#[cfg(test)]
#[path = "skill_test.rs"]
mod tests;
