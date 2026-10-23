use super::ToolCallStatus;
use ratatui::style::Color;

/// 返回 (指示器字符, 颜色)
///
/// - Pending: 灰色 ● 闪烁（排队 / 等待审批中，提示用户系统存活而非卡死）
/// - Running: 灰色 ● 闪烁（执行中）
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
        // Pending（Queued / AwaitingApproval）与 Running 共用闪烁动画：
        // 消除「看看见 ⎿ Waiting… 但指示器不闪、误以为进程卡死」的混淆。
        ToolCallStatus::Pending | ToolCallStatus::Running => {
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
