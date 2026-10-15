use super::ToolCallStatus;
use ratatui::style::Color;

/// 返回 (指示器字符, 颜色)
///
/// - Pending: 灰色 ●
/// - Running: 灰色 ● 闪烁
/// - Completed: 绿色 ●
/// - Failed: 红色 ●
pub fn format_indicator(
    status: ToolCallStatus,
    tick: u64,
    pending: Color,
    completed: Color,
    failed: Color,
) -> (&'static str, Color) {
    match status {
        ToolCallStatus::Pending => ("●", pending),
        ToolCallStatus::Running => {
            let visible = (tick / 4).is_multiple_of(2);
            let ch = if visible { "●" } else { " " };
            (ch, pending)
        }
        ToolCallStatus::Completed => ("●", completed),
        ToolCallStatus::Failed => ("●", failed),
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
