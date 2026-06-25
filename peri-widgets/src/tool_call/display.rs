use super::ToolCallStatus;
use ratatui::style::Color;

/// 返回 (指示器字符, 颜色)
///
/// - Pending: 灰色 ●
/// - Running: 灰色 ● 闪烁
/// - Completed: 绿色 ●
/// - Failed: 红色 ●
pub fn format_indicator(status: ToolCallStatus, tick: u64) -> (&'static str, Color) {
    match status {
        ToolCallStatus::Pending => ("●", Color::Rgb(153, 153, 153)), // #999999 MUTED
        ToolCallStatus::Running => {
            let visible = (tick / 4).is_multiple_of(2);
            let ch = if visible { "●" } else { " " };
            (ch, Color::Rgb(153, 153, 153)) // #999999 MUTED
        }
        ToolCallStatus::Completed => ("●", Color::Rgb(78, 186, 101)), // #4EBA65 SAGE
        ToolCallStatus::Failed => ("●", Color::Rgb(255, 107, 128)),   // #FF6B80 ERROR
    }
}

pub fn format_args_summary(args: &str, max_width: usize) -> String {
    if args.len() <= max_width {
        args.to_string()
    } else {
        let mut truncated: String = args.chars().take(max_width.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

#[cfg(test)]
#[path = "display_test.rs"]
mod tests;
