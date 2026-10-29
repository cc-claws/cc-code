//! Agent event dispatch — the main `handle_agent_event` dispatcher routes
//! individual AgentEvent variants to specialized handlers.
//! Extracted sub-modules:
//!   acp_bridge.rs — ACP notification bridge
//!   lifecycle.rs — cleanup, done, interrupted, error
//!   subagent.rs  — token usage, subagent start
//!   polling.rs   — poll_agent, poll_background_events, poll_cron_triggers

use super::{agent_events_bg::BackgroundTaskResult, *};
mod acp_bridge;
mod lifecycle;
mod output_refresh;
mod polling;
mod rewind;
mod subagent;

impl App {
    /// 到期清除各 Agent 的系统提示。返回是否有变化（供事件循环决定是否重绘）。
    ///
    /// 提示的最短可见时长见 `agent_comm::SYSTEM_NOTICE_MIN_VISIBLE_MS`；这里是它的
    /// 到期出口——事件循环每次空转都会走到，与 agent 是否仍在运行无关，
    /// 因此轮次结束后提示也能按时消失而不残留。
    pub fn expire_system_notices(&mut self) -> bool {
        let now = std::time::Instant::now();
        let session = self.session_mgr.current_mut();
        let mut changed = session.agent.expire_system_notice(now);
        for sub in session.subagent_sessions.values_mut() {
            changed |= sub.agent.expire_system_notice(now);
        }
        changed
    }

    pub(crate) fn handle_agent_event(&mut self, event: AgentEvent) -> (bool, bool, bool) {
        // 标准 ACP 消息同样经过此守卫；不依赖全局 depth 或最后启动的任务。
        let source = match &event {
            AgentEvent::ToolPending {
                source_agent_id, ..
            }
            | AgentEvent::ToolStart {
                source_agent_id, ..
            }
            | AgentEvent::ToolOutput {
                source_agent_id, ..
            }
            | AgentEvent::ToolEnd {
                source_agent_id, ..
            }
            | AgentEvent::AssistantChunk {
                source_agent_id, ..
            } => source_agent_id.clone(),
            _ => None,
        };
        if let Some(id) = source {
            return self.handle_subagent_event(&id, event);
        }
        match event {
            AgentEvent::SubagentEvent {
                source_agent_id,
                event,
            } => self.handle_subagent_event(&source_agent_id, *event),
            AgentEvent::SubagentInputResult {
                instance_id,
                input_id,
                error,
            } => {
                if let Some(sub) = self
                    .session_mgr
                    .current_mut()
                    .subagent_sessions
                    .get_mut(&instance_id)
                {
                    if let Some(error) = error {
                        sub.pending_inputs.retain(|(id, _)| *id != input_id);
                        sub.notices.push(format!("Message failed: {error}"));
                        sub.rebuild();
                    }
                }
                self.render_rebuild();
                (true, false, false)
            }
            AgentEvent::ShellCommandCompleted(record) => {
                self.handle_shell_command_completed(record)
            }
            // 后台 shell 完成通知由 poll_background_shell_events 直接 submit_message，
            // 不经此 event 路径；保留 arm 保证 match 穷尽。
            AgentEvent::BackgroundShellCompleted { .. } => (false, false, false),
            // watchdog stall 警告：生成通知注入对话流（idle 直接 submit，推理中暂存 pending）
            AgentEvent::BackgroundShellStalled {
                task_id,
                command,
                description,
                last_output,
            } => {
                let notif = super::background_shell::shell_stalled_notification(
                    &task_id,
                    &command,
                    description.as_deref(),
                    &last_output,
                );
                let owner_session_id = self
                    .session_mgr
                    .current()
                    .agent_shells
                    .iter()
                    .find(|slot| slot.task_id == task_id)
                    .and_then(|slot| slot.owner_session_id.clone())
                    .or_else(|| {
                        self.session_mgr
                            .current()
                            .background_shells
                            .iter()
                            .find(|shell| shell.id == task_id)
                            .and_then(|shell| shell.owner_session_id.clone())
                    });
                // 与完成通知共用合并注入逻辑：idle 合并为 1 轮 turn，推理中入队待 drain
                self.inject_bg_shell_notifications(vec![super::PendingBgShellNotification {
                    owner_session_id,
                    content: notif,
                }]);
                (true, false, false)
            }
            AgentEvent::SubAgentStart {
                agent_id,
                instance_id,
                task_preview,
                is_background,
                prompt,
            } => self.handle_subagent_start(
                agent_id,
                instance_id,
                task_preview,
                is_background,
                prompt,
            ),
            AgentEvent::SubagentLifecycle {
                agent_name: _,
                started,
            } => {
                if started {
                    // SubAgent 实际开始执行：切换为工具使用模式（方案 A：verb 保持整轮固定）
                    self.session_mgr
                        .current_mut()
                        .spinner_state
                        .set_mode_keep_verb(cc_widgets::SpinnerMode::ToolUse);
                } else {
                    // SubAgent 执行结束：恢复 spinner 为响应模式（verb 不变）
                    self.session_mgr
                        .current_mut()
                        .spinner_state
                        .set_mode_keep_verb(cc_widgets::SpinnerMode::Responding);
                }
                // 触发 rebuild 刷新 SubAgentGroup 卡片显示
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::SubAgentEnd {
                result,
                is_error,
                agent_id,
                instance_id,
            } => {
                if let Some(ref id) = instance_id {
                    self.handle_subagent_lifecycle(&serde_json::json!({
                        "event": "stopped", "instanceId": id, "result": result,
                        "isError": is_error, "agentType": agent_id,
                    }));
                }
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::SubAgentEnd {
                        result,
                        is_error,
                        agent_id,
                        instance_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::SubagentProgress {
                instance_id,
                total_steps,
            } => {
                if let Some(agent) = self
                    .session_mgr
                    .current_mut()
                    .background_agents
                    .iter_mut()
                    .find(|a| a.instance_id == instance_id)
                {
                    agent.total_steps = total_steps;
                }
                for vm in &mut self.session_mgr.current_mut().messages.view_messages {
                    if let crate::ui::message_view::MessageViewModel::SubAgentGroup {
                        instance_id: Some(ref iid),
                        total_steps: ref mut steps,
                        ..
                    } = vm
                    {
                        if iid == &instance_id {
                            *steps = total_steps;
                            vm.recompute_hash();
                            break;
                        }
                    }
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::ContextWarning {
                used_tokens: _,
                total_tokens,
                percentage: _,
            } => {
                // 子 Agent 的 ContextWarning 不应触发父 Agent 的 auto-compact
                // 从核心层同步 context_window（核心层通过 model.context_window() 获取正确值）
                let cw = total_tokens as u32;
                if cw > 0 && self.session_mgr.current_mut().agent.context_window != cw {
                    tracing::debug!(
                        old = self.session_mgr.current_mut().agent.context_window,
                        new = cw,
                        "context_window updated from core layer"
                    );
                    self.session_mgr.current_mut().agent.context_window = cw;
                }
                (true, false, false)
            }
            AgentEvent::OAuthAuthorizationNeeded {
                server_name,
                authorization_url,
                callback_tx,
            } => self.handle_oauth_needed(server_name, authorization_url, callback_tx),
            AgentEvent::OAuthAuthorizationCompleted { server_name } => {
                self.handle_oauth_completed(server_name)
            }
            AgentEvent::OAuthAuthorizationFailed { server_name, error } => {
                self.handle_oauth_failed(server_name, error)
            }
            AgentEvent::McpActionCompleted {
                server_name,
                action,
                success,
            } => self.handle_mcp_action_completed(server_name, action, success),
            AgentEvent::PluginActionCompleted {
                plugin_id,
                action,
                success,
                message,
            } => self.handle_plugin_action_completed(plugin_id, action, success, message),
            AgentEvent::TokenUsageUpdate {
                usage,
                model: _model,
                stop_reason: _,
            } => self.handle_token_usage_update(usage),
            pending @ AgentEvent::ToolPending { .. } => {
                self.session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(pending);
                self.session_mgr.current_mut().agent.tool_status_dirty = true;
                (true, false, false)
            }
            AgentEvent::ToolStart {
                tool_call_id,
                name,
                display,
                args,
                input,
                source_agent_id,
            } => {
                self.session_mgr.current_mut().agent.retry_status = None;
                // 提示未满最短可见时长时有意保留（见 SYSTEM_NOTICE_MIN_VISIBLE_MS）
                self.session_mgr
                    .current_mut()
                    .agent
                    .dismiss_system_notice(std::time::Instant::now());
                self.session_mgr.current_mut().agent.agent_replied = true;
                self.session_mgr.current_mut().agent.tool_call_count += 1;
                // 记录当前执行中的工具
                {
                    let args_summary: String = super::tool_display::sanitize_display_text(&args)
                        .chars()
                        .take(40)
                        .collect();
                    let active = super::agent_comm::ActiveToolInfo {
                        tool_call_id: tool_call_id.clone(),
                        name: name.clone(),
                        display: display.clone(),
                        args_summary: args_summary.clone(),
                    };
                    let agent = &mut self.session_mgr.current_mut().agent;
                    agent.running_tools.push(active.clone());
                    agent.active_tool = Some(active);
                    // 状态栏「最近工具」：最新在最前，超出容量丢最老
                    agent.push_recent_tool(super::agent_comm::RecentToolEntry {
                        tool_call_id: tool_call_id.clone(),
                        display: display.clone(),
                        args_summary,
                        running: true,
                        started_at: std::time::Instant::now(),
                        visible_until: None,
                    });
                }
                // 跨切面：spinner —— 思考段结束，进入工具执行
                // 方案 A（verb 整轮固定）：keep_verb，保持一次性选定的动词
                self.session_mgr.current_mut().spinner_state.end_thinking();
                self.session_mgr
                    .current_mut()
                    .spinner_state
                    .set_mode_keep_verb(cc_widgets::SpinnerMode::ToolUse);
                // Pipeline：创建 ToolBlock / 路由进 SubAgentGroup
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::ToolStart {
                        tool_call_id,
                        name,
                        display,
                        args,
                        input,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                if !self.session_mgr.current().agent.tool_status_dirty {
                    self.request_rebuild();
                }
                (true, false, false)
            }
            output @ AgentEvent::ToolOutput { .. } => {
                self.session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(output);
                self.session_mgr.current_mut().agent.tool_output_dirty = true;
                (true, false, false)
            }
            AgentEvent::ToolEnd {
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id,
            } => {
                // 清除执行中工具，累加工具统计
                {
                    let agent = &mut self.session_mgr.current_mut().agent;
                    agent
                        .running_tools
                        .retain(|tool| tool.tool_call_id != tool_call_id);
                    agent.active_tool = agent.running_tools.last().cloned();
                    // 状态栏「最近工具」：标记结束并进入最短可见停留
                    agent.finish_recent_tool(&tool_call_id, std::time::Instant::now());
                }
                if !name.is_empty() {
                    let count = *self
                        .session_mgr
                        .current_mut()
                        .agent
                        .session_tool_stats
                        .entry(name.clone())
                        .or_insert(0)
                        + 1;
                    self.session_mgr
                        .current_mut()
                        .agent
                        .session_tool_stats
                        .insert(name.clone(), count);
                    tracing::debug!(tool = %name, count, "session_tool_stats updated");
                }
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::ToolEnd {
                        tool_call_id,
                        name,
                        output,
                        is_error,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                if !self.session_mgr.current().agent.tool_status_dirty {
                    self.request_rebuild();
                }
                (true, false, false)
            }
            AgentEvent::AssistantChunk {
                chunk,
                source_agent_id,
            } => {
                self.session_mgr.current_mut().agent.retry_status = None;
                // 提示未满最短可见时长时有意保留（见 SYSTEM_NOTICE_MIN_VISIBLE_MS）
                self.session_mgr
                    .current_mut()
                    .agent
                    .dismiss_system_notice(std::time::Instant::now());
                self.session_mgr.current_mut().agent.agent_replied = true;
                // 跨切面：spinner
                // 跨切面：spinner —— 思考段结束（开始输出文本）
                // 方案 A：verb 整轮固定，此处只切模式不改词
                self.session_mgr.current_mut().spinner_state.end_thinking();
                self.session_mgr
                    .current_mut()
                    .spinner_state
                    .set_mode_keep_verb(cc_widgets::SpinnerMode::Responding);
                // Pipeline：路由到 SubAgentGroup 或父 Agent AssistantBubble
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::AssistantChunk {
                        chunk,
                        source_agent_id,
                    });
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::Done => self.handle_done(),
            AgentEvent::Interrupted => self.handle_interrupted(),
            AgentEvent::Error(ref e) => self.handle_error(e),
            AgentEvent::InteractionRequest { ctx, response_tx } => {
                self.handle_interaction_request(ctx, response_tx)
            }
            AgentEvent::TodoUpdate(todos) => {
                self.session_mgr.current_mut().todo_items = todos;
                (true, false, false)
            }
            AgentEvent::StateSnapshot(msgs) => {
                // 子 Agent 的 StateSnapshot 不应污染父 Agent 的 origin_messages，
                // 否则子 Agent 的全部内部消息会混入父 Agent 的对话历史和持久化。
                if msgs
                    .iter()
                    .any(|message| matches!(message, cc_agent::messages::BaseMessage::Human { .. }))
                {
                    // 执行中的补充消息已经进入历史，中断时不能撤回本轮开头的输入。
                    self.session_mgr.current_mut().messages.last_submitted_text = None;
                }
                self.session_mgr
                    .current_mut()
                    .agent
                    .origin_messages
                    .extend(msgs.clone());
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::StateSnapshot(msgs));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                self.request_rebuild();
                (true, false, false)
            }
            AgentEvent::CompactCompleted {
                summary,
                files,
                skills,
                micro_cleared,
                messages,
            } => self.handle_compact_completed(summary, files, skills, micro_cleared, messages),
            AgentEvent::CompactStarted { manual } => self.handle_compact_started(manual),
            AgentEvent::CompactError(msg) => self.handle_compact_error(msg),
            AgentEvent::HooksCompleted {
                event,
                ran,
                duration_ms,
                failures,
                is_async,
                tool_call_id,
            } => {
                if let Some(tool_call_id) = tool_call_id {
                    // 工具事件的 hook 行挂在触发它的 ToolBlock 上：随工具块一起折叠、
                    // 滚动。管线负责在尾部重建时重新贴（见 MessagePipeline::push_hook_rows）。
                    let rows = hook_rows_for(&event, ran, duration_ms, &failures, is_async);
                    self.session_mgr
                        .current_mut()
                        .messages
                        .pipeline
                        .push_hook_rows(&tool_call_id, rows);
                    self.request_rebuild();
                } else if failures.is_empty() {
                    // 阻断/失败在普通模式也要显示；纯成功汇总只在详细模式出现。
                    let text = if is_async {
                        format!("Async hook {event} completed")
                    } else {
                        format!("{ran} {event} hooks ran · {duration_ms}ms")
                    };
                    self.push_hook_system_note(
                        text,
                        crate::ui::message_view::NoteTone::Neutral,
                        true,
                    );
                } else {
                    for failure in &failures {
                        let (verb, tone) = if failure.kind == "block" {
                            ("blocked", crate::ui::message_view::NoteTone::Warning)
                        } else {
                            ("failed", crate::ui::message_view::NoteTone::Error)
                        };
                        self.push_hook_system_note(
                            format!("{event} hook {verb}: {}", failure.reason),
                            tone,
                            false,
                        );
                    }
                }
                (true, false, false)
            }
            AgentEvent::RewindCompleted { summary, messages } => {
                self.handle_rewind_completed(summary, messages)
            }
            AgentEvent::RecapCompleted { text } => self.handle_recap_completed(text),
            AgentEvent::RecapError(msg) => self.handle_recap_error(msg),
            AgentEvent::LlmRetrying {
                attempt,
                max_attempts,
                delay_ms,
                error,
            } => {
                // 子 Agent 的 LlmRetrying 不应覆盖父 Agent 的 retry_status 显示
                self.session_mgr.current_mut().agent.retry_status =
                    Some(super::agent_comm::RetryStatus {
                        attempt,
                        max_attempts,
                        delay_ms,
                        error,
                    });
                (true, false, false)
            }
            AgentEvent::SystemNotice { level, message } => {
                // 子 Agent 的提示不覆盖父 Agent 展示（与 LlmRetrying 同策略）
                self.session_mgr.current_mut().agent.system_notice = Some(
                    super::agent_comm::SystemNotice::new(level, message, std::time::Instant::now()),
                );
                (true, false, false)
            }
            AgentEvent::AiReasoning(text) => {
                // 跨切面：进入思考段（幂等；重复 reasoning chunk 不重置起点）
                // 方案 A：verb 保持整轮固定，仅切 mode 到 Thinking
                self.session_mgr
                    .current_mut()
                    .spinner_state
                    .begin_thinking();
                self.session_mgr
                    .current_mut()
                    .spinner_state
                    .set_mode_keep_verb(cc_widgets::SpinnerMode::Thinking);
                let actions = self
                    .session_mgr
                    .current_mut()
                    .messages
                    .pipeline
                    .handle_event(AgentEvent::AiReasoning(text));
                for action in actions {
                    self.apply_pipeline_action(action);
                }
                (true, false, false)
            }
            AgentEvent::BackgroundTaskCompleted {
                task_id,
                agent_name,
                success,
                output,
                tool_calls_count,
                duration_ms,
                child_thread_id,
            } => self.handle_background_task_completed(BackgroundTaskResult {
                task_id,
                agent_name,
                success,
                output,
                tool_calls_count,
                duration_ms,
                child_thread_id,
            }),
            AgentEvent::LspDiagnostics {
                errors,
                warnings,
                files_with_errors,
            } => {
                self.session_mgr.current_mut().agent.lsp_errors = errors;
                self.session_mgr.current_mut().agent.lsp_warnings = warnings;
                self.session_mgr.current_mut().agent.lsp_files_with_errors = files_with_errors;
                (true, false, false)
            }
            AgentEvent::ThreadTitleGenerated {
                session_id,
                thread_id,
                title,
            } => {
                let is_current = self.session_mgr.current().metadata.session_id == session_id;
                let user_renamed = (is_current && self.session_mgr.current().metadata.user_renamed)
                    || thread_id
                        .as_ref()
                        .is_some_and(|id| self.global_ui.manual_thread_titles.read().contains(id));
                // 校验 1（防切换会话串台）：只有处于同一个 session 且未被用户手动改名才更新内存标题
                if is_current && !user_renamed {
                    self.session_mgr.current_mut().metadata.thread_title = Some(title.clone());
                    self.refresh_terminal_title();
                }
                // 持久化：只有在用户未手动命名时才写入数据库，避免覆盖用户指定名称
                if !user_renamed {
                    let target_tid = thread_id.or_else(|| {
                        if is_current {
                            self.session_mgr.current().current_thread_id.clone()
                        } else {
                            None
                        }
                    });
                    if let Some(tid) = target_tid {
                        let store = self.services.thread_store.clone();
                        let manual_titles = self.global_ui.manual_thread_titles.clone();
                        let title_write_lock = self.global_ui.thread_title_write_lock.clone();
                        tokio::spawn(async move {
                            let _guard = title_write_lock.lock().await;
                            let user_named = manual_titles.read().contains(&tid);
                            if !user_named {
                                let _ = store.update_title(&tid, &title).await;
                            }
                        });
                    }
                }
                (true, false, false)
            }
        }
    }

    // poll_agent/poll_background_events/poll_cron_triggers are in polling.rs
}

/// 把一次 hook 执行结果转成挂在工具块上的渲染行。
///
/// 顺序对齐 PRD 快照 C5/C6：阻断/失败行在前，汇总行在后。汇总行是 `detail_only`，
/// 普通模式不渲染。
fn hook_rows_for(
    event: &str,
    ran: usize,
    duration_ms: u64,
    failures: &[cc_agent::agent::events::HookFailureInfo],
    is_async: bool,
) -> Vec<crate::ui::message_view::HookRow> {
    use crate::ui::message_view::{HookRow, NoteTone};
    let mut rows = Vec::new();
    for failure in failures {
        let (verb, tone) = if failure.kind == "block" {
            ("blocked", NoteTone::Warning)
        } else {
            ("failed", NoteTone::Error)
        };
        rows.push(HookRow::failure(
            format!("{event} hook {verb}: {}", failure.reason),
            tone,
        ));
    }
    rows.push(HookRow::summary(if is_async {
        format!("Async hook {event} completed")
    } else {
        format!("{ran} {event} hooks ran · {duration_ms}ms")
    }));
    rows
}

#[cfg(test)]
#[path = "../agent_ops_test.rs"]
mod tests;
