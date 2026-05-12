use super::message_pipeline::PipelineAction;
use super::*;

impl App {
    pub(crate) fn handle_background_task_completed(
        &mut self,
        task_id: String,
        agent_name: String,
        success: bool,
        output: String,
        tool_calls_count: usize,
        duration_ms: u64,
    ) -> (bool, bool, bool) {
        // 递减后台任务计数
        self.session_mgr.sessions[self.session_mgr.active].background_task_count =
            self.session_mgr.sessions[self.session_mgr.active]
                .background_task_count
                .saturating_sub(1);

        // 用于 LLM 上下文的纯文本通知
        let state_notification = if success {
            format!(
                "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n结果:\n{}",
                &task_id[..8.min(task_id.len())],
                agent_name,
                tool_calls_count,
                duration_ms,
                output,
            )
        } else {
            format!(
                "[后台任务 {} 执行失败] Agent: {}\n错误:\n{}",
                &task_id[..8.min(task_id.len())],
                agent_name,
                output,
            )
        };

        // 将通知加入 agent_state_messages，使下一轮 agent 执行可见
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_state_messages
            .push(rust_create_agent::messages::BaseMessage::human(
                state_notification.as_str(),
            ));

        // 以 ToolBlock 样式显示（紧凑单行格式，折叠长输出）
        let short_id = &task_id[..8.min(task_id.len())];
        let display_name = format!("bg:{}", agent_name);
        // 输出截断为单行（取第一行，再截取前 80 字符）
        let first_line = output.lines().next().unwrap_or("");
        let one_line = if first_line.chars().count() > 80 {
            let truncated: String = first_line.chars().take(80).collect();
            format!("{}...", truncated)
        } else if first_line.is_empty() && !output.is_empty() {
            String::from("(empty)")
        } else {
            first_line.to_string()
        };
        let header_info = if success {
            format!(
                "{} completed ({} calls, {}ms): {}",
                short_id, tool_calls_count, duration_ms, one_line
            )
        } else {
            format!("{} failed: {}", short_id, one_line)
        };
        let mut vm =
            MessageViewModel::tool_block(display_name.clone(), header_info, None, !success);
        if let MessageViewModel::ToolBlock { collapsed, .. } = &mut vm {
            *collapsed = true; // 始终折叠，摘要已在 header 中
        }
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));

        // 诊断日志：记录 BackgroundTaskCompleted 处理后的 view_messages 中 SubAgentGroup 数量
        {
            let subagent_count = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .iter()
                .filter(|vm| vm.is_subagent_group())
                .count();
            let frozen_count = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .frozen_subagent_vms_count();
            tracing::debug!(
                task_id = %&task_id[..8.min(task_id.len())],
                agent_name = %agent_name,
                subagent_count_in_view = subagent_count,
                frozen_count,
                background_task_count = self.session_mgr.sessions[self.session_mgr.active].background_task_count,
                agent_done_pending_bg = self.session_mgr.sessions[self.session_mgr.active].agent.agent_done_pending_bg,
                "[bg-diag] after BackgroundTaskCompleted"
            );
        }

        // 如果 agent 已完成（Done）且所有后台任务都已完成，关闭通道并自动提交 continuation
        if self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .agent_done_pending_bg
            && self.session_mgr.sessions[self.session_mgr.active].background_task_count == 0
        {
            tracing::info!("all background tasks completed, auto-submitting continuation");
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_done_pending_bg = false;
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_rx = None;
            // 截断显示文本（完整数据已在 agent_state_messages 中供 LLM 使用）
            let display_notification = if success {
                let output_preview: String = output
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();
                format!(
                    "[后台任务 {} 已完成] Agent: {} | 工具调用: {} | 耗时: {}ms\n{}",
                    &task_id[..8.min(task_id.len())],
                    agent_name,
                    tool_calls_count,
                    duration_ms,
                    if output.chars().count() > 80 || output.lines().count() > 1 {
                        format!("{}...", output_preview)
                    } else {
                        output_preview
                    },
                )
            } else {
                let err_preview: String = output.chars().take(80).collect();
                format!(
                    "[后台任务 {} 执行失败] Agent: {} | {}",
                    &task_id[..8.min(task_id.len())],
                    agent_name,
                    err_preview,
                )
            };
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .pending_bg_continuation = Some(display_notification);

            // 后台任务运行期间被延迟的 auto-compact：现在通道已安全关闭，可以触发。
            // compact 完成后（loading = false），poll_agent 会在下一帧处理
            // pending_bg_continuation 并通过 submit_message 自动提交 continuation。
            if self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .needs_auto_compact
            {
                self.session_mgr.sessions[self.session_mgr.active]
                    .agent
                    .needs_auto_compact = false;
                tracing::info!(
                    "auto-compact: deferred from Done (background tasks were running), triggering now"
                );
                self.start_compact("auto".to_string());
                return (true, false, true);
            }
            return (true, false, true);
        }

        (true, false, false)
    }
}
