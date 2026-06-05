use std::sync::Arc;

use peri_agent::{
    agent::{
        events::AgentEvent, react::AgentInput, state::AgentState, AgentCancellationToken,
        BackgroundTaskResult, ReActAgent, State as _,
    },
    messages::BaseMessage,
    thread::ThreadMeta,
};

use crate::{
    hooks::types::HookEvent,
    subagent::{
        background::{BackgroundTask, BackgroundTaskRegistry, BackgroundTaskStatus},
        SubAgentMiddlewareConfig,
    },
    tools::ArcToolWrapper,
};

use super::{
    build_agent::CancelPolicy, build_subagent_middlewares, fire_subagent_lifecycle_hooks_static,
};

impl super::SubAgentTool {
    pub(crate) async fn invoke_background(
        &self,
        prompt: String,
        subagent_type: Option<String>,
        cwd: String,
        is_fork: bool,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let registry = self
            .background_registry
            .as_ref()
            .ok_or("Background tasks not available: no registry configured")?;

        if registry.active_count() >= 3 {
            return Err("Error: maximum 3 concurrent background tasks reached. \
                 Wait for a running task to complete before starting a new one."
                .into());
        }

        let task_id = format!("bg-{}", uuid::Uuid::new_v4());

        if is_fork {
            return self
                .invoke_background_fork(prompt, cwd, task_id, registry)
                .await;
        }

        let agent_id =
            match &subagent_type {
                Some(id) => id.clone(),
                None => return Err(
                    "Error: background mode requires subagent_type parameter (or use fork: true)"
                        .into(),
                ),
            };

        let agent_def = match self.load_agent_def(&agent_id, &cwd) {
            Ok(a) => a,
            Err(e) => return Err(e.into()),
        };

        let build_result = self
            .build_agent_from_def(
                &agent_def,
                &agent_id,
                &cwd,
                CancelPolicy::Independent,
                true,  // skip_events
                false, // don't setup event handler
            )
            .await?;

        let agent_builder = build_result.builder;
        let agent_name = agent_id.clone();
        let prompt_summary: String = prompt.chars().take(100).collect();

        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();
        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);
        let spawn_bg_sender = self.bg_event_sender.clone();

        let bg_child_thread_id = build_result.child_thread_id.clone();
        let spawn_thread_store = self.thread_store.clone();
        let spawn_child_thread_id = bg_child_thread_id.clone();
        let spawn_deregister_runtime = self.deregister_runtime.clone();
        let has_thread_store = self.thread_store.is_some();

        // Register AgentRuntime before spawning
        // Independent: child_cancel is NOT linked to parent. Only session-level cancel_all_agents cancels it.
        // The same child_cancel is passed to execute() so cancel via active_agents map works.
        let child_cancel = if has_thread_store {
            if let Some(ref register) = self.register_runtime {
                let cc = build_result
                    .cancel_token
                    .clone()
                    .unwrap_or_else(AgentCancellationToken::new);
                register(
                    bg_child_thread_id.clone(),
                    cc.clone(),
                    "independent".to_string(),
                );
                Some(cc)
            } else {
                build_result.cancel_token.clone()
            }
        } else {
            build_result.cancel_token.clone()
        };
        let cancel_token = child_cancel.or(self.cancel.clone());

        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, &cwd, &agent_name, None)
            .await;

        let handle = tokio::spawn(async move {
            let mut state = if let Some(ref store) = spawn_thread_store {
                AgentState::new(&cwd)
                    .with_persistence(Arc::clone(store), spawn_child_thread_id.clone())
            } else {
                AgentState::new(&cwd)
            };
            let start = std::time::Instant::now();

            let result = match agent_builder
                .execute(AgentInput::text(&prompt), &mut state, cancel_token)
                .await
            {
                Ok(output) => {
                    let tool_calls_count = state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count();
                    BackgroundTaskResult {
                        task_id: spawn_task_id.clone(),
                        agent_name: spawn_agent_name.clone(),
                        prompt_summary: spawn_prompt_summary.clone(),
                        success: true,
                        output: output.text,
                        tool_calls_count,
                        duration_ms: start.elapsed().as_millis() as u64,
                        child_thread_id: Some(spawn_child_thread_id.clone()),
                    }
                }
                Err(e) => BackgroundTaskResult {
                    task_id: spawn_task_id.clone(),
                    agent_name: spawn_agent_name.clone(),
                    prompt_summary: spawn_prompt_summary.clone(),
                    success: false,
                    output: e.to_string(),
                    tool_calls_count: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    child_thread_id: Some(spawn_child_thread_id.clone()),
                },
            };

            // Update child thread status
            if let Some(ref store) = spawn_thread_store {
                let status = if result.success { "done" } else { "error" };
                let _ = store
                    .update_thread_status(&spawn_child_thread_id, status)
                    .await;
            }

            spawn_registry.complete(&spawn_task_id, result.clone());

            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            // 通过独立通道发送完成事件（不依赖 event_tx，不受 close_channel 影响）
            if let Some(ref sender) = spawn_bg_sender {
                tracing::info!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    success = result.success,
                    "[bg-diag] bg-task sending BackgroundTaskCompleted via bg_event_tx"
                );
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result));
            } else {
                tracing::warn!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    "[bg-diag] bg-task spawn_bg_sender is None — NOT sent"
                );
            }

            // Deregister AgentRuntime after execution completes
            if let Some(ref deregister) = spawn_deregister_runtime {
                if has_thread_store {
                    deregister(&spawn_child_thread_id);
                }
            }
        });

        registry.register(BackgroundTask {
            id: task_id.clone(),
            agent_name: agent_name.clone(),
            prompt_summary: prompt_summary.clone(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: handle,
        })?;

        // 通知 TUI background agent 启动（递增 background_task_count）。
        // 必须在 registry.register() 成功之后发送，防止注册失败留下幽灵计数。
        tracing::info!(
            task_id = %task_id,
            child_thread_id = %bg_child_thread_id,
            agent_name = %agent_name,
            "[bg-diag] background agent started"
        );
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_name.clone(),
                instance_id: bg_child_thread_id.clone(),
                is_background: true,
            });
        }

        if self.thread_store.is_some() {
            Ok(format!(
                "Background task {} started (thread: {}). You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id, bg_child_thread_id
            ))
        } else {
            Ok(format!(
                "Background task {} started. You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id
            ))
        }
    }

    pub(crate) async fn invoke_background_fork(
        &self,
        prompt: String,
        cwd: String,
        task_id: String,
        registry: &Arc<BackgroundTaskRegistry>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let agent_name = "fork".to_string();
        let prompt_summary: String = prompt.chars().take(100).collect();
        let fork_directive = crate::subagent::fork::build_fork_directive(&prompt);

        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Err(
                "Error: Fork path requires parent message history, but parent_messages is not set"
                    .into(),
            ),
        };

        // Create child thread for background fork
        let bg_fork_child_thread_id = uuid::Uuid::now_v7().to_string();
        if let Some(ref store) = self.thread_store {
            let snapshot_id = parent_msgs.last().map(|m| m.id().as_uuid().to_string());
            let mut child_meta = ThreadMeta::new(&cwd);
            child_meta.id = bg_fork_child_thread_id.clone();
            child_meta.parent_thread_id = self.parent_thread_id.clone();
            child_meta.snapshot_at_message_id = snapshot_id;
            child_meta.hidden = true;
            child_meta.cancel_policy = "independent".to_string();
            child_meta.title = Some(format!("bg-fork-{}", task_id));
            store
                .create_thread(child_meta)
                .await
                .map_err(|e| format!("Failed to create child thread: {}", e))?;
        }

        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);
        for mw in build_subagent_middlewares(SubAgentMiddlewareConfig::for_fork(&cwd)) {
            agent_builder = agent_builder.add_middleware(mw);
        }

        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder.register_tool(Box::new(ArcToolWrapper(Arc::clone(tool))));
        }

        let spawn_registry = Arc::clone(registry);
        let spawn_hooks = Arc::clone(&self.registered_hooks);
        let spawn_bg_sender = self.bg_event_sender.clone();
        let spawn_task_id = task_id.clone();
        let spawn_agent_name = agent_name.clone();
        let spawn_prompt_summary = prompt_summary.clone();
        let spawn_thread_store = self.thread_store.clone();
        let spawn_child_thread_id = bg_fork_child_thread_id.clone();
        let spawn_deregister_runtime = self.deregister_runtime.clone();
        let has_thread_store = self.thread_store.is_some();

        // Register AgentRuntime before spawning
        // Independent: child_cancel is NOT linked to parent. Only session-level cancel_all_agents cancels it.
        // The same child_cancel is passed to execute() so cancel via active_agents map works.
        let child_cancel = if has_thread_store {
            if let Some(ref register) = self.register_runtime {
                let cc = AgentCancellationToken::new();
                register(
                    bg_fork_child_thread_id.clone(),
                    cc.clone(),
                    "independent".to_string(),
                );
                Some(cc)
            } else {
                None
            }
        } else {
            None
        };
        let cancel_token = child_cancel.or(self.cancel.clone());

        self.fire_subagent_lifecycle_hook(HookEvent::SubagentStart, &cwd, &agent_name, None)
            .await;

        let handle = tokio::spawn(async move {
            let mut fork_state = if let Some(ref store) = spawn_thread_store {
                AgentState::new(&cwd)
                    .with_persistence(Arc::clone(store), spawn_child_thread_id.clone())
            } else {
                AgentState::new(&cwd)
            };
            // Inject parent messages for immediate execution
            for msg in parent_msgs {
                fork_state.add_message(msg);
            }
            let start = std::time::Instant::now();

            let result = match agent_builder
                .execute(
                    AgentInput::text(&fork_directive),
                    &mut fork_state,
                    cancel_token,
                )
                .await
            {
                Ok(output) => {
                    let tool_calls_count = fork_state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count();
                    BackgroundTaskResult {
                        task_id: spawn_task_id.clone(),
                        agent_name: spawn_agent_name.clone(),
                        prompt_summary: spawn_prompt_summary.clone(),
                        success: true,
                        output: output.text,
                        tool_calls_count,
                        duration_ms: start.elapsed().as_millis() as u64,
                        child_thread_id: Some(spawn_child_thread_id.clone()),
                    }
                }
                Err(e) => BackgroundTaskResult {
                    task_id: spawn_task_id.clone(),
                    agent_name: spawn_agent_name.clone(),
                    prompt_summary: spawn_prompt_summary.clone(),
                    success: false,
                    output: e.to_string(),
                    tool_calls_count: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    child_thread_id: Some(spawn_child_thread_id.clone()),
                },
            };

            // Update child thread status
            if let Some(ref store) = spawn_thread_store {
                let status = if result.success { "done" } else { "error" };
                let _ = store
                    .update_thread_status(&spawn_child_thread_id, status)
                    .await;
            }

            spawn_registry.complete(&spawn_task_id, result.clone());

            fire_subagent_lifecycle_hooks_static(
                &spawn_hooks,
                HookEvent::SubagentStop,
                &cwd,
                &spawn_agent_name,
                Some(&result.output),
            )
            .await;

            // 通过独立通道发送完成事件（不依赖 event_tx，不受 close_channel 影响）
            if let Some(ref sender) = spawn_bg_sender {
                tracing::info!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    success = result.success,
                    "[bg-diag] bg-task sending BackgroundTaskCompleted via bg_event_tx"
                );
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result));
            } else {
                tracing::warn!(
                    task_id = %spawn_task_id,
                    agent_name = %spawn_agent_name,
                    "[bg-diag] bg-task spawn_bg_sender is None — NOT sent"
                );
            }

            // Deregister AgentRuntime after execution completes
            if let Some(ref deregister) = spawn_deregister_runtime {
                if has_thread_store {
                    deregister(&spawn_child_thread_id);
                }
            }
        });

        registry.register(BackgroundTask {
            id: task_id.clone(),
            agent_name: agent_name.clone(),
            prompt_summary: prompt_summary.clone(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: handle,
        })?;

        // 通知 TUI background agent 启动（递增 background_task_count）。
        // 必须在 registry.register() 成功之后发送，防止注册失败留下幽灵计数。
        tracing::info!(
            task_id = %task_id,
            child_thread_id = %bg_fork_child_thread_id,
            agent_name = %agent_name,
            "[bg-diag] background agent started"
        );
        if let Some(ref handler) = self.event_handler {
            handler.on_event(AgentEvent::SubagentStarted {
                agent_name: agent_name.clone(),
                instance_id: bg_fork_child_thread_id.clone(),
                is_background: true,
            });
        }

        if self.thread_store.is_some() {
            Ok(format!(
                "Background task {} started (thread: {}). You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id, bg_fork_child_thread_id
            ))
        } else {
            Ok(format!(
                "Background task {} started. You will be notified when it completes.                  You can continue with other tasks in the meantime.",
                task_id
            ))
        }
    }
}
