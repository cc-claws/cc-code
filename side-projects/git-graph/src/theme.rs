use peri_widgets::theme::{DarkTheme, Theme};
use ratatui::style::{Color, Modifier, Style};

pub struct GigTheme {
    base: DarkTheme,
}

#[allow(dead_code)]
impl GigTheme {
    pub fn new() -> Self {
        Self { base: DarkTheme }
    }

    // === 背景 ===
    pub fn graph_bg(&self) -> Color {
        Color::Rgb(20, 20, 25)
    }
    pub fn detail_bg(&self) -> Color {
        Color::Rgb(25, 25, 30)
    }
    pub fn selected_bg(&self) -> Color {
        Color::Rgb(38, 79, 120)
    }
    pub fn toolbar_bg(&self) -> Color {
        Color::Rgb(35, 35, 45)
    }
    pub fn sidebar_bg(&self) -> Color {
        Color::Rgb(22, 22, 28)
    }
    pub fn panel_bg(&self) -> Color {
        Color::Rgb(25, 25, 32)
    }

    // === 状态色 ===
    pub fn status_added(&self) -> Color {
        Color::Green
    }
    pub fn status_deleted(&self) -> Color {
        Color::Red
    }
    pub fn status_modified(&self) -> Color {
        Color::Yellow
    }

    // === 徽章工厂 ===
    /// 分支徽章：白字 + 彩色背景 + BOLD
    pub fn badge_branch(&self, bg: Color) -> Style {
        Style::default()
            .fg(Color::White)
            .bg(bg)
            .add_modifier(Modifier::BOLD)
    }

    /// 标签徽章：黑字 + 青色背景 + BOLD
    pub fn badge_tag(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// 文件状态徽章（A/M/D）
    pub fn badge_status(&self, color: Color) -> Style {
        Style::default()
            .fg(Color::White)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    }

    /// 按钮 [+]/[-] 样式：圆角底色
    pub fn badge_button_add(&self) -> Style {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    }

    pub fn badge_button_remove(&self, color: Color) -> Style {
        Style::default().fg(Color::White).bg(color)
    }

    // === 辅助 ===
    /// 选中行高亮：给所有 spans 加背景色
    pub fn highlight_line(&self, spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
        let bg = self.selected_bg();
        spans
            .into_iter()
            .map(|span| {
                let new_style = span.style.patch(Style::default().bg(bg));
                ratatui::text::Span::styled(span.content, new_style)
            })
            .collect()
    }
}

use ratatui::text::Span;

impl Theme for GigTheme {
    fn accent(&self) -> Color {
        self.base.accent()
    }
    fn success(&self) -> Color {
        self.base.success()
    }
    fn warning(&self) -> Color {
        self.base.warning()
    }
    fn error(&self) -> Color {
        self.base.error()
    }
    fn thinking(&self) -> Color {
        self.base.thinking()
    }
    fn text(&self) -> Color {
        self.base.text()
    }
    fn muted(&self) -> Color {
        self.base.muted()
    }
    fn dim(&self) -> Color {
        self.base.dim()
    }
    fn border(&self) -> Color {
        self.base.border()
    }
    fn border_active(&self) -> Color {
        self.base.border_active()
    }
    fn popup_bg(&self) -> Color {
        self.base.popup_bg()
    }
    fn cursor_bg(&self) -> Color {
        self.base.cursor_bg()
    }
    fn loading(&self) -> Color {
        self.base.loading()
    }
}
