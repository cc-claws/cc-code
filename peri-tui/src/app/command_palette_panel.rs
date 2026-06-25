use std::any::Any;

use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::Input;

use crate::config::PeriConfig;

use super::{
    panel_component::PanelComponent,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

// ─── ProviderModel 条目 ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProviderModelEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub alias: String,
    pub model_name: String,
}

// ─── 列表行类型 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ListRow {
    Header(String),
    Entry(usize),
}

// ─── 选择阶段 ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SelectPhase {
    Model,
    Effort,
}

pub const EFFORT_OPTIONS: &[(&str, &str)] = &[
    ("low", "Low - Fast, less reasoning"),
    ("medium", "Medium - Balanced"),
    ("high", "High - More reasoning"),
    ("xhigh", "XHigh - Deep reasoning"),
    ("max", "Max - Maximum reasoning"),
];

// ─── CommandPalettePanel ─────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CommandPalettePanel {
    pub entries: Vec<ProviderModelEntry>,
    pub rows: Vec<ListRow>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub phase: SelectPhase,
    pub selected_model: Option<ProviderModelEntry>,
    pub effort_cursor: usize,
    pub current_effort: String,
}

impl CommandPalettePanel {
    pub fn from_config(cfg: &PeriConfig) -> Self {
        let mut entries = Vec::new();
        for provider in &cfg.config.providers {
            let p_name = provider.display_name().to_string();
            for alias in &["opus", "sonnet", "haiku"] {
                if let Some(model) = provider.models.get_model(alias) {
                    if !model.is_empty() {
                        entries.push(ProviderModelEntry {
                            provider_id: provider.id.clone(),
                            provider_name: p_name.clone(),
                            alias: alias.to_string(),
                            model_name: model.to_string(),
                        });
                    }
                }
            }
        }

        let current_effort = cfg
            .config
            .thinking
            .as_ref()
            .map(|t| t.effort.as_str())
            .unwrap_or("high");
        let effort_cursor = EFFORT_OPTIONS
            .iter()
            .position(|(k, _)| *k == current_effort)
            .unwrap_or(2);

        // 构建 rows（分组 + 条目）
        let mut rows = Vec::new();
        let mut last_provider = String::new();
        for (idx, entry) in entries.iter().enumerate() {
            if entry.provider_name != last_provider {
                rows.push(ListRow::Header(entry.provider_name.clone()));
                last_provider = entry.provider_name.clone();
            }
            rows.push(ListRow::Entry(idx));
        }

        let mut panel = Self {
            entries,
            rows,
            cursor: 0,
            scroll_offset: 0,
            phase: SelectPhase::Model,
            selected_model: None,
            effort_cursor,
            current_effort: current_effort.to_string(),
        };

        // 光标定位到当前激活
        let active_provider = &cfg.config.active_provider_id;
        let active_alias = &cfg.config.active_alias;
        if let Some(pos) = panel.rows.iter().position(|row| match row {
            ListRow::Entry(idx) => {
                let e = &panel.entries[*idx];
                &e.provider_id == active_provider && &e.alias == active_alias
            }
            _ => false,
        }) {
            panel.cursor = pos;
        }

        panel
    }

    fn move_up(&mut self) {
        if self.cursor == 0 {
            return;
        }
        for i in (0..self.cursor).rev() {
            if matches!(self.rows[i], ListRow::Entry(_)) {
                self.cursor = i;
                return;
            }
        }
    }

    fn move_down(&mut self) {
        for i in (self.cursor + 1)..self.rows.len() {
            if matches!(self.rows[i], ListRow::Entry(_)) {
                self.cursor = i;
                return;
            }
        }
    }

    fn visible_rows(&self, area_height: u16) -> usize {
        // 减去边框(2) + 提示行(1) + 分隔线(1)
        area_height.saturating_sub(4) as usize
    }

    fn ensure_visible(&mut self, visible: usize) {
        if visible == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + visible {
            self.scroll_offset = self.cursor - visible + 1;
        }
    }

    pub fn selected(&self) -> Option<&ProviderModelEntry> {
        match self.rows.get(self.cursor)? {
            ListRow::Entry(idx) => self.entries.get(*idx),
            _ => None,
        }
    }

    pub fn cursor_is_entry(&self) -> bool {
        matches!(self.rows.get(self.cursor), Some(ListRow::Entry(_)))
    }

    fn enter_effort_phase(&mut self) {
        self.phase = SelectPhase::Effort;
        self.scroll_offset = 0;
    }

    fn confirm(&self, ctx: &mut PanelContext<'_>) {
        let Some(ref entry) = self.selected_model else {
            return;
        };
        let (effort_key, _) = EFFORT_OPTIONS[self.effort_cursor];
        apply_selection(entry, effort_key, ctx);
    }
}

// ─── PanelComponent ──────────────────────────────────────────────────────────

impl PanelComponent for CommandPalettePanel {
    fn kind(&self) -> PanelKind {
        PanelKind::CommandPalette
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;

        // ── Effort 阶段 ──
        if self.phase == SelectPhase::Effort {
            return match input {
                Input { key: Key::Esc, .. }
                | Input { key: Key::Left, .. }
                | Input {
                    key: Key::Backspace, ..
                } => {
                    self.phase = SelectPhase::Model;
                    EventResult::Consumed
                }
                Input { key: Key::Up, .. } => {
                    if self.effort_cursor > 0 {
                        self.effort_cursor -= 1;
                    }
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    if self.effort_cursor + 1 < EFFORT_OPTIONS.len() {
                        self.effort_cursor += 1;
                    }
                    EventResult::Consumed
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    self.confirm(ctx);
                    EventResult::ClosePanel
                }
                Input {
                    key: Key::Char(c), ..
                } if c.is_ascii_digit() => {
                    let idx = c.to_digit(10).unwrap() as usize;
                    if idx >= 1 && idx <= EFFORT_OPTIONS.len() {
                        self.effort_cursor = idx - 1;
                        self.confirm(ctx);
                        return EventResult::ClosePanel;
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            };
        }

        // ── Model 阶段 ──
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                self.move_up();
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.move_down();
                EventResult::Consumed
            }
            // Enter / Right(→): 进入 effort 选择
            Input {
                key: Key::Enter, ..
            }
            | Input {
                key: Key::Right, ..
            } => {
                if let Some(entry) = self.selected().cloned() {
                    self.selected_model = Some(entry);
                    self.enter_effort_phase();
                }
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let relative_y = mouse.row.saturating_sub(area.y);

            if self.phase == SelectPhase::Effort {
                if relative_y >= 3 {
                    let clicked = (relative_y - 3) as usize;
                    if clicked < EFFORT_OPTIONS.len() {
                        self.effort_cursor = clicked;
                        self.confirm(ctx);
                        return EventResult::ClosePanel;
                    }
                }
            } else {
                if relative_y >= 4 {
                    let clicked_row = (relative_y - 4) as usize + self.scroll_offset;
                    if clicked_row < self.rows.len() {
                        self.cursor = clicked_row;
                        if self.cursor_is_entry() {
                            if let Some(entry) = self.selected().cloned() {
                                self.selected_model = Some(entry);
                                self.enter_effort_phase();
                            }
                            return EventResult::Consumed;
                        }
                    }
                }
            }
        }
        EventResult::NotConsumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        if self.phase == SelectPhase::Effort {
            if lines < 0 {
                self.effort_cursor = self.effort_cursor.saturating_sub((-lines) as usize);
            } else {
                let new = self.effort_cursor + lines as usize;
                if new < EFFORT_OPTIONS.len() {
                    self.effort_cursor = new;
                }
            }
        } else {
            if lines < 0 {
                for _ in 0..(-lines) {
                    self.move_up();
                }
            } else {
                for _ in 0..lines {
                    self.move_down();
                }
            }
        }
        EventResult::Consumed
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        let max = (screen_height as f64 * 0.85) as u16;
        let needed = if self.phase == SelectPhase::Effort {
            10
        } else {
            self.rows.len() as u16 + 5
        };
        needed.min(max).max(10)
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        let visible = self.visible_rows(area.height);
        self.ensure_visible(visible);
        crate::ui::main_ui::panels::command_palette::render_command_palette(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        if self.phase == SelectPhase::Effort {
            vec![
                ("↑↓".to_string(), lc.tr("key-move")),
                ("1-5".to_string(), "Quick".to_string()),
                ("Enter".to_string(), lc.tr("key-confirm")),
                ("Esc/←".to_string(), "Back".to_string()),
            ]
        } else {
            vec![
                ("↑↓".to_string(), lc.tr("key-move")),
                ("Enter/→".to_string(), "Effort".to_string()),
                ("Esc".to_string(), lc.tr("key-close")),
            ]
        }
    }
}

// ─── 选中应用逻辑 ─────────────────────────────────────────────────────────────

fn apply_selection(entry: &ProviderModelEntry, effort: &str, ctx: &mut PanelContext<'_>) {
    let Some(cfg) = ctx.services.peri_config.as_mut() else {
        return;
    };
    cfg.config.active_provider_id = entry.provider_id.clone();
    cfg.config.active_alias = entry.alias.clone();

    let t = cfg.config.thinking.get_or_insert_with(|| {
        crate::config::ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: effort.to_string(),
            max_tokens: 32000,
        }
    });
    t.enabled = true;
    t.effort = effort.to_string();

    if let Err(e) = App::save_config(cfg, ctx.services.config_path_override.as_deref()) {
        ctx.session_mgr
            .current_mut()
            .messages
            .push_system_note(ctx.services.lc.tr_args(
                "app-config-save-failed",
                &[("error".into(), e.to_string().into())],
            ));
    }

    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
        ctx.services.provider_name = p.display_name().to_string();
        ctx.services.model_name = p.model_name().to_string();
        let cw = p.context_window();
        if cw > 0 {
            ctx.session_mgr.current_mut().agent.context_window = cw;
        }
    }

    ctx.session_mgr
        .current_mut()
        .messages
        .push_system_note(ctx.services.lc.tr_args(
            "app-model-switched",
            &[
                ("alias".into(), entry.alias.clone().into()),
                ("effort".into(), effort.into()),
            ],
        ));

    ctx.sync_acp_config();
}

#[cfg(test)]
#[path = "command_palette_panel_test.rs"]
mod tests;
