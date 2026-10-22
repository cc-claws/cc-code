use super::Theme;
use ratatui::style::Color;

/// 项目默认深色主题
///
/// 色值与 cc-tui/src/ui/theme.rs 的常量一一对应。
/// 业务特有常量（TOOL_NAME=SAGE, SUB_AGENT=SAGE, MODEL_INFO=#A0825F）
/// 保留在 TUI 层，不在此处定义。
#[derive(Debug, Clone)]
pub struct DarkTheme;

impl Theme for DarkTheme {
    fn accent(&self) -> Color {
        Color::Rgb(215, 119, 87)
    } // ACCENT #D77757
    fn success(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn warning(&self) -> Color {
        Color::Rgb(255, 193, 7)
    } // WARNING #FFC107
    fn error(&self) -> Color {
        Color::Rgb(255, 107, 128)
    } // ERROR #FF6B80
    fn thinking(&self) -> Color {
        Color::Rgb(162, 169, 228)
    } // THINKING #A2A9E4（与 TUI theme::THINKING 一致）
    fn permission(&self) -> Color {
        Color::Rgb(177, 185, 249)
    } // PERMISSION #B1B9F9（对齐 CC dark `permission`）
    fn text(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn muted(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn dim(&self) -> Color {
        Color::Rgb(80, 80, 80)
    } // DIM #505050
    fn border(&self) -> Color {
        Color::Rgb(80, 80, 80)
    } // BORDER #505050
    fn border_active(&self) -> Color {
        Color::Rgb(215, 119, 87)
    } // = accent #D77757
    fn popup_bg(&self) -> Color {
        Color::Rgb(0, 0, 0)
    } // POPUP_BG #000000
    fn cursor_bg(&self) -> Color {
        Color::Rgb(38, 38, 38)
    } // CURSOR_BG #262626
    fn loading(&self) -> Color {
        Color::Rgb(147, 165, 255)
    } // LOADING #93A5FF

    fn user_bg(&self) -> Color {
        Color::Rgb(55, 55, 55)
    } // USER_BG #373737

    fn bash_border(&self) -> Color {
        Color::Rgb(253, 93, 177)
    } // BASH_BORDER #FD5DB1
}

/// 由调用方注入色值的主题：cc-tui 按当前调色板构造，使 widgets 渲染
/// （markdown / diff / 消息块）随 /theme 切换同步变色。
#[derive(Debug, Clone, Copy)]
pub struct CustomTheme {
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub thinking: Color,
    /// CC 主题的 `permission` 色（Markdown 行内代码用）
    pub permission: Color,
    pub text: Color,
    pub muted: Color,
    pub dim: Color,
    pub border: Color,
    pub border_active: Color,
    pub popup_bg: Color,
    pub cursor_bg: Color,
    pub loading: Color,
    pub user_bg: Color,
    pub bash_border: Color,
    pub diff_add: Color,
    pub diff_add_decoration: Color,
    pub diff_add_bg: Color,
    pub diff_add_word_bg: Color,
    pub diff_remove: Color,
    pub diff_remove_decoration: Color,
    pub diff_remove_bg: Color,
    pub diff_remove_word_bg: Color,
    pub diff_hunk: Color,
    pub syntax_theme_name: &'static str,
}

impl Theme for CustomTheme {
    fn accent(&self) -> Color {
        self.accent
    }
    fn success(&self) -> Color {
        self.success
    }
    fn warning(&self) -> Color {
        self.warning
    }
    fn error(&self) -> Color {
        self.error
    }
    fn thinking(&self) -> Color {
        self.thinking
    }
    fn permission(&self) -> Color {
        self.permission
    }
    fn text(&self) -> Color {
        self.text
    }
    fn muted(&self) -> Color {
        self.muted
    }
    fn dim(&self) -> Color {
        self.dim
    }
    fn border(&self) -> Color {
        self.border
    }
    fn border_active(&self) -> Color {
        self.border_active
    }
    fn popup_bg(&self) -> Color {
        self.popup_bg
    }
    fn cursor_bg(&self) -> Color {
        self.cursor_bg
    }
    fn loading(&self) -> Color {
        self.loading
    }
    fn user_bg(&self) -> Color {
        self.user_bg
    }
    fn bash_border(&self) -> Color {
        self.bash_border
    }
    fn diff_add(&self) -> Color {
        self.diff_add
    }
    fn diff_add_decoration(&self) -> Color {
        self.diff_add_decoration
    }
    fn diff_remove_decoration(&self) -> Color {
        self.diff_remove_decoration
    }
    fn diff_add_bg(&self) -> Color {
        self.diff_add_bg
    }
    fn diff_add_word_bg(&self) -> Color {
        self.diff_add_word_bg
    }
    fn diff_remove(&self) -> Color {
        self.diff_remove
    }
    fn diff_remove_bg(&self) -> Color {
        self.diff_remove_bg
    }
    fn diff_remove_word_bg(&self) -> Color {
        self.diff_remove_word_bg
    }
    fn diff_hunk(&self) -> Color {
        self.diff_hunk
    }
    fn syntax_theme_name(&self) -> &'static str {
        self.syntax_theme_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("presets_test.rs");
}
