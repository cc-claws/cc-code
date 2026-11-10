use std::any::Any;

use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::{Input, Key};

use super::{
    panel_component::PanelComponent,
    panel_list::PanelList,
    panel_manager::{EventResult, PanelContext, PanelKind, PanelState},
    App,
};

/// Session kernel picker. Moving the cursor only changes the selection;
/// the backend must acknowledge Enter before the current kernel changes.
#[derive(Clone)]
pub struct KernelPanel {
    pub(crate) list: PanelList<&'static str>,
}

impl KernelPanel {
    pub fn new(current: &str) -> Self {
        let mut list = PanelList::new();
        list.set_items(
            cc_agent::kernel::ALL_SPECS
                .iter()
                .map(|spec| spec.id)
                .collect(),
        );
        if let Some(cursor) = list.items().iter().position(|id| *id == current) {
            list.move_cursor_to(cursor);
        }
        Self { list }
    }

    pub fn selected(&self) -> Option<&'static str> {
        self.list.selected().copied()
    }
}

impl App {
    /// 打开内核选择面板（Ctrl+P 快捷键与 `/kernel` 无参入口共用）。
    pub fn open_kernel_panel(&mut self) {
        let current = self.current_kernel_id().to_string();
        let panel = KernelPanel::new(&current);
        self.open_panel(PanelState::Kernel(panel));
    }
}

impl PanelComponent for KernelPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Kernel
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        match input.key {
            Key::Esc => EventResult::ClosePanel,
            Key::Up => {
                self.list.move_cursor(-1);
                EventResult::Consumed
            }
            Key::Down => {
                self.list.move_cursor(1);
                EventResult::Consumed
            }
            Key::Enter => {
                if let Some(id) = self.selected() {
                    crate::command::kernel::KernelCommand::switch(ctx, id);
                }
                EventResult::ClosePanel
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && self
                .list
                .handle_mouse_click(mouse.row, mouse.column, area, 2)
        {
            return EventResult::Consumed;
        }
        EventResult::NotConsumed
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        (5 + self.list.len()) as u16
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::kernel::render_kernel_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("↑↓".to_string(), lc.tr("key-move")),
            ("Enter".to_string(), lc.tr("key-confirm")),
            ("Esc".to_string(), lc.tr("key-close")),
        ]
    }
}

#[cfg(test)]
#[path = "kernel_panel_test.rs"]
mod tests;
