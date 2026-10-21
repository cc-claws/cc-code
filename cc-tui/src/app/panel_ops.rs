#[cfg(any(test, feature = "headless"))]
use super::*;

// panel_ops.rs — test helpers for App.
//
// Panel operation functions have been split into per-panel submodules:
//   panel_model, panel_login, panel_config, panel_status,
//   panel_memory, panel_plugin, panel_agent, panel_hooks.
// Each submodule contributes inherent impl App blocks directly.

// ─── 测试辅助方法（仅在 cfg(any(test, feature = "headless")) 下编译）──────────

#[cfg(any(test, feature = "headless"))]
impl App {
    /// 向事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent) {
        self.session_mgr
            .current_mut()
            .agent
            .agent_event_queue
            .push(event);
    }

    /// 强制从 pipeline 规范状态重建 view_messages 并发送 RenderEvent。
    /// 用于 headless 测试：确保流式缓冲区内容（throttle 未触发的 chunk）也被渲染。
    pub fn flush_rebuild(&mut self) {
        let prefix_len = self.session_mgr.current_mut().messages.round_start_vm_idx;
        let action = self
            .session_mgr
            .current_mut()
            .messages
            .pipeline
            .build_rebuild_all(prefix_len);
        self.apply_pipeline_action(action);
    }

    /// 批量处理队列中所有待处理事件，复用 handle_agent_event 逻辑
    pub fn process_pending_events(&mut self) {
        let events: Vec<AgentEvent> =
            std::mem::take(&mut self.session_mgr.current_mut().agent.agent_event_queue);
        for event in events {
            let (_updated, should_break, should_return) = self.handle_agent_event(event);
            if should_return || should_break {
                break;
            }
        }
    }

    /// 构造 Headless 测试用 App，使用 ratatui TestBackend 替代真实终端
    pub async fn new_headless(
        width: u16,
        height: u16,
    ) -> (App, crate::ui::headless::HeadlessHandle) {
        use crate::thread::SqliteThreadStore;
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend should never fail");

        // 启动渲染线程
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(width);
        let _ = render_tx.try_send(crate::ui::render_thread::RenderEvent::SetDetailStrategy(
            "auto".to_owned(),
        ));

        // SQLite 会收紧父目录权限，不能把数据库直接放在共享 Temp 根目录。
        let db_dir =
            std::env::temp_dir().join(format!("zen-threads-test-{}", uuid::Uuid::now_v7()));
        let thread_store: Arc<dyn ThreadStore> = Arc::new(
            SqliteThreadStore::new(db_dir.join("threads.db"))
                .await
                .expect("无法创建测试用 SQLite 数据库"),
        );
        let shell_command_store = Arc::new(crate::shell_history::ShellCommandStore::new(
            std::env::temp_dir().join(format!("peri-shell-test-{}.jsonl", uuid::Uuid::now_v7())),
        ));

        // 将配置路径重定向到临时目录，防止测试污染全局 ~/.cc-code/settings.json
        let test_config_path = std::env::temp_dir().join(format!(
            "zen-config-test-{}/settings.json",
            uuid::Uuid::now_v7()
        ));

        let (bg_event_tx, bg_event_rx) = tokio::sync::mpsc::channel(128);

        let lc = crate::i18n::LcRegistry::default();
        let commands =
            super::CommandSystem::new(crate::command::default_registry(), Vec::new(), &lc);

        let session = super::ChatSession {
            ui: super::UiState::new(super::build_textarea(false), "/tmp", false, false),
            messages: super::MessageState::new(
                "/tmp".to_string(),
                render_tx.clone(),
                std::sync::Arc::clone(&render_cache),
                std::sync::Arc::clone(&render_notify),
            ),
            session_panels: super::panel_manager::PanelManager::new(),
            commands,
            metadata: super::SessionMetadata::new(),
            agent: super::AgentComm::default(),
            langfuse: super::LangfuseState::default(),
            current_thread_id: None,
            todo_items: Vec::new(),
            background_agents: Vec::new(),
            focused_instance_id: None,
            subagent_sessions: std::collections::HashMap::new(),
            subagent_control_prefix: None,
            subagent_interrupt_at: None,
            pending_thread_approvals: std::collections::VecDeque::new(),
            approval_source_agent_id: None,
            main_scroll_position: (usize::MAX, true),
            main_input_draft: None,
            spinner_state: cc_widgets::SpinnerState::new(cc_widgets::SpinnerMode::Idle),
            shell_pool: super::ShellCommandPool::default(),
            background_shells: Vec::new(),
            agent_shells: Vec::new(),
            pending_bg_shell_notifications: std::collections::VecDeque::new(),
            bg_shell_steering: Default::default(),
            latest_recap: None,
        };

        let app = App {
            session_mgr: super::SessionManager::new(session),
            services: super::ServiceRegistry {
                peri_config: None,
                cwd: "/tmp".to_string(),
                provider_name: "test".to_string(),
                model_name: "test-model".to_string(),
                // 测试夹具：headless App 不接真实 HITL 链路，显式用 Bypass 避免测试被审批阻塞。
                // 注意：这**不是**面向用户的默认档 —— 用户默认档在 main.rs 为 Auto（见安全加固）。
                permission_mode: cc_middlewares::prelude::SharedPermissionMode::new(
                    cc_middlewares::prelude::PermissionMode::Bypass,
                ),
                thread_store,
                shell_command_store,
                mcp_pool: None,
                mcp_init_rx: None,
                cron: super::CronState::default(),
                plugin_data: None,
                bg_event_tx,
                bg_event_rx: Some(bg_event_rx),
                config_path_override: Some(test_config_path),
                claude_settings_override: Some(std::env::temp_dir().join(format!(
                    "claude-settings-test-{}.json",
                    uuid::Uuid::now_v7()
                ))),
                resource_monitor: parking_lot::Mutex::new(
                    super::service_registry::ProcessResourceMonitor::new(),
                ),
                lc: crate::i18n::LcRegistry::default(),
                channel_state: None,
                git_branch_cache: parking_lot::Mutex::new(
                    super::service_registry::GitBranchCache::new(),
                ),
                git_branch_poller: parking_lot::Mutex::new(
                    super::service_registry::GitBranchPoller::new(),
                ),
                // headless 测试默认关闭更新检查，避免测试进程发起真实网络请求
                update_checker: parking_lot::Mutex::new(super::UpdateChecker::new()),
                panic_notify_rx: None,
            },
            global_panels: PanelManager::new(),
            global_ui: super::GlobalUiState::new(),
            focused: true,
            auto_recap: super::recap_auto::AutoRecapState::new(false),
            // 测试构造：调度器指向临时路径且不推进会话计数，避免污染真实 tip 历史
            tip_scheduler: crate::ui::tips::TipScheduler::new(
                std::env::temp_dir().join("spinner-tips-test.json"),
            ),
            spinner_tip_state: crate::ui::tips::SpinnerTipState::default(),
            acp_client: None,
            agent_shell_registrations_rx: None,
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}

#[cfg(test)]
#[path = "panel_ops_test.rs"]
mod tests;
