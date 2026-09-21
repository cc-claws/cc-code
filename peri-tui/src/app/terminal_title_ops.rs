use crate::{
    app::App,
    terminal_title::{sanitize_title, TerminalTitleItem, TerminalTitleStatusKind},
};

impl App {
    /// 计算当前会话的终端标题生命周期状态
    pub fn current_title_status(&self) -> TerminalTitleStatusKind {
        // 优先级 1：等待用户操作（HITL 审批、AskUser 问答、OAuth、Rewind 确认）
        if self
            .session_mgr
            .current()
            .agent
            .interaction_prompt
            .is_some()
            || self.global_ui.oauth_prompt.is_some()
        {
            return TerminalTitleStatusKind::ActionRequired;
        }

        // 优先级 2：任务运行中
        if self.session_mgr.current().ui.loading {
            let session = self.session_mgr.current();
            let is_tool = session.agent.active_tool.is_some()
                || !session.agent.running_tools.is_empty()
                || *session.spinner_state.mode() == peri_widgets::SpinnerMode::ToolUse;
            if is_tool {
                TerminalTitleStatusKind::Working
            } else {
                TerminalTitleStatusKind::Thinking
            }
        } else {
            // 优先级 3：非运行态，检查是否为已完成任务状态
            if self
                .session_mgr
                .current()
                .agent
                .last_task_duration
                .is_some()
            {
                TerminalTitleStatusKind::Done
            } else {
                TerminalTitleStatusKind::Idle
            }
        }
    }

    /// 提取当前项目名称（取 cwd 的最后一级目录名）
    pub fn project_name(&self) -> &str {
        std::path::Path::new(&self.services.cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| {
                if self.services.cwd.is_empty() {
                    "peri"
                } else {
                    &self.services.cwd
                }
            })
    }

    /// 刷新终端标题（带内容去重与清洗）
    pub fn refresh_terminal_title(&mut self) {
        let status = self.current_title_status();
        let project = self.project_name();
        let thread_title = self.session_mgr.current().metadata.thread_title.as_deref();
        let spinner_frame = self.session_mgr.current().spinner_state.title_frame();
        let mut frame_buf = [0u8; 4];
        let spinner_str = spinner_frame.encode_utf8(&mut frame_buf);

        let activity = match status {
            TerminalTitleStatusKind::ActionRequired => {
                let now_sec = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now_sec.is_multiple_of(2) {
                    "[ ! ] Action Required"
                } else {
                    "[ . ] Action Required"
                }
            }
            TerminalTitleStatusKind::Working | TerminalTitleStatusKind::Thinking => spinner_str,
            TerminalTitleStatusKind::Idle | TerminalTitleStatusKind::Done => "",
        };

        let item = TerminalTitleItem::new(status, project, thread_title, Some(activity));
        let raw_title = item.format_title();
        let clean_title = sanitize_title(&raw_title);

        // 内容去重：仅在标题内容变化时写出 OSC 0 转义序列
        if self.global_ui.last_terminal_title.as_deref() == Some(&clean_title) {
            return;
        }

        self.global_ui.last_terminal_title = Some(clean_title.clone());
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::SetTitle(clean_title)
        );
    }
}

#[cfg(test)]
#[path = "terminal_title_ops_test.rs"]
mod terminal_title_ops_test;
