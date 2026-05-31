use super::*;

impl App {
    /// 打开 /login 面板
    pub fn open_login_panel(&mut self) {
        let cfg = self
            .services
            .peri_config
            .get_or_insert_with(PeriConfig::default);
        let panel = login_panel::LoginPanel::from_config(cfg);
        self.open_panel(PanelState::Login(panel));
    }

    /// 关闭 /login 面板（不保存）
    pub fn close_login_panel(&mut self) {
        self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .close_if(PanelKind::Login);
    }

    /// 选中（激活）光标处的 Provider
    pub fn login_panel_select_provider(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let selected_name = panel
            .providers
            .get(panel.cursor())
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        panel.select_provider(cfg);
        if !selected_name.is_empty() {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .push_system_note(self.services.lc.tr_args(
                    "app-provider-activated",
                    &[("name".into(), selected_name.into())],
                ));
        }
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .push_system_note(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                ));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
        if let Some(ref acp_client) = self.acp_client {
            let acp = acp_client.clone();
            let cfg = self.services.peri_config.as_ref().unwrap().clone();
            let alias = cfg.config.active_alias.clone();
            // 同步等待 update_config 完成，确保 ACP Server 端 provider 已更新
            // 再关闭面板，避免后续 prompt 使用旧 provider
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(e) = acp.update_config(&cfg).await {
                        tracing::error!(error = %e, "login_panel: update_config failed");
                    }
                    // 额外发送 set_config_option("model") 作为 double-check，
                    // 确保 ACP Server 端的 provider 和 active_alias 完全同步
                    if let Err(e) = acp.set_config_option("model", &alias).await {
                        tracing::error!(error = %e, "login_panel: set_config_option(model) failed");
                    }
                });
            });
        }
        self.close_login_panel();
    }

    /// 保存 Login 面板的编辑/新建内容到 PeriConfig，自动激活并关闭面板
    pub fn login_panel_apply_edit(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let edit_name = panel.buf_name.clone();
        let is_new = matches!(panel.mode, login_panel::LoginPanelMode::New);
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        if !panel.apply_edit(cfg) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    self.services.lc.tr("app-provider-name-empty"),
                ));
            return;
        }
        let display = if edit_name.is_empty() {
            "Provider".to_string()
        } else {
            edit_name
        };
        // 自动激活保存的 provider
        panel.select_provider(cfg);
        let key = if is_new {
            "app-provider-created"
        } else {
            "app-provider-saved"
        };
        self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .view_messages
            .push(MessageViewModel::system(
                self.services
                    .lc
                    .tr_args(key, &[("name".into(), display.into())]),
            ));
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                )));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
        if let Some(ref acp_client) = self.acp_client {
            let acp = acp_client.clone();
            let cfg = self.services.peri_config.as_ref().unwrap().clone();
            let alias = cfg.config.active_alias.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(e) = acp.update_config(&cfg).await {
                        tracing::error!(error = %e, "login_panel_apply_edit: update_config failed");
                    }
                    if let Err(e) = acp.set_config_option("model", &alias).await {
                        tracing::error!(error = %e, "login_panel_apply_edit: set_config_option failed");
                    }
                });
            });
        }
        self.close_login_panel();
    }

    /// 确认删除光标处的 Provider
    pub fn login_panel_confirm_delete(&mut self) {
        let Some(panel) = self.session_mgr.sessions[self.session_mgr.active]
            .session_panels
            .get_mut::<login_panel::LoginPanel>()
        else {
            return;
        };
        let Some(cfg) = self.services.peri_config.as_mut() else {
            return;
        };
        let deleted_name = panel
            .providers
            .get(panel.cursor())
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        panel.confirm_delete(cfg);
        if !deleted_name.is_empty() {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-provider-deleted",
                    &[("name".into(), deleted_name.into())],
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.services.config_path_override.as_deref()) {
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.to_string().into())],
                )));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.services.provider_name = p.display_name().to_string();
            self.services.model_name = p.model_name().to_string();
        }
        if let Some(ref acp_client) = self.acp_client {
            let acp = acp_client.clone();
            let cfg = self.services.peri_config.as_ref().unwrap().clone();
            let alias = cfg.config.active_alias.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(e) = acp.update_config(&cfg).await {
                        tracing::error!(error = %e, "login_panel_confirm_delete: update_config failed");
                    }
                    if let Err(e) = acp.set_config_option("model", &alias).await {
                        tracing::error!(error = %e, "login_panel_confirm_delete: set_config_option failed");
                    }
                });
            });
        }
    }
}
