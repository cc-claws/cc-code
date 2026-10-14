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

        // 用户是否正在这个子 Agent 的视口里阅读。
        // 必须两个 id 都在场才算命中：写成 `child_thread_id == focused_instance_id`
        // 会让「都没有 id」也判成聚焦（None == None），从而误留抽屉行。
        let was_focused = match (
            child_thread_id.as_deref(),
            self.session_mgr.current().focused_instance_id.as_deref(),
        ) {
            (Some(child), Some(focused)) => child == focused,
            _ => false,
        };

        // 下游（完成行标题 / Stopped 判定）需要的信息先取好：非聚焦分支随后会回收会话。
        let finished_title = child_thread_id
            .as_deref()
            .and_then(|id| self.session_mgr.current().subagent_sessions.get(id))
            .map(|sub| sub.task_preview.clone())
            .filter(|preview| !preview.is_empty());
        let finished_stopped = child_thread_id
            .as_deref()
            .and_then(|id| self.session_mgr.current().subagent_sessions.get(id))
            .is_some_and(|sub| sub.status == super::subagent_session::SubagentRunStatus::Stopped);
        // 停止发起方：true = 用户按键，false = 主 Agent 经 Agent(action="stop")。
        // 仅在确为停止时才有意义（PRD §4.4.1 的 `stopped by user|main Agent` 区分）。
        let stopped_by_user = child_thread_id
            .as_deref()
            .and_then(|id| self.session_mgr.current().subagent_sessions.get(id))
            .is_some_and(|sub| sub.stopped_by_user);

        if was_focused {
            // 用户正在看它：视口与抽屉行都原地保留（就地显示 `✓ completed`），
            // 等他离开视口时再由 `switch_subagent_viewport` 回收。
            // 结束瞬间抽走视口 = 正在读的内容凭空消失，这不是回收而是打断。
            self.request_rebuild();
        } else {
            // 抽屉行定位：优先用 child_thread_id 精确匹配 background_agents；
            // 仅在事件压根没带 child_thread_id 时才按 agent_name 兜底
            // （同名多任务并存时只有 instance_id 是唯一标识）。
            let removed = if let Some(ctid) = child_thread_id.as_deref() {
                self.session_mgr
                    .current_mut()
                    .background_agents
                    .iter()
                    .position(|a| a.instance_id == ctid)
            } else {
                self.session_mgr
                    .current_mut()
                    .background_agents
                    .iter()
                    .position(|a| a.agent_name == agent_name)
            };
            let mut removed_row_id = None;
            if let Some(pos) = removed {
                let removed_row = self.session_mgr.current_mut().background_agents.remove(pos);
                removed_row_id = Some(removed_row.instance_id.clone());
            }

            // 游标必须跟着列表收敛：被移除行在游标之前 → 下标前移一位（仍指向原任务）；
            // 否则夹到新的末行，避免出现「越界游标 → 没有任何亮白行、Enter/x 落到别的行」。
            if let Some(cursor) = self.session_mgr.current().ui.bg_bar_cursor {
                let len = self.session_mgr.current().background_agents.len();
                let adjusted = if removed.is_some_and(|pos| cursor > pos + 1) { cursor - 1 } else { cursor };
                self.session_mgr.current_mut().ui.bg_bar_cursor = if len == 0 { None } else { Some(adjusted.min(len)) };
            }

            // 行已被摘掉的子会话再也没有入口（Enter 只能从抽屉行进入）→ 同帧回收，
            // 否则 `subagent_sessions` 会留下永久不可达的条目（PRD §4.14 的债务）。
            // 行可能已被别处先摘掉（`poll_agent` 对已停止且结束超过 3s 的行只摘行、留会话），
            // 此时 `removed_row_id` 是 None —— 必须回退到 child_thread_id，否则漏掉的
            // 恰恰是这次修复要堵的不可达条目。仍活跃的会话（同名兜底误命中）不许摘。
            let reclaim_id = removed_row_id.or_else(|| child_thread_id.clone());
            if let Some(id) = reclaim_id {
                let session = self.session_mgr.current_mut();
                if session
                    .subagent_sessions
                    .get(&id)
                    .is_some_and(|sub| !sub.status.is_active())
                {
                    session.subagent_sessions.remove(&id);
                }
            }
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

        let title = finished_title.as_deref().unwrap_or(&agent_name);
        // 停止与「完成/失败」是不同语义，必须独立成行：用户要能区分「它自己跑完了」
        // 「它出错了」「它被停了」三种终态（PRD §4.4.1）。停止行还区分发起方。
        let notification = if finished_stopped {
            let by = if stopped_by_user { "user" } else { "main Agent" };
            format!("• Agent \"{title}\" was stopped by {by}")
        } else {
            let status = if success { "finished" } else { "failed" };
            format!(
                "• Agent \"{title}\" {status} · {}",
                super::subagent_session::format_duration(std::time::Duration::from_millis(duration_ms))
            )
        };
        self.apply_pipeline_action(PipelineAction::add_message(MessageViewModel::system(notification)));
        // 停止任务不得触发新的模型轮次，否则全停快捷键会再次唤醒主任务。
        if finished_stopped {
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

        // 等待态计数行随活跃数递减（PRD §4.4.2）：一个任务收尾后立刻反映「还差几个」，
        // 归零时该行被移除（`refresh_bg_waiting_note` 内部处理）。必须放在下面两个
        // return 分支之前，否则「全部完成」那条路径会带着过期的计数行返回。
        self.refresh_bg_waiting_note();

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
