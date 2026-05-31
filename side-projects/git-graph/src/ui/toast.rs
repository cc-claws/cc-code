use crate::app::{App, ToastStyle};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// 渲染底部 toast 栏（1 行）
pub fn draw_toast(f: &mut Frame, area: Rect, app: &mut App) {
    // 清除过期 toast
    if let Some(toast) = &app.toast {
        if std::time::Instant::now() >= toast.expires_at {
            app.toast = None;
        }
    }

    let toast = match &app.toast {
        Some(t) => t,
        None => return,
    };

    let (fg, bg) = match toast.style {
        ToastStyle::Success => (Color::Rgb(180, 255, 180), Color::Rgb(30, 60, 30)),
        ToastStyle::Error => (Color::Rgb(255, 180, 180), Color::Rgb(80, 20, 20)),
        ToastStyle::Info => (Color::Cyan, Color::Rgb(20, 40, 60)),
    };

    let text = toast
        .message
        .chars()
        .take(area.width as usize)
        .collect::<String>();
    let para = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {} ", text),
        Style::default().fg(fg).bg(bg),
    )]));
    f.render_widget(para, area);
}
