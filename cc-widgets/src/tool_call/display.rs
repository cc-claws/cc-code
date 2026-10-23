use super::ToolCallStatus;
use ratatui::style::Color;

/// 返回 (指示器字符, 颜色)
///
/// - Pending: 灰色 ● 恒亮（排队 / 等待审批，对齐 CC `isQueued` 分支）
/// - Running: 灰色 ● 闪烁（执行中）
/// - Completed: 绿色 ●
/// - Failed: 红色 ●
///
/// 节奏：`tick` 由渲染线程按 200ms 推进，闪烁周期 800ms 亮 / 800ms 灭
/// （`tick / 4` 每 4 个 tick 即 800ms 翻转一次）。
pub fn format_indicator(
    status: ToolCallStatus,
    tick: u64,
    pending: Color,
    completed: Color,
    failed: Color,
) -> (&'static str, Color) {
    match status {
        // Queued / AwaitingApproval 恒亮：排队不是「活跃」，不该伪装成执行中。
        // 等待语义由同屏的 `Waiting…` / `Waiting for permission…` 文案承载
        // （对齐 CC `AssistantToolUseMessage`：isQueued 走独立分支，不进 ToolUseLoader）。
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
