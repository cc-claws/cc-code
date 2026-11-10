use std::any::Any;

use cc_acp::worktree::{Worktree, WorktreeList};
use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::{Input, Key};

use super::{
    panel_component::PanelComponent,
    panel_list::PanelList,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

pub struct WorktreePanel {
    pub(crate) list: PanelList<Option<Worktree>>,
    pub(crate) current_cwd: String,
}

impl WorktreePanel {
    pub fn new(worktrees: WorktreeList) -> Self {
        let mut list = PanelList::new();
        let mut items = vec![None];
        items.extend(worktrees.worktrees.into_iter().map(Some));
        list.set_items(items);
        Self {
            list,
            current_cwd: worktrees.repository_root,
        }
    }
}

impl PanelComponent for WorktreePanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Worktree
    }

    fn handle_key(&mut self, input: Input, _ctx: &mut PanelContext<'_>) -> EventResult {
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
            Key::Enter => match self.list.selected() {
                Some(Some(tree)) if !tree.bare && !tree.prunable => {
                    EventResult::OpenWorktree(Some(tree.path.clone()))
                }
                Some(None) => EventResult::OpenWorktree(None),
                _ => EventResult::Consumed,
            },
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
                .handle_mouse_click(mouse.row, mouse.column, area, 0)
        {
            EventResult::Consumed
        } else {
            EventResult::NotConsumed
        }
    }

    fn handle_scroll(&mut self, delta: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.list.move_cursor(delta as isize);
        EventResult::Consumed
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        (self.list.len().saturating_add(2).min(u16::MAX as usize) as u16)
            .min(screen_height / 2)
            .max(3)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::worktree::render_worktree_panel(f, self, app, area);
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
