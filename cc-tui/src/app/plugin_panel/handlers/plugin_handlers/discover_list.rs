use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::PluginPanel,
};

impl PluginPanel {
    pub(crate) fn handle_discover_list(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.discover_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.discover_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Char(c), ..
            } => {
                self.discover_searching = true;
                self.discover_search.insert(c);
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.spawn_install_current(ctx);
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }

    pub(crate) fn handle_installed_list(
        &mut self,
        input: Input,
        ctx: &PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input {
                key: Key::Right, ..
            }
            | Input { key: Key::Tab, .. } => {
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.installed_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.installed_list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Char(' '),
                ..
            } => {
                if let Some(&entry_idx) = self.visible_indices().get(self.installed_list.cursor()) {
                    if let Some(entry) = self.entries.get_mut(entry_idx) {
                        entry.enabled = !entry.enabled;
                    }
                }
                self.persist_enabled_state(ctx.services.claude_settings_override.as_ref());
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(&entry_idx) = self.visible_indices().get(self.installed_list.cursor()) {
                    self.detail_index = Some(entry_idx);
                    self.detail_cursor = 0;
                }
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            _ => EventResult::Consumed,
        }
    }
}
