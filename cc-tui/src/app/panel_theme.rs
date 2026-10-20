use super::{message_pipeline::PipelineAction, *};

impl App {
    /// 打开 /theme 面板
    pub fn open_theme_panel(&mut self) {
        self.services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = ThemePanel::new();
        self.open_panel(PanelState::Theme(panel));
    }

    /// 关闭 /theme 面板（不保存）
    pub fn close_theme_panel(&mut self) {
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::Theme);
    }

    /// 直接应用主题（/theme <name> 参数路径）：切换当前主题、持久化到配置、
    /// 推送系统提示。与面板 Enter 确认的行为一致。
    pub fn apply_theme(&mut self, name: crate::ui::theme::ThemeName) {
        crate::ui::theme::set_current(name);
        let save_result = {
            let cfg = self
                .services
                .peri_config
                .get_or_insert_with(PeriConfig::default);
            cfg.config.theme = Some(name.setting_value().to_string());
            Self::save_config(cfg, self.services.config_path_override.as_deref())
        };
        let note = self.services.lc.tr_args(
            "app-theme-set",
            &[("name".into(), name.setting_value().into())],
        );
        self.session_mgr
            .current_mut()
            .messages
            .push_system_note(note);
        if let Err(e) = save_result {
            let note = self.services.lc.tr_args(
                "app-config-save-failed",
                &[("error".into(), e.to_string().into())],
            );
            self.session_mgr
                .current_mut()
                .messages
                .push_system_note(note);
        }

        // markdown 正文的 `rendered` 是构建 ViewModel 时的快照，主题切换后必须全量
        // 重建才会换成新配色（CC 那边由 React 依赖 theme 自动重渲染）。
        // 流式进行中不重建，避免打断当前轮次——那部分内容每帧都会重渲染。
        let streaming = {
            let p = &self.session_mgr.current().messages.pipeline;
            p.has_streaming_content() || p.has_pending_tool_calls()
        };
        if !streaming {
            let messages = self
                .session_mgr
                .current()
                .messages
                .pipeline
                .completed_messages()
                .to_vec();
            let cwd = self.services.cwd.clone();
            let view_msgs =
                super::message_pipeline::MessagePipeline::messages_to_view_models(&messages, &cwd);
            self.session_mgr.current_mut().messages.round_start_vm_idx = 0;
            self.apply_pipeline_action(PipelineAction::RebuildAll {
                prefix_len: 0,
                tail_vms: view_msgs,
            });
        }
    }
}
