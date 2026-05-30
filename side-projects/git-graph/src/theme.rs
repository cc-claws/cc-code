use peri_widgets::theme::{DarkTheme, Theme};
use ratatui::style::Color;

pub struct GigTheme {
    base: DarkTheme,
}

#[allow(dead_code)]
impl GigTheme {
    pub fn new() -> Self {
        Self { base: DarkTheme }
    }

    pub fn graph_bg(&self) -> Color {
        Color::Rgb(20, 20, 25)
    }
    pub fn detail_bg(&self) -> Color {
        Color::Rgb(25, 25, 30)
    }
    pub fn selected_bg(&self) -> Color {
        Color::Rgb(40, 40, 60)
    }
    pub fn toolbar_bg(&self) -> Color {
        Color::Rgb(35, 35, 45)
    }
    pub fn status_added(&self) -> Color {
        Color::Green
    }
    pub fn status_deleted(&self) -> Color {
        Color::Red
    }
    pub fn status_modified(&self) -> Color {
        Color::Yellow
    }
}

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
