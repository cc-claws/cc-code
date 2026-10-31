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
    pub(crate) main_display: crate::ui::render_thread::DisplayFlags,
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
    pub(crate) fn remember_viewport_display(&mut self) {
        let display = self.ui.display();
        if let Some(sub) = self
            .focused_instance_id
            .as_ref()
            .and_then(|id| self.subagent_sessions.get_mut(id))
        {
            sub.display = display;
        } else {
            self.main_display = display;
        }
        self.messages.invalidate_layout();
    }

    pub(crate) fn render_view_messages(
        &mut self,
    ) -> Vec<crate::ui::message_view::MessageViewModel> {
        if let Some(sub) = self
            .focused_instance_id
            .as_ref()
            .and_then(|id| self.subagent_sessions.get_mut(id))
        {
            sub.refresh_view_if_dirty();
        }
        let mut messages = self.current_view_messages().to_vec();
        if self.focused_instance_id.is_none() {
            for message in &mut messages {
                message.set_image_paths(&self.metadata.image_paths);
            }
        }
        messages
    }

    pub(crate) fn rebuild_viewport(&mut self) {
        let messages = self.render_view_messages();
        self.send_render_snapshot(crate::ui::render_thread::RenderEvent::Rebuild(messages));
    }

    pub(crate) fn send_render_snapshot(&mut self, event: crate::ui::render_thread::RenderEvent) {
        let event = crate::ui::render_thread::RenderEvent::WithDisplay {
            display: self.ui.display(),
            event: Box::new(self.with_render_viewport(event)),
        };
        self.messages.send_render_snapshot(event);
    }

    /// 重排命令携带同一次缓存取样的阅读位置，不依赖下一帧 Viewport 上报。
    pub(crate) fn with_render_viewport(
        &self,
        event: crate::ui::render_thread::RenderEvent,
    ) -> crate::ui::render_thread::RenderEvent {
        use crate::ui::render_thread::{RenderEvent, RenderViewport};
        let Some(area) = self.ui.messages_area else {
            return event;
        };
        let offset = if self.ui.scroll_follow {
            self.ui.scrollbar_max_offset
        } else {
            self.ui.scroll_offset
        };
        let prefix = self.ui.message_prefix_rows;
        let prefix_visible = prefix.saturating_sub(offset).min(area.height as usize);
        let cache = self.messages.render_cache.read();
        let anchor =
            if self.ui.scroll_follow || offset < prefix || self.messages.last_viewport.is_none() {
                None
            } else {
                cache
                    .scroll_anchor
                    .filter(|anchor| anchor.scroll_revision == self.ui.scroll_revision)
                    .map(|anchor| anchor.position)
                    .or_else(|| cache.viewport_anchor_at(offset.saturating_sub(prefix)))
            };
        RenderEvent::WithViewport {
            viewport: RenderViewport {
                top: offset.saturating_sub(prefix),
                height: area.height as usize - prefix_visible,
                follow_tail: self.ui.scroll_follow,
                anchor,
                scroll_revision: self.ui.scroll_revision,
            },
            event: Box::new(event),
        }
    }

    pub(crate) fn send_render_event(
        &self,
        event: crate::ui::render_thread::RenderEvent,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<crate::ui::render_thread::RenderEvent>>
    {
        self.messages
            .send_render_event(self.with_render_viewport(event))
    }

    // 会话构造参数天然较多（cwd / 命令表 / 技能 / 配置项…），集中传参比散装
    // builder 更直观；新增 ui 布尔后超过 clippy 默认阈值。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cwd: String,
        command_registry: CommandRegistry,
        skills: Vec<SkillMetadata>,
        lc: &crate::i18n::LcRegistry,
        detail_enabled: bool,
        diff_enabled: bool,
        hook_rows_visible: bool,
        render_config: Option<&crate::config::AppConfig>,
    ) -> Self {
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(80);
        let _ = render_tx.try_send(crate::ui::render_thread::RenderEvent::SetDetailStrategy(
            render_config
                .and_then(|config| config.detail_render_strategy.clone())
                .unwrap_or_else(|| "auto".to_owned()),
        ));
        let commands = CommandSystem::new(command_registry, skills.clone(), lc);
        let mut messages = MessageState::new(
            cwd.clone(),
            render_tx.clone(),
            std::sync::Arc::clone(&render_cache),
            std::sync::Arc::clone(&render_notify),
        );
        if let Some(mode) = render_config.and_then(|config| config.streaming_mode.as_deref()) {
            messages.pipeline.init_streaming_mode_from_config(mode);
        }
        // 渲染线程的 diff/detail 标志出生即为 false；先同步主视口的配置初值。
        // 后续内容快照会原子携带当前视口的实际显示状态。
        if diff_enabled {
            let _ = render_tx.try_send(crate::ui::render_thread::RenderEvent::ToggleDiff(true));
        }
        if detail_enabled {
            let _ = render_tx.try_send(crate::ui::render_thread::RenderEvent::ToggleDetail(true));
        }
        // hook 行开关的缺省值是「展示」，渲染线程出生即为 true；只有配置显式关闭时
        // 才需要补发事件，否则配置的 false 到不了渲染线程（同 diff_enabled 的门）。
        if !hook_rows_visible {
            let _ =
                render_tx.try_send(crate::ui::render_thread::RenderEvent::ToggleHookRows(false));
        }
        Self {
            ui: UiState::new(
                super::build_textarea(false),
                &cwd,
                detail_enabled,
                diff_enabled,
                hook_rows_visible,
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
            main_display: crate::ui::render_thread::DisplayFlags {
                detail_mode: detail_enabled,
                diff_visible: diff_enabled,
                show_tool_messages: false,
            },
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
