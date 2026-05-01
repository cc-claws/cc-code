use crate::config::{ThinkingConfig, ZenConfig};

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
}

// ─── 行索引常量 ─────────────────────────────────────────────────────────────────

pub const ROW_OPUS: usize = 0;
pub const ROW_SONNET: usize = 1;
pub const ROW_HAIKU: usize = 2;
pub const ROW_THINKING: usize = 3;
pub const ROW_LOGIN: usize = 4;
pub const ROW_COUNT: usize = 5;

// ─── ModelPanel ─────────────────────────────────────────────────────────────────

pub struct ModelPanel {
    /// 当前激活 Provider 的显示名称
    pub provider_name: String,
    /// 竖向列表光标 (0..ROW_COUNT)
    pub cursor: usize,
    /// 当前选中的级别
    pub active_tab: AliasTab,
    /// Thinking 开关缓冲
    pub buf_thinking_enabled: bool,
    /// Thinking budget 缓冲（字符串，便于逐字编辑）
    pub buf_thinking_budget: String,
    /// Thinking budget 编辑光标（char-based index）
    pub cur_thinking_budget: usize,
    /// Thinking effort 缓冲 "low" / "medium" / "high"
    pub buf_thinking_effort: String,
}

impl ModelPanel {
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let active_tab = match cfg.config.active_alias.as_str() {
            "sonnet" => AliasTab::Sonnet,
            "haiku" => AliasTab::Haiku,
            _ => AliasTab::Opus,
        };

        let thinking = cfg.config.thinking.as_ref();
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

        let budget = thinking
            .map(|t| t.budget_tokens.to_string())
            .unwrap_or_else(|| "8000".to_string());
        let cur_budget = budget.chars().count();

        Self {
            provider_name,
            cursor,
            active_tab,
            buf_thinking_enabled: thinking.map(|t| t.enabled).unwrap_or(false),
            buf_thinking_budget: budget,
            cur_thinking_budget: cur_budget,
            buf_thinking_effort: thinking
                .map(|t| t.effort.clone())
                .unwrap_or_else(|| "medium".to_string()),
        }
    }

    /// 上下移动光标（循环）
    pub fn move_cursor(&mut self, delta: i32) {
        if delta > 0 {
            self.cursor = (self.cursor + 1) % ROW_COUNT;
        } else if delta < 0 {
            self.cursor = (self.cursor + ROW_COUNT - 1) % ROW_COUNT;
        }
    }

    /// 输入字符（仅 Thinking 行接受数字）
    pub fn push_char(&mut self, c: char) {
        if self.cursor == ROW_THINKING && c.is_ascii_digit() && self.buf_thinking_budget.len() < 8 {
            if self.cur_thinking_budget > self.buf_thinking_budget.len() {
                self.cur_thinking_budget = self.buf_thinking_budget.len();
            }
            self.buf_thinking_budget.insert(self.cur_thinking_budget, c);
            self.cur_thinking_budget += 1;
        }
    }

    /// 退格（仅 Thinking 行）
    pub fn pop_char(&mut self) {
        if self.cursor == ROW_THINKING && self.cur_thinking_budget > 0 && self.cur_thinking_budget <= self.buf_thinking_budget.len() {
            let bp = self.buf_thinking_budget.char_indices().nth(self.cur_thinking_budget - 1).map(|(i,_)| i);
            let nb = self.buf_thinking_budget.char_indices().nth(self.cur_thinking_budget).map(|(i,_)| i).unwrap_or(self.buf_thinking_budget.len());
            if let Some(b) = bp {
                self.buf_thinking_budget.drain(b..nb);
                self.cur_thinking_budget -= 1;
            }
        }
    }

    /// 粘贴文本（仅 Thinking 行，过滤出数字）
    pub fn paste_text(&mut self, text: &str) {
        if self.cursor == ROW_THINKING {
            for c in text.chars() {
                if c.is_ascii_digit() {
                    self.push_char(c);
                }
            }
        }
    }

    /// 切换 Thinking 开关（仅 Thinking 行）
    pub fn toggle_thinking(&mut self) {
        if self.cursor == ROW_THINKING {
            self.buf_thinking_enabled = !self.buf_thinking_enabled;
        }
    }

    /// 循环切换 effort（仅 Thinking 行）：medium → high → low → medium
    pub fn cycle_effort(&mut self, reverse: bool) {
        if self.cursor == ROW_THINKING {
            if reverse {
                self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                    "low" => "high".to_string(),
                    "high" => "medium".to_string(),
                    _ => "low".to_string(),
                };
            } else {
                self.buf_thinking_effort = match self.buf_thinking_effort.as_str() {
                    "low" => "medium".to_string(),
                    "medium" => "high".to_string(),
                    _ => "low".to_string(),
                };
            }
        }
    }

    /// 将面板状态写入 ZenConfig（alias + thinking）
    pub fn apply_to_config(&self, cfg: &mut ZenConfig) {
        cfg.config.active_alias = self.active_tab.to_key().to_string();
        // 只在用户主动开启时才写入 thinking 配置，否则保持 None（不传递任何 thinking 参数）
        if self.buf_thinking_enabled {
            let t = cfg.config.thinking.get_or_insert_with(|| ThinkingConfig {
                enabled: true,
                budget_tokens: self.buf_thinking_budget.parse().unwrap_or(8000),
                effort: self.buf_thinking_effort.clone(),
            });
            t.enabled = true;
            t.budget_tokens = self.buf_thinking_budget.parse().unwrap_or(8000);
            t.effort = self.buf_thinking_effort.clone();
        } else if cfg.config.thinking.is_some() {
            cfg.config.thinking.as_mut().unwrap().enabled = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AppConfig;
    use crate::config::ProviderConfig;

    fn make_config() -> ZenConfig {
        ZenConfig {
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
        assert_eq!(panel.cursor, ROW_OPUS);
        assert_eq!(panel.provider_name, "TestProvider");
        assert!(!panel.buf_thinking_enabled);
        assert_eq!(panel.buf_thinking_budget, "8000");
        assert_eq!(panel.buf_thinking_effort, "medium");
    }

    #[test]
    fn test_from_config_sonnet() {
        let mut cfg = make_config();
        cfg.config.active_alias = "sonnet".to_string();
        let panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.active_tab, AliasTab::Sonnet);
        assert_eq!(panel.cursor, ROW_SONNET);
    }

    #[test]
    fn test_move_cursor_wrap() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_SONNET);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_HAIKU);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_THINKING);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_LOGIN);
        panel.move_cursor(1);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.move_cursor(-1);
        assert_eq!(panel.cursor, ROW_LOGIN);
    }

    #[test]
    fn test_thinking_editing() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.cursor = ROW_THINKING;

        panel.toggle_thinking();
        assert!(panel.buf_thinking_enabled);

        panel.buf_thinking_budget.clear();
        panel.push_char('1');
        panel.push_char('0');
        panel.push_char('2');
        panel.push_char('4');
        assert_eq!(panel.buf_thinking_budget, "1024");

        panel.pop_char();
        assert_eq!(panel.buf_thinking_budget, "102");

        panel.paste_text("4abc56");
        assert_eq!(panel.buf_thinking_budget, "102456");
    }

    #[test]
    fn test_push_char_ignored_on_model_rows() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.push_char('5');
        assert_eq!(panel.buf_thinking_budget, "8000");
    }

    #[test]
    fn test_toggle_thinking_ignored_on_model_rows() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.toggle_thinking();
        assert!(!panel.buf_thinking_enabled);
    }

    #[test]
    fn test_apply_to_config() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.active_tab = AliasTab::Sonnet;
        panel.buf_thinking_enabled = true;
        panel.buf_thinking_budget = "16000".to_string();

        let mut cfg2 = make_config();
        panel.apply_to_config(&mut cfg2);
        assert_eq!(cfg2.config.active_alias, "sonnet");
        assert!(cfg2.config.thinking.as_ref().unwrap().enabled);
        assert_eq!(cfg2.config.thinking.as_ref().unwrap().budget_tokens, 16000);
    }

    #[test]
    fn test_apply_to_config_no_thinking_when_disabled() {
        let mut cfg = ZenConfig {
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
        // 未开启 thinking，不应创建配置
        assert!(cfg.config.thinking.is_none());
    }

    #[test]
    fn test_cycle_effort() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.cursor = ROW_THINKING;

        assert_eq!(panel.buf_thinking_effort, "medium");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "high");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "medium");

        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "low");
        panel.cycle_effort(true);
        assert_eq!(panel.buf_thinking_effort, "high");
    }

    #[test]
    fn test_cycle_effort_ignored_on_other_rows() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        assert_eq!(panel.cursor, ROW_OPUS);
        panel.cycle_effort(false);
        assert_eq!(panel.buf_thinking_effort, "medium");
    }

    #[test]
    fn test_apply_to_config_with_effort() {
        let cfg = make_config();
        let mut panel = ModelPanel::from_config(&cfg);
        panel.buf_thinking_enabled = true;
        panel.buf_thinking_effort = "high".to_string();

        let mut cfg2 = ZenConfig {
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
        panel.apply_to_config(&mut cfg2);
        let t = cfg2.config.thinking.as_ref().unwrap();
        assert!(t.enabled);
        assert_eq!(t.effort, "high");
    }
}
