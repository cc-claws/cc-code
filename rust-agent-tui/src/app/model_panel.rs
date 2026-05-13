use std::any::Any;

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use crate::config::{PeriConfig, ThinkingConfig};

use super::panel_component::PanelComponent;
use super::panel_list::PanelList;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

// ─── AliasTab 枚举 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AliasTab {
    Opus,
    Sonnet,
    Haiku,
}

impl AliasTab {
    pub fn label(&self) -> &str {
        match self {
            Self::Opus => "Opus",
            Self::Sonnet => "Sonnet",
            Self::Haiku => "Haiku",
        }
    }

    pub fn to_key(&self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Sonnet => "sonnet",
            Self::Haiku => "haiku",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Opus => "Most capable for complex work",
            Self::Sonnet => "Balanced performance and speed",
            Self::Haiku => "Fastest for quick answers",
        }
    }
}

// ─── 行索引常量 ─────────────────────────────────────────────────────────────────

pub const ROW_OPUS: usize = 0;
pub const ROW_SONNET: usize = 1;
pub const ROW_HAIKU: usize = 2;
pub const ROW_EFFORT: usize = 3;
pub const ROW_COUNT: usize = 4;

// ─── ModelPanel ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ModelPanel {
    /// 当前激活 Provider 的显示名称
    pub provider_name: String,
    /// 当前选中的级别
    pub active_tab: AliasTab,
    /// Thinking effort 缓冲 "low" / "medium" / "high"
    pub buf_thinking_effort: String,
    /// 光标/滚动状态管理
    pub(crate) list: PanelList<AliasTab>,
}

impl ModelPanel {
    pub fn from_config(cfg: &PeriConfig) -> Self {
        let active_tab = match cfg.config.active_alias.as_str() {
            "sonnet" => AliasTab::Sonnet,
            "haiku" => AliasTab::Haiku,
            _ => AliasTab::Opus,
        };

        let provider_name = cfg
            .config
            .providers
            .iter()
            .find(|p| p.id == cfg.config.active_provider_id)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();

        let cursor = match active_tab {
            AliasTab::Opus => ROW_OPUS,
            AliasTab::Sonnet => ROW_SONNET,
            AliasTab::Haiku => ROW_HAIKU,
        };

        let effort = cfg
            .config
            .thinking
            .as_ref()
            .map(|t| t.effort.clone())
            .unwrap_or_else(|| "high".to_string());

        let mut list = PanelList::new();
        list.set_items(vec![AliasTab::Opus, AliasTab::Sonnet, AliasTab::Haiku]);
        // PanelList 管理 3 个 AliasTab，但实际有 4 行（含 Effort）
        // cursor 直接用 list 管理，第 4 行（Effort）通过 cursor == 3 处理
        for _ in 0..cursor {
            list.move_cursor(1);
        }

        Self {
            provider_name,
            active_tab,
            buf_thinking_effort: effort,
            list,
        }
    }

    /// 光标位置
    pub fn cursor(&self) -> usize {
        self.list.cursor()
    }

    /// 循环切换 effort：low → medium → high → xhigh → max → low（任意光标位置可切换）
    pub fn cycle_effort(&mut self, reverse: bool) {
        if reverse {
            self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                "low" => "max".to_string(),
                "max" => "xhigh".to_string(),
                "xhigh" => "high".to_string(),
                "high" => "medium".to_string(),
                _ => "low".to_string(),
            };
        } else {
            self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                "low" => "medium".to_string(),
                "medium" => "high".to_string(),
                "high" => "xhigh".to_string(),
                "xhigh" => "max".to_string(),
                _ => "low".to_string(),
            };
        }
    }

    /// 将面板状态写入 PeriConfig（alias + thinking）
    pub fn apply_to_config(&self, cfg: &mut PeriConfig) {
        cfg.config.active_alias = self.active_tab.to_key().to_string();
        let t = cfg.config.thinking.get_or_insert_with(|| ThinkingConfig {
            enabled: true,
            budget_tokens: 8000,
            effort: self.buf_thinking_effort.clone(),
        });
        t.enabled = true;
        t.effort = self.buf_thinking_effort.clone();
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for ModelPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Model
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                // clamp 模式，不循环
                self.list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.list.move_cursor(1);
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => match self.cursor() {
                ROW_OPUS => {
                    self.active_tab = AliasTab::Opus;
                    Self::apply_and_close(self, ctx);
                    EventResult::ClosePanel
                }
                ROW_SONNET => {
                    self.active_tab = AliasTab::Sonnet;
                    Self::apply_and_close(self, ctx);
                    EventResult::ClosePanel
                }
                ROW_HAIKU => {
                    self.active_tab = AliasTab::Haiku;
                    Self::apply_and_close(self, ctx);
                    EventResult::ClosePanel
                }
                ROW_EFFORT => {
                    self.cycle_effort(false);
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            },
            // Space: 切换 effort 等级（无需选中 effort 行）
            Input {
                key: Key::Char(' '),
                ..
            } => {
                self.cycle_effort(false);
                EventResult::Consumed
            }
            // ←/→: 随时切换 effort 等级
            Input { key: Key::Left, .. } => {
                self.cycle_effort(true);
                EventResult::Consumed
            }
            Input {
                key: Key::Right, ..
            } => {
                self.cycle_effort(false);
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
            // border_top=1，计算点击的行索引
            let relative_y = mouse.row.saturating_sub(area.y);
            if relative_y >= 1 {
                let clicked = (relative_y - 1) as usize;
                if clicked < ROW_COUNT {
                    // 直接设置 cursor（绕过 PanelList 的 items 长度限制）
                    // PanelList 只有 3 个 items，但实际有 4 行
                    for _ in 0..clicked.saturating_sub(self.cursor()) {
                        self.list.move_cursor(1);
                    }
                    for _ in 0..self.cursor().saturating_sub(clicked) {
                        self.list.move_cursor(-1);
                    }
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
        12
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::model::render_model_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("↑↓", "导航"),
            ("Enter", "确认"),
            ("←→/Space", "Effort"),
            ("Esc", "关闭"),
        ]
    }
}

impl ModelPanel {
    /// 将面板状态写入 config，推送系统消息，更新 provider/model 名称
    fn apply_and_close(panel: &ModelPanel, ctx: &mut PanelContext<'_>) {
        let alias_label = panel.active_tab.label().to_string();
        let effort = panel.buf_thinking_effort.clone();

        let Some(cfg) = ctx.services.peri_config.as_mut() else {
            return;
        };
        panel.apply_to_config(cfg);

        let effort_display = match effort.as_str() {
            "low" => "Low",
            "high" => "High",
            "xhigh" => "XHigh",
            "max" => "Max",
            _ => "Medium",
        };

        ctx.session_mgr.sessions[ctx.session_mgr.active]
            .messages
            .push_system_note(format!(
                "模型已切换为: {} ({} effort)",
                alias_label, effort_display
            ));

        if let Err(e) = App::save_config(cfg, ctx.services.config_path_override.as_deref()) {
            ctx.session_mgr.sessions[ctx.session_mgr.active]
                .messages
                .push_system_note(format!("配置保存失败: {}", e));
        }

        if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
            ctx.services.provider_name = p.display_name().to_string();
            ctx.services.model_name = p.model_name().to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AppConfig;
    use crate::config::ProviderConfig;

    fn make_config() -> PeriConfig {
        PeriConfig {
            schema: None,
            config: AppConfig {
                active_alias: "opus".to_string(),
                active_provider_id: "test".to_string(),
                providers: vec![ProviderConfig {
                    id: "test".to_string(),
                    name: Some("TestProvider".to_string()),
                    ..Default::default()
                }],
                thinking: Some(ThinkingConfig {
                    enabled: false,
                    budget_tokens: 8000,
                    effort: "medium".to_string(),
                }),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_from_config_defaults() {
        let cfg = make_config();
        let panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.active_tab, AliasTab::Opus);
        assert_eq!(panel.cursor(), ROW_OPUS);
        assert_eq!(panel.provider_name, "TestProvider");
        assert_eq!(panel.buf_thinking_effort, "medium");
    }

    #[test]
    fn test_from_config_sonnet() {
        let mut cfg = make_config();
        cfg.config.active_alias = "sonnet".to_string();
        let panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.active_tab, AliasTab::Sonnet);
        assert_eq!(panel.cursor(), ROW_SONNET);
    }

    #[test]
    fn test_move_cursor_clamp() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor(), ROW_OPUS);
        panel.list.move_cursor(1);
        assert_eq!(panel.cursor(), ROW_SONNET);
        panel.list.move_cursor(1);
        assert_eq!(panel.cursor(), ROW_HAIKU);
        // clamp：不再循环到 OPUS
        panel.list.move_cursor(1);
        assert_eq!(panel.cursor(), ROW_HAIKU);
        panel.list.move_cursor(-1);
        assert_eq!(panel.cursor(), ROW_SONNET);
        panel.list.move_cursor(-1);
        assert_eq!(panel.cursor(), ROW_OPUS);
        panel.list.move_cursor(-1);
        assert_eq!(panel.cursor(), ROW_OPUS, "不应循环到底部");
    }

    #[test]
    fn test_cycle_effort() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);

        assert_eq!(panel.buf_thinking_effort, "medium");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "high");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "xhigh");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "max");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "medium");

        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "max");
        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "xhigh");
        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "high");
    }

    #[test]
    fn test_cycle_effort_works_from_any_row() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor(), ROW_OPUS);
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "high");
    }

    #[test]
    fn test_apply_to_config() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.active_tab = AliasTab::Sonnet;
        panel.buf_thinking_effort = "high".to_string();

        let mut cfg2 = make_config();
        panel.apply_to_config(&mut cfg2);
        assert_eq!(cfg2.config.active_alias, "sonnet");
        assert!(cfg2.config.thinking.as_ref().unwrap().enabled);
        assert_eq!(cfg2.config.thinking.as_ref().unwrap().effort, "high");
    }

    #[test]
    fn test_apply_to_config_creates_thinking_when_none() {
        let mut cfg = PeriConfig {
            schema: None,
            config: AppConfig {
                active_alias: "opus".to_string(),
                active_provider_id: "test".to_string(),
                providers: vec![ProviderConfig {
                    id: "test".to_string(),
                    ..Default::default()
                }],
                thinking: None,
                ..Default::default()
            },
        };
        let panel = ModelPanel::from_config(&cfg);
        panel.apply_to_config(&mut cfg);
        let t = cfg.config.thinking.as_ref().unwrap();
        assert!(t.enabled);
        assert_eq!(t.effort, "high");
    }

    #[test]
    fn test_alias_tab_description() {
        assert_eq!(
            AliasTab::Opus.description(),
            "Most capable for complex work"
        );
        assert_eq!(
            AliasTab::Sonnet.description(),
            "Balanced performance and speed"
        );
        assert_eq!(AliasTab::Haiku.description(), "Fastest for quick answers");
    }
}
