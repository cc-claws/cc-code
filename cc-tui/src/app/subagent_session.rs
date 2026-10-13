//! 每个子线程拥有独立的消息、指标和流式状态；视口切换只选择数据源。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cc_agent::messages::BaseMessage;

use super::{message_pipeline::MessagePipeline, AgentComm, AgentEvent, ChatSession, MessageViewModel};

pub(crate) fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 { format!("{}h {}m", seconds / 3600, seconds % 3600 / 60) }
    else if seconds >= 60 { format!("{}m {}s", seconds / 60, seconds % 60) }
    else { format!("{seconds}s") }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubagentRunStatus {
    Running,
    AwaitingApproval,
    Completed,
    Stopped,
    Failed,
}

impl SubagentRunStatus {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::AwaitingApproval)
    }
}

pub(crate) struct BackgroundAgentSession {
    pub instance_id: String,
    pub agent_type: String,
    pub task_preview: String,
    pub current_activity: String,
    pub started_at: Instant,
    pub elapsed: Option<Duration>,
    pub timeout_secs: Option<u64>,
    pub model: String,
    pub status: SubagentRunStatus,
    pub final_result: Option<String>,
    pub pipeline: MessagePipeline,
    pub view_messages: Vec<MessageViewModel>,
    pub agent: AgentComm,
    pub spinner: cc_widgets::SpinnerState,
    pub pending_inputs: Vec<(uuid::Uuid, String)>,
    pub scroll_position: (usize, bool),
    pub running_tool_times: HashMap<String, Instant>,
    pub completion_received: bool,
    pub ended_at: Option<Instant>,
    pub notices: Vec<String>,
    pub input_draft: Option<tui_textarea::TextArea<'static>>,
    /// 是否为后台（run_in_background）任务；前台同步 SubAgent 的事件同时驱动主时间线 SubAgentGroup。
    pub is_background: bool,
}

impl BackgroundAgentSession {
    pub(crate) fn new(id: String, agent_type: String, preview: String, cwd: String) -> Self {
        let started_at = Instant::now();
        let agent = AgentComm { session_start_time: Some(started_at), ..AgentComm::default() };
        Self {
            instance_id: id,
            agent_type,
            task_preview: preview,
            current_activity: String::new(),
            started_at,
            elapsed: None,
            timeout_secs: None,
            model: String::new(),
            status: SubagentRunStatus::Running,
            final_result: None,
            pipeline: MessagePipeline::new(cwd),
            view_messages: Vec::new(),
            agent,
            spinner: cc_widgets::SpinnerState::new(cc_widgets::SpinnerMode::Thinking),
            pending_inputs: Vec::new(),
            scroll_position: (usize::MAX, true),
            running_tool_times: HashMap::new(),
            completion_received: false,
            ended_at: None,
            notices: Vec::new(),
            input_draft: None,
            is_background: false,
        }
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.elapsed.unwrap_or_else(|| self.started_at.elapsed())
    }

    pub(crate) fn tokens_used(&self) -> u64 {
        let tracker = &self.agent.session_token_tracker;
        tracker.total_input_tokens + tracker.total_output_tokens
    }

    pub(crate) fn rebuild(&mut self) {
        self.view_messages = self.pipeline.build_tail_vms();
        for vm in &mut self.view_messages {
            if let MessageViewModel::ToolBlock { tool_call_id, started_at, timeout_ms, .. } = vm {
                if let Some(start) = self.running_tool_times.get(tool_call_id) {
                    *started_at = Some(*start);
                    *timeout_ms = self.timeout_secs.map(|seconds| seconds.saturating_mul(1000));
                    vm.recompute_hash();
                }
            }
        }
        // 投递确认前的用户消息也可见；快照回流时移除，避免重复气泡。
        for (_, text) in &self.pending_inputs {
            self.view_messages.push(MessageViewModel::user(text.clone()));
        }
        self.view_messages.extend(self.notices.iter().cloned().map(MessageViewModel::system));
    }

    pub(crate) fn handle_event(&mut self, event: AgentEvent) {
        let now = Instant::now();
        match &event {
            AgentEvent::ToolStart { tool_call_id, name, display, args, .. } => {
                if self.running_tool_times.insert(tool_call_id.clone(), now).is_none() {
                    self.agent.tool_call_count += 1;
                }
                self.current_activity = format!("{}({})", display, args);
                // 与主 Agent 一致地登记「运行中工具」：状态栏第二行读的是
                // running_tools / recent_tools，只写 active_tool 的话子视口永远为空。
                let args_summary: String = crate::app::tool_display::sanitize_display_text(args)
                    .chars()
                    .take(40)
                    .collect();
                let active = super::agent_comm::ActiveToolInfo {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    display: display.clone(),
                    args_summary: args_summary.clone(),
                };
                self.agent.running_tools.push(active.clone());
                self.agent.active_tool = Some(active);
                self.agent.push_recent_tool(super::agent_comm::RecentToolEntry {
                    tool_call_id: tool_call_id.clone(),
                    display: display.clone(),
                    args_summary,
                    running: true,
                    started_at: now,
                    visible_until: None,
                });
                self.spinner.end_thinking();
                self.spinner.set_mode_keep_verb(cc_widgets::SpinnerMode::ToolUse);
            }
            AgentEvent::ToolEnd { tool_call_id, name, .. } => {
                if self.running_tool_times.remove(tool_call_id).is_some() {
                    *self.agent.session_tool_stats.entry(name.clone()).or_insert(0) += 1;
                }
                self.agent
                    .running_tools
                    .retain(|tool| tool.tool_call_id != *tool_call_id);
                self.agent.active_tool = self.agent.running_tools.last().cloned();
                self.agent.finish_recent_tool(tool_call_id, now);
            }
            AgentEvent::AiReasoning(_) => {
                self.spinner.begin_thinking();
                self.spinner.set_mode_keep_verb(cc_widgets::SpinnerMode::Thinking);
            }
            AgentEvent::AssistantChunk { .. } => {
                self.spinner.end_thinking();
                self.spinner.set_mode_keep_verb(cc_widgets::SpinnerMode::Responding);
            }
            AgentEvent::TokenUsageUpdate { usage, model, .. } => {
                self.agent.session_token_tracker.accumulate(usage);
                self.spinner.set_token_count(self.tokens_used() as usize);
                if !model.is_empty() { self.model.clone_from(model); }
            }
            AgentEvent::ContextWarning { total_tokens, .. } => {
                self.agent.context_window = (*total_tokens).min(u32::MAX as u64) as u32;
            }
            AgentEvent::LlmRetrying { attempt, max_attempts, delay_ms, error } => {
                self.agent.retry_status = Some(super::agent_comm::RetryStatus {
                    attempt: *attempt, max_attempts: *max_attempts, delay_ms: *delay_ms, error: error.clone(),
                });
            }
            AgentEvent::SystemNotice { level, message } => {
                self.agent.system_notice = Some(super::agent_comm::SystemNotice { level: *level, message: message.clone() });
            }
            AgentEvent::LspDiagnostics { errors, warnings, files_with_errors } => {
                self.agent.lsp_errors = *errors;
                self.agent.lsp_warnings = *warnings;
                self.agent.lsp_files_with_errors = *files_with_errors;
            }
            AgentEvent::StateSnapshot(messages) => {
                for message in messages {
                    if let BaseMessage::Human { .. } = message {
                        let text = message.content();
                        if let Some(index) = self.pending_inputs.iter().position(|(_, input)| input == &text) {
                            self.pending_inputs.remove(index);
                        }
                    }
                }
            }
            _ => {}
        }
        self.pipeline.handle_event(event);
        self.rebuild();
    }
}

impl ChatSession {
    pub(crate) fn active_subagent(&self) -> Option<&BackgroundAgentSession> {
        self.focused_instance_id.as_ref().and_then(|id| self.subagent_sessions.get(id))
    }

    pub(crate) fn current_view_messages(&self) -> &[MessageViewModel] {
        self.active_subagent().map(|sub| sub.view_messages.as_slice()).unwrap_or(&self.messages.view_messages)
    }

    pub(crate) fn viewport_agent(&self) -> &AgentComm {
        self.active_subagent().map(|sub| &sub.agent).unwrap_or(&self.agent)
    }

    pub(crate) fn viewport_spinner(&self) -> &cc_widgets::SpinnerState {
        self.active_subagent().map(|sub| &sub.spinner).unwrap_or(&self.spinner_state)
    }

    pub(crate) fn viewport_loading(&self) -> bool {
        self.active_subagent().map(|sub| sub.status.is_active()).unwrap_or(self.ui.loading)
    }

    pub(crate) fn has_running_subagents(&self) -> bool {
        self.background_agents.iter().any(|agent| {
            self.subagent_sessions.get(&agent.instance_id).is_none_or(|sub| sub.status.is_active())
        })
    }

    /// 抽屉是否处于键盘导航态。
    ///
    /// 必须同时满足「还有后台 Agent」+「游标已激活」：后台 Agent 全部结束后
    /// `bg_bar_cursor` 可能残留，若不收敛会让第三行卡在 `↑/↓ to select…` 且
    /// `handle_bar_focus` 继续吞掉所有按键。
    pub(crate) fn drawer_nav_active(&self) -> bool {
        self.ui.bg_bar_cursor.is_some() && !self.background_agents.is_empty()
    }
}

impl super::App {
    pub(crate) fn clear_subagent_sessions(&mut self) {
        self.cancel_all_subagents();
        self.switch_subagent_viewport(None);
        let session = self.session_mgr.current_mut();
        session.background_agents.clear();
        session.subagent_sessions.clear();
        session.pending_thread_approvals.clear();
        session.approval_source_agent_id = None;
        session.subagent_interrupt_at = None;
        session.subagent_control_prefix = None;
    }

    pub(crate) fn switch_subagent_viewport(&mut self, id: Option<String>) {
        let session = self.session_mgr.current_mut();
        let position = (session.ui.scroll_offset, session.ui.scroll_follow);
        let draft = std::mem::replace(&mut session.ui.textarea, super::build_textarea(false));
        if let Some(sub) = session.focused_instance_id.as_ref().and_then(|id| session.subagent_sessions.get_mut(id)) {
            sub.scroll_position = position;
            sub.input_draft = Some(draft);
        } else { session.main_scroll_position = position; session.main_input_draft = Some(draft); }
        session.focused_instance_id = id.filter(|id| session.subagent_sessions.contains_key(id));
        let draft = if let Some(sub) = session.focused_instance_id.as_ref().and_then(|id| session.subagent_sessions.get_mut(id)) {
            sub.input_draft.take()
        } else { session.main_input_draft.take() };
        if let Some(draft) = draft { session.ui.textarea = draft; }
        let (offset, follow) = session.active_subagent().map(|sub| sub.scroll_position).unwrap_or(session.main_scroll_position);
        session.ui.scroll_offset = offset;
        session.ui.scroll_follow = follow;
        session.ui.text_selection.clear();
        session.ui.bg_bar_cursor = None;
        self.render_rebuild();
    }

    pub(crate) fn cancel_subagent(&mut self, id: &str) {
        if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(id) {
            if !sub.status.is_active() { return; }
            sub.current_activity = "Stopping…".into();
        }
        self.cancel_subagent_approval(id);
        if let (Some(client), Some(session_id)) = (self.acp_client.clone(), self.session_mgr.current().current_thread_id.clone()) {
            let target = id.to_string();
            tokio::spawn(async move {
                if let Err(error) = client.cancel_subagent(&session_id, &target).await {
                    tracing::warn!(%error, "Subagent cancellation failed");
                }
            });
        }
        // 本地 shell host 的句柄按来源精确中断，保证子进程及时退出。
        for shell in &self.session_mgr.current().agent_shells {
            if shell.source_agent_id.as_deref() == Some(id) { shell.kill.abort(); }
        }
    }

    pub(crate) fn cancel_subagent_approval(&mut self, id: &str) {
        if self.session_mgr.current().approval_source_agent_id.as_deref() == Some(id) {
            let response = self.take_interaction_cancel_response();
            self.session_mgr.current_mut().approval_source_agent_id = None;
            if let (Some(client), Some((request_id, response))) = (self.acp_client.clone(), response) {
                tokio::spawn(async move { let _ = client.send_response(request_id, Ok(response)).await; });
            }
        }
    }

    pub(crate) fn cancel_all_subagents(&mut self) {
        let ids: Vec<_> = self.session_mgr.current().subagent_sessions.values()
            .filter(|sub| sub.status.is_active()).map(|sub| sub.instance_id.clone()).collect();
        for id in ids { self.cancel_subagent(&id); }
    }

    pub(crate) fn send_subagent_input(&mut self, text: String) {
        let Some(id) = self.session_mgr.current().focused_instance_id.clone() else { return; };
        let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(&id) else { return; };
        if !sub.status.is_active() || text.trim().is_empty() { return; }
        let input_id = uuid::Uuid::new_v4();
        sub.pending_inputs.push((input_id, text.clone()));
        sub.rebuild();
        self.session_mgr.current_mut().ui.textarea = super::build_textarea(false);
        self.render_rebuild();
        let event_tx = self.services.bg_event_tx.clone();
        let client = self.acp_client.clone();
        let session_id = self.session_mgr.current().current_thread_id.clone();
        tokio::spawn(async move {
            let result = match (client, session_id) {
                (Some(client), Some(session_id)) => client.send_subagent_message(&session_id, &id, &text).await,
                _ => Err("ACP session unavailable".into()),
            };
            let _ = event_tx.send(AgentEvent::SubagentInputResult {
                instance_id: id, input_id, error: result.err(),
            });
        });
    }
}

#[cfg(test)]
#[path = "subagent_session_test.rs"]
mod tests;
