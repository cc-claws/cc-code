use cc_agent::{
    agent::{events::AgentEvent, react::AgentInput, State as _},
    messages::BaseMessage,
};

use super::build_agent::{AgentBuildOptions, CancelPolicy};
use super::format_subagent_result;

impl super::SubAgentTool {
    /// fork = 带主 Agent 上下文的续跑，同样是一种子 Agent。
    ///
    /// 构建/持久化/中间件/工具/事件全部复用 `build_agent_from_def`（`fork: true`），
    /// 这里只保留 fork 真正的两个特例：注入父级消息、prompt 包一层 fork 指令。
    pub(crate) async fn invoke_fork(
        &self,
        prompt: &str,
        cwd: &str,
        description: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Err(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .into(),
            ),
        };
        // fork 借用 general-purpose 定义承载「无 agent.md 覆写」的默认值，
        // 实际人格/工具由 `fork: true` 决定（复用主 Agent 系统提示 + 全量继承工具）。
        let agent_def = self.load_agent_def("general-purpose", cwd)?;
        let build = self
            .build_agent_from_def(
                &agent_def,
                "fork",
                cwd,
                AgentBuildOptions {
                    cancel_policy: CancelPolicy::Cascade,
                    skip_events: false,
                    setup_event_handler: true,
                    fork: true,
                    task_preview: super::subagent_task_preview(description, prompt),
                    // 记录分叉点：历史里能看出这条 fork 线程从父线程哪条消息长出来
                    snapshot_at_message_id: parent_msgs
                        .last()
                        .map(|message| message.id().as_uuid().to_string()),
                },
            )
            .await?;
        let agent_builder = build.builder;
        let mut fork_state = build.state;
        let child_thread_id = build.child_thread_id;
        // instance_id 统一使用 child_thread_id（UUID v7，持久化线程标识）
        let instance_id = child_thread_id.clone();

        // fork 特例 1/2：父级消息作为模型上下文注入子状态（不重复写入 child thread）
        for msg in parent_msgs {
            fork_state.add_message(msg);
        }
        // fork 特例 2/2：prompt 需要包一层 fork 指令
        let fork_directive = crate::subagent::fork::build_fork_directive(prompt);

        // Register AgentRuntime: only when thread_store is present (non-legacy path)
        // Panic-safe: DeregisterGuard ensures deregister runs on drop (panic or early return)
        //
        // child_cancel is linked to parent via child_token(): parent cancel → child_cancel fires.
        // The same child_cancel is passed to execute(), so cascade cancel works correctly.
        let child_cancel = build.cancel_token.unwrap_or_default();
        let _deregister_guard = if self.thread_store.is_some() {
            if let Some(ref register) = self.register_runtime {
                register(
                    child_thread_id.clone(),
                    child_cancel.clone(),
                    "cascade".to_string(),
                );
            }
            super::define::DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: self.deregister_runtime.clone(),
            }
        } else {
            super::define::DeregisterGuard {
                thread_id: child_thread_id.clone(),
                deregister: None,
            }
        };

        let fork_result = agent_builder
            .execute(
                AgentInput::text(fork_directive),
                &mut fork_state,
                Some(child_cancel),
            )
            .await;

        let (output_summary, stopped_is_error) = match &fork_result {
            Ok(output) => (output.text.chars().take(500).collect::<String>(), false),
            Err(e) => (
                format!("Error: {}", e)
                    .chars()
                    .take(500)
                    .collect::<String>(),
                true,
            ),
        };
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStopped {
                agent_name: "fork".to_string(),
                result: output_summary.clone(),
                is_error: stopped_is_error,
                elapsed_ms: 0,
                instance_id: instance_id.clone(),
            });
        }
        self.fire_subagent_lifecycle_hook(
            crate::hooks::types::HookEvent::SubagentStop,
            cwd,
            "fork",
            Some(&output_summary),
        )
        .await;

        match fork_result {
            Ok(output) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "done").await;
                }
                let result_text = format_subagent_result(&output);
                if self.thread_store.is_some() {
                    Ok(format!(
                        "child_thread_id: {}\n{}",
                        child_thread_id, result_text
                    ))
                } else {
                    Ok(result_text)
                }
            }
            Err(cc_agent::error::AgentError::Interrupted) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store
                        .update_thread_status(&child_thread_id, "cancelled")
                        .await;
                }
                Ok("Fork sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                if let Some(ref store) = self.thread_store {
                    let _ = store.update_thread_status(&child_thread_id, "error").await;
                }
                let msg = format!("Fork sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }
}
