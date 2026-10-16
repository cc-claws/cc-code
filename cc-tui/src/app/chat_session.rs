use std::collections::VecDeque;
use std::time::Instant;

use cc_middlewares::prelude::{SkillMetadata, TodoItem};

use super::{
    langfuse_state::LangfuseState, AgentComm, AgentShellSlot, BackgroundShell, CommandSystem,
    MessageState, SessionMetadata, ShellCommandPool, UiState,
};
use crate::{command::CommandRegistry, thread::ThreadId};

/// 正在运行的后台 SubAgent
#[derive(Clone, Debug)]
pub struct RunningBgAgent {
    pub agent_name: String,
    pub instance_id: String,
    pub started_at: Instant,
    pub total_steps: usize,
}

/// 后台任务完成/等待提示与发起它的 thread 绑定，切换对话时不会串入当前消息流。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBgShellNotification {
    pub owner_session_id: Option<String>,
    pub content: String,
}

/// 独立聊天会话：封装一个对话的完整 UI 状态、Agent 通信状态和持久化上下文。
pub struct ChatSession {
    pub ui: UiState,
    pub messages: MessageState,
    pub session_panels: super::panel_manager::PanelManager,
    pub commands: CommandSystem,
    pub metadata: SessionMetadata,
    pub agent: AgentComm,
    pub current_thread_id: Option<ThreadId>,
    pub langfuse: LangfuseState,
    pub todo_items: Vec<TodoItem>,
    pub background_agents: Vec<RunningBgAgent>,
    pub focused_instance_id: Option<String>,
    pub(crate) subagent_sessions:
        std::collections::HashMap<String, super::subagent_session::BackgroundAgentSession>,
    pub subagent_control_prefix: Option<Instant>,
    pub subagent_interrupt_at: Option<Instant>,
    pub pending_thread_approvals:
        VecDeque<(cc_acp::transport::types::RequestId, serde_json::Value)>,
    pub approval_source_agent_id: Option<String>,
    pub main_scroll_position: (usize, bool),
    pub main_input_draft: Option<tui_textarea::TextArea<'static>>,
    pub spinner_state: cc_widgets::SpinnerState,
    pub shell_pool: ShellCommandPool,
    /// Ctrl+B 后台化的 shell 任务列表（多个并发后台命令）
    pub background_shells: Vec<BackgroundShell>,
    /// agent Bash 工具调用的 shell 跟踪槽位（前台可 Ctrl+B + 后台化后的都在此）。
    /// 与 `background_shells` 平行：本列表的命令由 `BashTool::invoke` 发起，
    /// result_rx 由 invoke 独占，UI 仅用 ExitSignal 检测退出。
    pub agent_shells: Vec<AgentShellSlot>,
    /// 尚未送达或属于其它会话的后台 shell 通知。
    pub pending_bg_shell_notifications: VecDeque<PendingBgShellNotification>,
    pub(crate) bg_shell_steering: super::bg_shell_notifications::BgShellSteering,
    /// 当前最新会话回顾文本（在上一轮任务总结行下方展示，新一轮开始时清空）
    pub latest_recap: Option<String>,
}

impl ChatSession {
    pub fn new(
        cwd: String,
        command_registry: CommandRegistry,
        skills: Vec<SkillMetadata>,
        lc: &crate::i18n::LcRegistry,
        detail_enabled: bool,
        diff_enabled: bool,
        streaming_mode: Option<String>,
    ) -> Self {
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);
        let commands = CommandSystem::new(command_registry, skills.clone(), lc);
        let mut messages = MessageState::new(
            cwd.clone(),
            render_tx.clone(),
            std::sync::Arc::clone(&render_cache),
            std::sync::Arc::clone(&render_notify),
        );
        if let Some(ref mode) = streaming_mode {
            messages.pipeline.init_streaming_mode_from_config(mode);
        }
        Self {
            ui: UiState::new(
                super::build_textarea(false),
                &cwd,
                detail_enabled,
                diff_enabled,
            ),
            messages,
            session_panels: super::panel_manager::PanelManager::new(),
            commands,
            metadata: SessionMetadata::new(),
            agent: AgentComm::default(),
            current_thread_id: None,
            langfuse: LangfuseState::default(),
            todo_items: Vec::new(),
            background_agents: Vec::new(),
            focused_instance_id: None,
            subagent_sessions: std::collections::HashMap::new(),
            subagent_control_prefix: None,
            subagent_interrupt_at: None,
            pending_thread_approvals: VecDeque::new(),
            approval_source_agent_id: None,
            main_scroll_position: (usize::MAX, true),
            main_input_draft: None,
            spinner_state: cc_widgets::SpinnerState::with_lang(
                cc_widgets::SpinnerMode::Idle,
                lc.current_lang(),
            ),
            shell_pool: ShellCommandPool::default(),
            background_shells: Vec::new(),
            agent_shells: Vec::new(),
            pending_bg_shell_notifications: VecDeque::new(),
            bg_shell_steering: Default::default(),
            latest_recap: None,
        }
    }

    /// 统一同步主会话及其所有子 Agent 的语言与动词库配置
    pub fn sync_language(&mut self, lang: &str) {
        self.spinner_state.set_language(lang);
        for sub in self.subagent_sessions.values_mut() {
            sub.set_language(lang);
        }
    }
}
