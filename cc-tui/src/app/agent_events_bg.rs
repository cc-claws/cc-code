use super::{message_pipeline::PipelineAction, *};
use crate::ui::message_view::MessageViewModel;

/// 后台任务完成的事件参数
pub(crate) struct BackgroundTaskResult {
    pub task_id: String,
    pub agent_name: String,
    pub success: bool,
    pub output: String,
    pub tool_calls_count: usize,
    pub duration_ms: u64,
    pub child_thread_id: Option<String>,
}

/// 构建后台任务完成的显示通知文本（截断版，供 UI 展示）
fn build_bg_display_notification(
    task_id: &str,
    agent_name: &str,
    success: bool,
    output: &str,
    tool_calls_count: usize,
    duration_ms: u64,
    lc: &crate::i18n::LcRegistry,
) -> String {
    let short_id: String = task_id.chars().take(8).collect();
    if success {
        let output_preview: String = output
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        let _preview = if output.chars().count() > 80 || output.lines().count() > 1 {
            format!("{}...", output_preview)
        } else {
            output_preview
        };
        lc.tr_args(
            "app-bg-task-done",
            &[
                ("id".into(), short_id.into()),
                ("agent".into(), agent_name.into()),
                ("tools".into(), (tool_calls_count as i64).into()),
                ("duration".into(), (duration_ms as i64).into()),
            ],
        )
    } else {
        let err_preview: String = output.chars().take(80).collect();
        lc.tr_args(
            "app-bg-task-failed",
            &[
                ("id".into(), short_id.into()),
                ("agent".into(), agent_name.into()),
                ("error".into(), err_preview.into()),
            ],
        )
    }
}

impl App {
    pub(crate) fn handle_background_task_completed(
        &mut self,
        result: BackgroundTaskResult,
    ) -> (bool, bool, bool) {
        let BackgroundTaskResult {
            task_id,
            agent_name,
            success,
            output,
            tool_calls_count,
            duration_ms,
            child_thread_id,
        } = result;
        if let Some(id) = child_thread_id.as_deref() {
            if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(id) {
                if sub.completion_received { return (false, false, false); }
                sub.completion_received = true;
            }
            self.handle_subagent_lifecycle(&serde_json::json!({
                "event": "stopped", "instanceId": id, "isError": !success,
                "result": output, "elapsedMs": duration_ms,
            }));
        }

        // 优先按 child_thread_id 匹配 background_agents，回退到 agent_name
        if let Some(ref ctid) = child_thread_id {
            if let Some(pos) = self
                .session_mgr
                .current_mut()
                .background_agents
                .iter()
                .position(|a| a.instance_id == *ctid)
            {
                self.session_mgr.current_mut().background_agents.remove(pos);
            }
        } else {
            // 回退：按 agent_name 匹配
            if let Some(pos) = self
                .session_mgr
                .current_mut()
                .background_agents
                .iter()
                .position(|a| a.agent_name == agent_name)
            {
                self.session_mgr.current_mut().background_agents.remove(pos);
            }
        }

        // 聚焦检查：用 child_thread_id 直接比较 focused_instance_id
        let was_focused = child_thread_id.as_deref()
            == self
                .session_mgr
                .current_mut()
                .focused_instance_id
                .as_deref();

        // 聚焦检查：如果被移除的是当前聚焦的 agent，退出聚焦
        if was_focused {
            self.session_mgr.current_mut().focused_instance_id = None;
            self.session_mgr.current_mut().ui.bg_bar_cursor = None;
            self.request_rebuild();
        } else if self.session_mgr.current().background_agents.is_empty() {
            // 抽屉已空 → 抽屉行不再渲染，残留的导航游标会让第三行卡在
            // `↑/↓ to select…` 并继续吞按键，这里一并收敛。
            self.session_mgr.current_mut().ui.bg_bar_cursor = None;
            self.request_rebuild();
        }
        // 用于 LLM 上下文的纯文本通知
        let short_id: String = task_id.chars().take(8).collect();
        let state_notification = if success {
            self.services.lc.tr_args(
                "app-bg-task-done-with-result",
                &[
                    ("id".into(), short_id.into()),
                    ("agent".into(), agent_name.clone().into()),
                    ("tools".into(), (tool_calls_count as i64).into()),
                    ("duration".into(), (duration_ms as i64).into()),
                    ("result".into(), output.clone().into()),
                ],
            )
        } else {
            self.services.lc.tr_args(
                "app-bg-task-failed-with-error",
                &[
                    ("id".into(), short_id.into()),
                    ("agent".into(), agent_name.clone().into()),
                    ("error".into(), output.clone().into()),
                ],
            )
        };

        // 将通知加入 origin_messages，使下一轮 agent 执行可见。
        // 仅在 executor 已结束（agent_done_pending_bg）时直接 push 作为兜底；
        // executor 运行期间的通知由 drain_notifications → StateSnapshot 路径写入，
        // 此处 push 会导致 origin_messages 中出现重复消息。
        if self.session_mgr.current_mut().agent.agent_done_pending_bg {
            self.session_mgr.current_mut().agent.origin_messages.push(
                cc_agent::messages::BaseMessage::human(state_notification.as_str()),
            );
        }

        let title = child_thread_id.as_deref()
            .and_then(|id| self.session_mgr.current().subagent_sessions.get(id))
            .map(|sub| sub.task_preview.as_str()).filter(|preview| !preview.is_empty())
            .unwrap_or(&agent_name);
        let status = if success { "finished" } else { "failed" };
        let notification = format!("• Agent \"{title}\" {status} · {}", super::subagent_session::format_duration(std::time::Duration::from_millis(duration_ms)));
        self.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(notification)));
        // 停止任务不得触发新的模型轮次，否则全停快捷键会再次唤醒主任务。
        if child_thread_id.as_deref().and_then(|id| self.session_mgr.current().subagent_sessions.get(id))
            .is_some_and(|sub| sub.status == super::subagent_session::SubagentRunStatus::Stopped) {
            if !self.session_mgr.current().has_running_subagents() {
                self.session_mgr.current_mut().agent.agent_done_pending_bg = false;
            }
            return (true, false, false);
        }
        // 累积当前完成通知到 pre_done_bg_completions（显示文本）
        let display_notification = build_bg_display_notification(
            &task_id,
            &agent_name,
            success,
            &output,
            tool_calls_count,
            duration_ms,
            &self.services.lc,
        );
        self.session_mgr
            .current_mut()
            .agent
            .pre_done_bg_completions
            .push(display_notification);

        // 累积结构化结果到 pre_done_bg_results（供 auto-continuation 注入合成消息）
        self.session_mgr
            .current_mut()
            .agent
            .pre_done_bg_results
            .push(cc_agent::agent::events::BackgroundTaskResult {
                task_id: task_id.clone(),
                agent_name: agent_name.clone(),
                prompt_summary: String::new(),
                success,
                output,
                tool_calls_count,
                duration_ms,
                child_thread_id: child_thread_id.clone(),
            });

        // 如果 agent 已完成（Done）且所有后台任务都已完成，关闭通道并自动提交 continuation
        if self.session_mgr.current_mut().agent.agent_done_pending_bg
            && !self.session_mgr.current().has_running_subagents()
        {
            tracing::info!("all background tasks completed, auto-submitting continuation");
            self.session_mgr.current_mut().agent.agent_done_pending_bg = false;
            // 使用结构化结果（而非显示文本）驱动 continuation
            let all_results =
                std::mem::take(&mut self.session_mgr.current_mut().agent.pre_done_bg_results);
            self.session_mgr.current_mut().agent.pending_bg_continuation = Some(all_results);

            return (true, false, true);
        } else if !self.session_mgr.current_mut().agent.agent_done_pending_bg
            && !self.session_mgr.current().has_running_subagents()
        {
            // 竞态修复：agent 尚未 Done，但所有后台任务已完成。
            // 暂存通知已在上方 push，待 Done 处理时检查此字段并设置 pending_bg_continuation。
            tracing::info!(
                "background task completed before Done, buffering notification for deferred continuation"
            );
        }

        (true, false, false)
    }
}
