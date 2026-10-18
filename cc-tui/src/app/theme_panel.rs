use std::any::Any;

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    Frame,
};
use tui_textarea::Input;

use crate::ui::theme::{self, ThemeName};

use super::{
    panel_component::PanelComponent,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

// ─── ThemePanel ──────────────────────────────────────────────────────────────

/// /theme 主题选择面板：上下移动光标时实时预览主题，Enter 确认并持久化，
/// Esc 恢复打开面板前的主题（与 claude-code-best ThemePicker 行为一致）。
#[derive(Clone)]
pub struct ThemePanel {
    /// 光标所在行（ThemeName::ALL 的下标）
    pub cursor: usize,
    /// 打开面板时的主题，Esc 时恢复
    pub original: ThemeName,
}

impl ThemePanel {
    pub fn new() -> Self {
        let current = theme::current_name();
        let cursor = ThemeName::ALL
            .iter()
            .position(|t| *t == current)
            .unwrap_or(0);
        Self {
            cursor,
            original: current,
        }
    }

    /// 光标当前指向的主题
    pub fn selected(&self) -> ThemeName {
        ThemeName::ALL[self.cursor.min(ThemeName::ALL.len() - 1)]
    }

    /// 将选中主题写入 config 并保存，推送系统提示（Enter 确认路径）。
    fn apply_and_close(panel: &ThemePanel, ctx: &mut PanelContext<'_>) {
        let name = panel.selected();
        theme::set_current(name);

        let mut save_err: Option<String> = None;
        if let Some(cfg) = ctx.services.peri_config.as_mut() {
            cfg.config.theme = Some(name.setting_value().to_string());
            if let Err(e) = App::save_config(cfg, ctx.services.config_path_override.as_deref()) {
                save_err = Some(e.to_string());
            }
        }

        ctx.session_mgr
            .current_mut()
            .messages
            .push_system_note(ctx.services.lc.tr_args(
                "app-theme-set",
                &[("name".into(), name.setting_value().into())],
            ));
        if let Some(e) = save_err {
            ctx.session_mgr
                .current_mut()
                .messages
                .push_system_note(ctx.services.lc.tr_args(
                    "app-config-save-failed",
                    &[("error".into(), e.into())],
                ));
        }
    }
}

impl Default for ThemePanel {
    fn default() -> Self {
        Self::new()
    }
}

// ─── PanelComponent 实现 ─────────────────────────────────────────────────────

impl PanelComponent for ThemePanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Theme
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => {
                // 取消：恢复打开面板前的主题
                theme::set_current(self.original);
                EventResult::ClosePanel
            }
            Input { key: Key::Up, .. } => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                theme::set_current(self.selected());
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                if self.cursor < ThemeName::ALL.len() - 1 {
                    self.cursor += 1;
                }
                theme::set_current(self.selected());
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                Self::apply_and_close(self, ctx);
                EventResult::ClosePanel
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
            // border_top=1 + 描述行 2 行（描述 + 空行），主题行从 relative_y=3 开始
            let relative_y = mouse.row.saturating_sub(area.y);
            if relative_y >= 3 {
                let clicked = (relative_y - 3) as usize;
                if clicked < ThemeName::ALL.len() {
                    self.cursor = clicked;
                    return self.handle_key(
                        Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                        ctx,
                    );
                }
            }
        }
        EventResult::NotConsumed
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        // 边框 2 + 描述 1 + 空行 1 + 主题行 + 底部提示 1
        (5 + ThemeName::ALL.len()) as u16
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::theme::render_theme_panel(f, self, app, area);
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
#[path = "theme_panel_test.rs"]
mod tests;
