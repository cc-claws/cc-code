use tui_textarea::{Input, Key};

use crate::app::{
    panel_manager::{EventResult, PanelContext},
    plugin_panel::{DetailAction, PluginPanel},
};

impl PluginPanel {
    pub(crate) fn handle_installed_detail(
        &mut self,
        input: Input,
        ctx: &PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Up, .. } => {
                if self.detail_cursor > 0 {
                    self.detail_cursor -= 1;
                }
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                let max = DetailAction::ALL.len().saturating_sub(1);
                if self.detail_cursor < max {
                    self.detail_cursor += 1;
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.do_detail_action(ctx);
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.detail_index = None;
                self.detail_cursor = 0;
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
