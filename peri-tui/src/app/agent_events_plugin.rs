use super::{message_pipeline::PipelineAction, plugin_panel::PluginPanel, *};

impl App {
    pub(crate) fn handle_plugin_action_completed(
        &mut self,
        plugin_id: String,
        action: String,
        success: bool,
        message: String,
    ) -> (bool, bool, bool) {
        // 从 installing/uninstalling 集合中移除
        if let Some(ref mut panel) = self.global_panels.get_mut::<PluginPanel>() {
            panel.installing.remove(&plugin_id);
            panel.uninstalling.remove(&plugin_id);
            panel.marketplace_updating.remove(&plugin_id);

            // 更新 discover 列表中的 installed 标记
            match (action.as_str(), success) {
                ("install", true) => {
                    for dp in &mut panel.discover_plugins {
                        if dp.plugin_id == plugin_id {
                            dp.installed = true;
                        }
                    }
                }
                ("uninstall", true) => {
                    // 更新 discover 列表
                    for dp in &mut panel.discover_plugins {
                        if dp.plugin_id == plugin_id {
                            dp.installed = false;
                        }
                    }
                    // 从 Installed 列表移除
                    panel.entries.retain(|e| e.id != plugin_id);
                    // 关闭详情页（如果正在查看被卸载的插件）
                    if panel.detail_index.is_some() || panel.discover_detail_index.is_some() {
                        panel.detail_index = None;
                        panel.discover_detail_index = None;
                        panel.detail_cursor = 0;
                        panel.discover_detail_cursor = 0;
                    }
                    // 调整 cursor 避免越界
                    let list_len = panel.current_list_len();
                    if panel.cursor() >= list_len {
                        let new_cursor = list_len.saturating_sub(1);
                        match panel.view {
                            super::plugin_panel::PluginPanelView::Installed
                            | super::plugin_panel::PluginPanelView::Errors => {
                                panel.installed_list.move_cursor_to(new_cursor);
                            }
                            super::plugin_panel::PluginPanelView::Discover => {
                                panel.discover_list.move_cursor_to(new_cursor);
                            }
                            super::plugin_panel::PluginPanelView::Marketplaces => {
                                panel.marketplace_list.move_cursor_to(new_cursor);
                            }
                        }
                    }
                }
                ("refresh", true) | ("add", true) => {
                    // Marketplace 刷新/添加成功，重新加载面板数据
                    // 保存当前面板状态
                    let current_view = panel.view;
                    let current_marketplace_cursor = panel.marketplace_list.cursor();
                    // 重新加载面板数据
                    self.open_plugin_panel();
                    // 恢复面板状态
                    if let Some(ref mut p) = self.global_panels.get_mut::<PluginPanel>() {
                        p.view = current_view;
                        // 确保 cursor 不越界
                        let max = p.marketplace_entries.len();
                        let restored = current_marketplace_cursor.min(max);
                        p.marketplace_list.move_cursor_to(restored);
                    }
                }
                ("install_counts_refresh", _) => {
                    // 安装量数据后台刷新完成，重新加载面板以更新排序
                    let current_view = panel.view;
                    let current_cursor = panel.cursor();
                    let current_discover_cursor = panel.discover_list.cursor();
                    let current_marketplace_cursor = panel.marketplace_list.cursor();
                    self.open_plugin_panel();
                    if let Some(ref mut p) = self.global_panels.get_mut::<PluginPanel>() {
                        p.view = current_view;
                        let restored = current_cursor.min(p.current_list_len().saturating_sub(1));
                        match p.view {
                            super::plugin_panel::PluginPanelView::Installed
                            | super::plugin_panel::PluginPanelView::Errors => {
                                p.installed_list.move_cursor_to(restored);
                            }
                            super::plugin_panel::PluginPanelView::Discover => {
                                p.discover_list.move_cursor_to(restored);
                            }
                            super::plugin_panel::PluginPanelView::Marketplaces => {
                                p.marketplace_list.move_cursor_to(restored);
                            }
                        }
                        p.discover_list.move_cursor_to(
                            current_discover_cursor
                                .min(p.discover_filtered_plugins().len().saturating_sub(1)),
                        );
                        let max = p.marketplace_entries.len();
                        let restored_marketplace = current_marketplace_cursor.min(max);
                        p.marketplace_list.move_cursor_to(restored_marketplace);
                    }
                    // 不显示系统消息
                    return (false, false, false);
                }
                _ => {}
            }
        }
        let msg = match (action.as_str(), success) {
            ("install", true) => {
                format!("Plugin installed: {}", plugin_id)
            }
            ("install", false) => {
                format!("Plugin install failed: {} ({})", plugin_id, message)
            }
            ("uninstall", true) => {
                format!("Plugin uninstalled: {}", plugin_id)
            }
            ("uninstall", false) => {
                format!("Plugin uninstall failed: {} ({})", plugin_id, message)
            }
            (_, true) => format!("Plugin action completed: {}", plugin_id),
            (_, false) => {
                format!("Plugin action failed: {} ({})", plugin_id, message)
            }
        };
        let vm = MessageViewModel::system(msg);
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }
}
