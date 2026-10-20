use std::any::Any;

use ratatui::{layout::Rect, Frame};
use tui_textarea::Input;

use super::{
    build_textarea, panel_component::PanelComponent, panel_manager::EventResult,
    panel_manager::PanelContext, panel_manager::PanelKind, App,
};

/// 输入历史面板：把 `Ctrl+↑/↓` 逐条回溯的那份历史摊开成列表，一次能看到多条。
///
/// **数据源与输入框回溯完全同一份**（`ui.input_history`，含跨会话持久化），
/// 面板只持有打开时的快照，避免两处状态互相干扰。
#[derive(Clone)]
pub struct InputHistoryPanel {
    /// 历史快照（最新在前，与 `ui.input_history` 同序）
    pub entries: Vec<String>,
    /// 光标位置
    pub cursor: usize,
}

/// 翻页步长
const PAGE_STEP: isize = 10;
/// 面板高度上限 / 下限（条数少时不开空面板，多时封顶）
const MAX_ROWS: usize = 18;
const MIN_ROWS: usize = 3;

impl InputHistoryPanel {
    pub fn new(entries: Vec<String>) -> Self {
        Self { entries, cursor: 0 }
    }

    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        self.cursor =
            ((self.cursor as isize + delta).rem_euclid(self.entries.len() as isize)) as usize;
    }

    pub fn selected(&self) -> Option<&str> {
        self.entries.get(self.cursor).map(String::as_str)
    }
}

impl PanelComponent for InputHistoryPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::InputHistory
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. }
            | Input {
                key: Key::Char('k'),
                ..
            } => {
                self.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. }
            | Input {
                key: Key::Char('j'),
                ..
            } => {
                self.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::PageUp, ..
            } => {
                self.move_cursor(-PAGE_STEP);
                EventResult::Consumed
            }
            Input {
                key: Key::PageDown, ..
            } => {
                self.move_cursor(PAGE_STEP);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let Some(text) = self.selected().map(str::to_owned) else {
                    return EventResult::ClosePanel;
                };
                fill_input(ctx, &text);
                EventResult::ClosePanel
            }
            _ => EventResult::Consumed,
        }
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        let rows = self.total().clamp(MIN_ROWS, MAX_ROWS) as u16;
        rows.min(screen_height.saturating_sub(4))
            .max(MIN_ROWS as u16)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::input_history::render_input_history_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("\u{2191}\u{2193}".to_string(), lc.tr("key-select")),
            ("Enter".to_string(), lc.tr("key-fill-input")),
            ("Esc".to_string(), lc.tr("key-close")),
        ]
    }
}

/// 把选中的历史写进输入框，并清掉回溯态。
///
/// 与 `Ctrl+↑/↓` 的回填语义保持一致：**只填入、不代发**，用户可以改完再回车。
fn fill_input(ctx: &mut PanelContext<'_>, text: &str) {
    let ui = &mut ctx.session_mgr.current_mut().ui;
    ui.history_index = None;
    ui.draft_input = None;
    ui.textarea = build_textarea(ui.loading);
    ui.textarea.insert_str(text);
}

#[cfg(test)]
#[path = "input_history_panel_test.rs"]
mod tests;
