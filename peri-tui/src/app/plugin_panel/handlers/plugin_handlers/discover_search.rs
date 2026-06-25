use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::PluginPanel,
};

impl PluginPanel {
    pub(crate) fn handle_discover_searching(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input {
                key: Key::Char(c), ..
            } => {
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
                key: Key::Backspace,
                ..
            } => {
                self.discover_search.backspace();
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input { key: Key::Up, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(1);
                EventResult::Consumed
            }
            Input { key: Key::Left, .. } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.view.prev();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input {
                key: Key::Right, ..
            } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.view.next();
                self.sync_current_view_items();
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.discover_searching = false;
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
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.spawn_install_current(ctx);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
