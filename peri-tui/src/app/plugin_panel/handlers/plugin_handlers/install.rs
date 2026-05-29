use crate::app::{
    panel_manager::PanelContext,
    plugin_panel::{DetailAction, PluginPanel},
    AgentEvent,
};

use peri_middlewares::plugin::InstallScope;

impl PluginPanel {
    /// 异步安装 Discover 视图中当前光标处的插件
    pub(super) fn spawn_install_current(&mut self, ctx: &PanelContext<'_>) {
        let plugin = match self.discover_current_plugin() {
            Some(p) => p,
            None => return,
        };
        let name = plugin.name.clone();
        let marketplace = plugin.marketplace.clone();
        let plugin_id = plugin.plugin_id.clone();
        self.installing.insert(plugin_id.clone());

        let project_dir = std::path::PathBuf::from(&ctx.services.cwd);
        let claude_dir = peri_middlewares::plugin::claude_home();
        let cache_dir = peri_middlewares::plugin::marketplaces_cache_dir();
        let tx = ctx.services.bg_event_tx.clone();
        tokio::spawn(async move {
            let result = peri_middlewares::plugin::install_plugin(
                &name,
                &marketplace,
                InstallScope::User,
                &cache_dir,
                &claude_dir,
                Some(&project_dir),
            )
            .await;
            let _ = tx.try_send(AgentEvent::PluginActionCompleted {
                plugin_id,
                action: "install".to_string(),
                success: result.is_ok(),
                message: result
                    .map(|_| String::new())
                    .unwrap_or_else(|e| e.to_string()),
            });
        });
    }

    /// 执行详情页当前操作（ToggleEnabled/Uninstall/BackToList）
    pub(super) fn do_detail_action(&mut self, ctx: &PanelContext<'_>) {
        let action = DetailAction::ALL.get(self.detail_cursor).copied();
        let entry_idx = self.detail_index;
        match action {
            Some(DetailAction::ToggleEnabled) => {
                if let Some(idx) = entry_idx {
                    if let Some(entry) = self.entries.get_mut(idx) {
                        entry.enabled = !entry.enabled;
                    }
                }
                self.persist_enabled_state(ctx.services.claude_settings_override.as_ref());
            }
            Some(DetailAction::Uninstall) => {
                if let Some(idx) = entry_idx {
                    let id = self.entries.get(idx).map(|e| e.id.clone());
                    if let Some(id) = id {
                        self.confirm_delete = Some(id);
                    }
                }
            }
            Some(DetailAction::BackToList) => {
                self.detail_index = None;
                self.detail_cursor = 0;
            }
            None => {}
        }
    }
}
