use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::{DiscoverDetailAction, PluginPanel},
    AgentEvent,
};

use peri_middlewares::plugin::InstallScope;

impl PluginPanel {
    pub(crate) fn handle_discover_detail(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Up, .. } => {
                if self.discover_detail_cursor > 0 {
                    self.discover_detail_cursor -= 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                let max = DiscoverDetailAction::ALL.len().saturating_sub(1);
                if self.discover_detail_cursor < max {
                    self.discover_detail_cursor += 1;
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let action = DiscoverDetailAction::ALL
                    .get(self.discover_detail_cursor)
                    .copied();
                let plugin_idx = self.discover_detail_index;
                match action {
                    Some(DiscoverDetailAction::InstallUser) => {
                        if let Some(dp) = plugin_idx.and_then(|i| self.discover_plugins.get(i)) {
                            let name = dp.name.clone();
                            let marketplace = dp.marketplace.clone();
                            let plugin_id = format!("{}@{}", name, marketplace);
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
                                    plugin_id: format!("{}@{}", name, marketplace),
                                    action: "install".to_string(),
                                    success: result.is_ok(),
                                    message: result
                                        .map(|_| String::new())
                                        .unwrap_or_else(|e| e.to_string()),
                                });
                            });
                        }
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    Some(DiscoverDetailAction::InstallProject) => {
                        if let Some(dp) = plugin_idx.and_then(|i| self.discover_plugins.get(i)) {
                            let name = dp.name.clone();
                            let marketplace = dp.marketplace.clone();
                            let plugin_id = format!("{}@{}", name, marketplace);
                            self.installing.insert(plugin_id.clone());
                            let project_dir = std::path::PathBuf::from(&ctx.services.cwd);
                            let claude_dir = peri_middlewares::plugin::claude_home();
                            let cache_dir = peri_middlewares::plugin::marketplaces_cache_dir();
                            let tx = ctx.services.bg_event_tx.clone();
                            tokio::spawn(async move {
                                let result = peri_middlewares::plugin::install_plugin(
                                    &name,
                                    &marketplace,
                                    InstallScope::Project,
                                    &cache_dir,
                                    &claude_dir,
                                    Some(&project_dir),
                                )
                                .await;
                                let _ = tx.try_send(AgentEvent::PluginActionCompleted {
                                    plugin_id: format!("{}@{}", name, marketplace),
                                    action: "install".to_string(),
                                    success: result.is_ok(),
                                    message: result
                                        .map(|_| String::new())
                                        .unwrap_or_else(|e| e.to_string()),
                                });
                            });
                        }
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    Some(DiscoverDetailAction::BackToList) => {
                        self.discover_detail_index = None;
                        self.discover_detail_cursor = 0;
                    }
                    None => {}
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.discover_detail_index = None;
                self.discover_detail_cursor = 0;
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
