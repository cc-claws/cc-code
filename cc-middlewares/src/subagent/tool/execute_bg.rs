use std::sync::Arc;
use cc_agent::{
    agent::{events::{AgentEvent, AgentEventHandler}, react::AgentInput, steering::SteeringQueue, AgentCancellationToken, BackgroundTaskResult},
    messages::BaseMessage,
};
use crate::{hooks::types::HookEvent, subagent::background::{BackgroundTask, BackgroundTaskSnapshot, BackgroundTaskStatus}};
use super::{build_agent::CancelPolicy, fire_subagent_lifecycle_hooks_static};

/// 后台事件使用会话独立通道，主 prompt 退出后仍保留完整子线程输出。
struct BackgroundEventHandler {
    task_id: String,
    instance_id: String,
    registry: Arc<crate::subagent::BackgroundTaskRegistry>,
    sender: Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>,
    fallback: Option<Arc<dyn AgentEventHandler>>,
}
impl AgentEventHandler for BackgroundEventHandler {
    fn on_event(&self, event: AgentEvent) {
        self.registry.record_event(&self.task_id, &event);
        let event = AgentEvent::SubagentEvent { source_agent_id: self.instance_id.clone(), event: Box::new(event) };
        if let Some(sender) = &self.sender { let _ = sender.send(event); }
        else if let Some(handler) = &self.fallback { handler.on_event(event); }
    }
}

impl super::SubAgentTool {
    pub(crate) async fn invoke_background(
        &self, prompt: String, subagent_type: Option<String>, cwd: String, is_fork: bool,
        timeout_secs: Option<u64>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let registry = self.background_registry.as_ref().ok_or("Background tasks not available: no registry configured")?;
        let agent_name = if is_fork { "fork".to_string() } else {
            subagent_type.ok_or("Error: background mode requires subagent_type parameter (or use fork: true)")?
        };
        let parent_messages = if is_fork {
            Some(self.parent_messages.as_ref().ok_or("Error: Fork path requires parent message history, but parent_messages is not set")?.read().clone())
        } else { None };
        // fork 复用同一构造/持久化/权限路径，仅上下文来源不同。
        let agent_def = self.load_agent_def(if is_fork { "general-purpose" } else { &agent_name }, &cwd)?;
        let build = self.build_agent_from_def(&agent_def, &agent_name, &cwd, CancelPolicy::Independent, true, false).await?;
        let thread_id = build.child_thread_id;
        let task_id = format!("bg-{}", uuid::Uuid::new_v4());
        let cancel = build.cancel_token.unwrap_or_else(AgentCancellationToken::new);
        let mailbox = SteeringQueue::default();
        let model = agent_def.frontmatter.model.clone();
        let preview = prompt.lines().next().unwrap_or(&prompt).chars().take(40).collect::<String>();
        let handler = Arc::new(BackgroundEventHandler {
            task_id: task_id.clone(), instance_id: thread_id.clone(), registry: Arc::clone(registry),
            sender: self.bg_event_sender.clone(), fallback: self.event_handler.clone(),
        });
        let agent = build.builder.with_steering(mailbox.clone()).with_event_handler(handler.clone());
        let mut state = build.state;
        if let Some(messages) = parent_messages {
            // fork 的祖先消息仅作为模型上下文，不重复写入 child thread。
            state.messages.extend(messages);
        }
        let directive = if is_fork { crate::subagent::fork::build_fork_directive(&prompt) } else { prompt.clone() };
        let snapshot = BackgroundTaskSnapshot {
            task_id: task_id.clone(), instance_id: thread_id.clone(), agent_name: agent_name.clone(),
            task_preview: preview.clone(), status: BackgroundTaskStatus::Running, elapsed_ms: 0,
            total_steps: 0, tokens_used: 0, current_activity: preview.clone(), model: model.clone(), timeout_secs, result: None,
        };
        let spawn_registry = registry.clone();
        let spawn_id = task_id.clone();
        let spawn_thread = thread_id.clone();
        let spawn_name = agent_name.clone();
        let spawn_preview = preview.clone();
        let spawn_cancel = cancel.clone();
        let store = self.thread_store.clone();
        let hooks = self.registered_hooks.clone();
        let sender = self.bg_event_sender.clone();
        let fallback = self.event_handler.clone();
        let deregister = self.deregister_runtime.clone();
        // 先原子注册，再打开启动闸门；快速完成和并发抢槽不会生成幽灵任务。
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            if start_rx.await.is_err() { return; }
            let _guard = super::define::DeregisterGuard { thread_id: spawn_thread.clone(), deregister };
            let start = std::time::Instant::now();
            handler.on_event(AgentEvent::StateSnapshot(vec![BaseMessage::human(prompt)]));
            let result = super::execute_with_timeout(&agent, AgentInput::text(directive), &mut state, spawn_cancel, timeout_secs).await;
            let success = result.is_ok();
            let output = match result { Ok(output) => output.text, Err(error) => error.to_string() };
            let result = BackgroundTaskResult {
                task_id: spawn_id.clone(), agent_name: spawn_name.clone(), prompt_summary: spawn_preview,
                success, output, tool_calls_count: state.messages.iter().filter(|m| matches!(m, BaseMessage::Tool { .. })).count(),
                duration_ms: start.elapsed().as_millis() as u64, child_thread_id: Some(spawn_thread.clone()),
            };
            spawn_registry.complete(&spawn_id, result.clone());
            if let Some(store) = store {
                let stopped = spawn_registry.status(&spawn_id).is_ok_and(|s| s.status == BackgroundTaskStatus::Stopped);
                let _ = store.update_thread_status(&spawn_thread, if success { "done" } else if stopped { "cancelled" } else { "error" }).await;
            }
            let stopped = AgentEvent::SubagentStopped { agent_name: spawn_name.clone(), instance_id: spawn_thread,
                result: result.output.clone(), is_error: !success, elapsed_ms: result.duration_ms };
            if let Some(sender) = &sender {
                let _ = sender.send(stopped);
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result.clone()));
            } else if let Some(handler) = fallback {
                handler.on_event(stopped);
                handler.on_event(AgentEvent::BackgroundTaskCompleted(result.clone()));
            }
            fire_subagent_lifecycle_hooks_static(&hooks, HookEvent::SubagentStop, &cwd, &spawn_name, Some(&result.output)).await;
        });
        registry.register(BackgroundTask { id: task_id.clone(), agent_name: agent_name.clone(), prompt_summary: preview.clone(),
            status: BackgroundTaskStatus::Running, started_at: std::time::Instant::now(), abort_handle: handle })?;
        registry.attach_runtime(snapshot, cancel.clone(), mailbox);
        if let Some(register) = &self.register_runtime { register(thread_id.clone(), cancel, "independent".into()); }
        let started = AgentEvent::SubagentStarted { agent_name, instance_id: thread_id.clone(), is_background: true,
            task_preview: preview, timeout_secs, model };
        // 与过程事件同一发送通道，开始事件必定排在首个增量之前。
        if let Some(sender) = &self.bg_event_sender { let _ = sender.send(started); }
        else if let Some(handler) = &self.event_handler { handler.on_event(started); }
        let _ = start_tx.send(());
        Ok(format!("Background task {task_id} started (thread: {thread_id}). You will be notified when it completes. Use Agent(action=\"status\" or \"cancel\", task_id=\"{task_id}\") to manage it."))
    }
}
