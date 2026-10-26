use super::fire_subagent_lifecycle_hooks_static;
use crate::{
    hooks::types::HookEvent,
    subagent::background::{BackgroundTask, BackgroundTaskSnapshot, BackgroundTaskStatus},
};
use cc_agent::{
    agent::{
        events::{AgentEvent, AgentEventHandler},
        react::AgentInput,
        steering::SteeringQueue,
        AgentCancellationToken, BackgroundTaskResult,
    },
    messages::BaseMessage,
};
use std::sync::Arc;

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
        let progress = self.progress_event(&event);
        let event = AgentEvent::SubagentEvent {
            source_agent_id: self.instance_id.clone(),
            event: Box::new(event),
        };
        if let Some(sender) = &self.sender {
            // 进度先于该步事件发出，消费端（TUI 的 total_steps）才能与工具流同步。
            if let Some(progress) = progress {
                let _ = sender.send(progress);
            }
            let _ = sender.send(event);
        } else if let Some(handler) = &self.fallback {
            if let Some(progress) = progress {
                handler.on_event(progress);
            }
            handler.on_event(event);
        }
    }

    /// 包装型 handler 必须转述内部 handler 的载荷需求声明，但**只在事件真的会走
    /// 内部 handler 时**才转述：`sender` 存在时事件被投进会话独立通道，
    /// `fallback` 根本收不到，此时声明 `true` 只会让子执行器为每次 LLM 调用
    /// 深拷贝整份历史（`cc-agent` 的 `llm_step`）——而下游（ACP mapper /
    /// TransportEventSink）会丢弃 `LlmCallStart`，Langfuse 也只从**父**事件泵
    /// 取样本，拿不到子 Agent 的载荷。所以那是一次纯浪费的 O(历史) 分配。
    fn wants_llm_call_payload(&self) -> bool {
        self.sender.is_none()
            && self
                .fallback
                .as_ref()
                .is_some_and(|handler| handler.wants_llm_call_payload())
    }
}

impl BackgroundEventHandler {
    /// ToolStart → 轻量 `SubagentProgress`（步数取自 registry 已累计的快照）。
    ///
    /// 基线由独立的 `BgProgressHandler` 发送；本 PR 合并事件处理器时漏掉了这一步，
    /// 导致 `peri/subagent_lifecycle: progress` 契约与 `RunningBgAgent.total_steps`
    /// 在生产路径上恒为 0（只有测试还覆盖着）。
    fn progress_event(&self, event: &AgentEvent) -> Option<AgentEvent> {
        if !matches!(event, AgentEvent::ToolStart { .. }) {
            return None;
        }
        let total_steps = self.registry.status(&self.task_id).ok()?.total_steps;
        Some(AgentEvent::SubagentProgress {
            instance_id: self.instance_id.clone(),
            total_steps,
        })
    }
}

impl super::SubAgentTool {
    pub(crate) async fn invoke_background(
        &self,
        prompt: String,
        subagent_type: Option<String>,
        cwd: String,
        is_fork: bool,
        timeout_secs: Option<u64>,
        description: Option<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 先做入参校验，再做运行环境校验：漏参是模型的问题，应给出可操作的错误，
        // 而不是被「本会话没有后台注册表」这类环境错误盖住。
        let agent_name = if is_fork {
            "fork".to_string()
        } else {
            subagent_type
                .ok_or("Error: subagent_type is required (or set fork: true for fork mode)")?
        };
        let parent_messages = if is_fork {
            Some(self.parent_messages.as_ref().ok_or("Error: Fork path requires parent message history, but parent_messages is not set")?.read().clone())
        } else {
            None
        };
        // 入参侧校验全部前置（缺参 / agent 定义不存在），环境侧校验（注册表）放最后，
        // 否则模型会拿到「没有后台注册表」这种它无法处理的错误。
        let agent_def = self.load_agent_def(
            if is_fork {
                "general-purpose"
            } else {
                &agent_name
            },
            &cwd,
        )?;
        let registry = self.background_registry.as_ref().ok_or(
            "Error: sub-agents run in the background only, but no background registry is configured in this session",
        )?;
        let build = self
            .build_agent_from_def(
                &agent_def,
                &agent_name,
                &cwd,
                super::build_agent::AgentBuildOptions {
                    fork: is_fork,
                    cascade_cancel: false,
                    // 记录分叉点：历史里能看出这条 fork 线程从父线程哪条消息长出来
                    snapshot_at_message_id: parent_messages
                        .as_ref()
                        .and_then(|messages| messages.last())
                        .map(|message| message.id().as_uuid().to_string()),
                },
            )
            .await?;
        let thread_id = build.child_thread_id;
        let task_id = format!("bg-{}", uuid::Uuid::new_v4());
        let cancel = build
            .cancel_token
            .unwrap_or_else(AgentCancellationToken::new);
        let mailbox = SteeringQueue::default();
        let model = agent_def.frontmatter.model.clone();
        let preview = super::subagent_task_preview(description.as_deref(), &prompt);
        let handler = Arc::new(BackgroundEventHandler {
            task_id: task_id.clone(),
            instance_id: thread_id.clone(),
            registry: Arc::clone(registry),
            sender: self.bg_event_sender.clone(),
            fallback: self.event_handler.clone(),
        });
        let agent = build
            .builder
            .with_steering(mailbox.clone())
            .with_event_handler(handler.clone());
        let mut state = build.state;
        if let Some(messages) = parent_messages {
            // fork 的祖先消息仅作为模型上下文，不重复写入 child thread。
            state.messages.extend(messages);
        }
        let directive = if is_fork {
            crate::subagent::fork::build_fork_directive(&prompt)
        } else {
            prompt.clone()
        };
        let snapshot = BackgroundTaskSnapshot {
            task_id: task_id.clone(),
            instance_id: thread_id.clone(),
            agent_name: agent_name.clone(),
            task_preview: preview.clone(),
            status: BackgroundTaskStatus::Running,
            elapsed_ms: 0,
            total_steps: 0,
            tokens_used: 0,
            current_activity: preview.clone(),
            model: model.clone(),
            timeout_secs,
            result: None,
        };
        let spawn_registry = registry.clone();
        let spawn_id = task_id.clone();
        let spawn_thread = thread_id.clone();
        let spawn_name = agent_name.clone();
        let spawn_preview = preview.clone();
        let spawn_cancel = cancel.clone();
        let spawn_cwd = cwd.clone();
        let store = self.thread_store.clone();
        let hooks = self.registered_hooks.clone();
        let sender = self.bg_event_sender.clone();
        let fallback = self.event_handler.clone();
        let deregister = self.deregister_runtime.clone();
        let task_prompt = prompt.clone();
        // 先原子注册，再打开启动闸门；快速完成和并发抢槽不会生成幽灵任务。
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            if start_rx.await.is_err() {
                return;
            }
            let _guard = super::define::DeregisterGuard {
                thread_id: spawn_thread.clone(),
                deregister,
            };
            let start = std::time::Instant::now();
            handler.on_event(AgentEvent::StateSnapshot(vec![BaseMessage::human(
                task_prompt,
            )]));
            // 执行体再包一层 task：子 Agent 内部 panic 会被 JoinError 捕获，从而仍能
            // 走完「complete → 落库 → 发终态事件 → 触发 hook」。
            // 不做这层保护的话，panic 会让注册表槽位永久占用（并发额度泄漏）、抽屉行
            // 卡在 Running、`has_running_subagents()` 恒真导致主 Agent 自动续接被阻塞。
            let execution = tokio::spawn(async move {
                let outcome = super::execute_with_timeout(
                    &agent,
                    AgentInput::text(directive),
                    &mut state,
                    spawn_cancel,
                    timeout_secs,
                )
                .await;
                (outcome, state)
            });
            let (success, output, tool_calls_count) = match execution.await {
                Ok((Ok(output), state)) => (
                    true,
                    output.text,
                    state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count(),
                ),
                Ok((Err(error), state)) => (
                    false,
                    error.to_string(),
                    state
                        .messages
                        .iter()
                        .filter(|m| matches!(m, BaseMessage::Tool { .. }))
                        .count(),
                ),
                Err(join_error) => (false, format!("Sub-agent panicked: {join_error}"), 0),
            };
            let result = BackgroundTaskResult {
                task_id: spawn_id.clone(),
                agent_name: spawn_name.clone(),
                prompt_summary: spawn_preview,
                success,
                output,
                tool_calls_count,
                duration_ms: start.elapsed().as_millis() as u64,
                child_thread_id: Some(spawn_thread.clone()),
            };
            spawn_registry.complete(&spawn_id, result.clone());
            if let Some(store) = store {
                let stopped = spawn_registry
                    .status(&spawn_id)
                    .is_ok_and(|s| s.status == BackgroundTaskStatus::Stopped);
                let _ = store
                    .update_thread_status(
                        &spawn_thread,
                        if success {
                            "done"
                        } else if stopped {
                            "cancelled"
                        } else {
                            "error"
                        },
                    )
                    .await;
            }
            let stopped = AgentEvent::SubagentStopped {
                agent_name: spawn_name.clone(),
                instance_id: spawn_thread,
                result: result.output.clone(),
                is_error: !success,
                elapsed_ms: result.duration_ms,
            };
            if let Some(sender) = &sender {
                let _ = sender.send(stopped);
                let _ = sender.send(AgentEvent::BackgroundTaskCompleted(result.clone()));
            } else if let Some(handler) = fallback {
                handler.on_event(stopped);
                handler.on_event(AgentEvent::BackgroundTaskCompleted(result.clone()));
            }
            fire_subagent_lifecycle_hooks_static(
                &hooks,
                HookEvent::SubagentStop,
                &spawn_cwd,
                &spawn_name,
                Some(&result.output),
            )
            .await;
        });
        if let Err(error) = registry.register(BackgroundTask {
            id: task_id.clone(),
            agent_name: agent_name.clone(),
            prompt_summary: preview.clone(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: handle,
        }) {
            // 并发闸拒绝：子线程已在上面的构建设档阶段创建，不清理就会在 DB 里留下
            // 永久 "running" 的隐藏行（既查不到也清不掉）。注册失败即视为该次委派失败。
            if let Some(store) = &self.thread_store {
                let _ = store.update_thread_status(&thread_id, "error").await;
            }
            return Err(error.into());
        }
        registry.attach_runtime(snapshot, cancel.clone(), mailbox);
        if let Some(register) = &self.register_runtime {
            register(thread_id.clone(), cancel, "independent".into());
        }
        let started = AgentEvent::SubagentStarted {
            agent_name: agent_name.clone(),
            instance_id: thread_id.clone(),
            is_background: true,
            task_preview: preview,
            timeout_secs,
            model,
            prompt: Some(prompt.clone()),
        };
        // 与过程事件同一发送通道，开始事件必定排在首个增量之前。
        if let Some(sender) = &self.bg_event_sender {
            let _ = sender.send(started);
        } else if let Some(handler) = &self.event_handler {
            handler.on_event(started);
        }
        // SubagentStart hook：原先只在前台同步路径触发，后台任务漏了（前台已移除）
        fire_subagent_lifecycle_hooks_static(
            &self.registered_hooks,
            HookEvent::SubagentStart,
            &cwd,
            &agent_name,
            None,
        )
        .await;
        let _ = start_tx.send(());
        // 只描述**真实存在**的管理方式：本工具没有 action/task_id 参数（曾经的提示让模型
        // 去调 `Agent(action="cancel")`，而 schema 里根本没有该参数，模型照做只会报错）。
        Ok(format!(
            "Background task {task_id} started (thread: {thread_id}). You will be notified when it completes; keep working on other tasks in the meantime. The task is tracked in the bottom drawer, where the user can stop one (x) or all (ctrl+x ctrl+k)."
        ))
    }
}
