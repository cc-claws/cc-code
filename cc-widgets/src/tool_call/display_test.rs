use super::*;
use ratatui::style::Color;

const PENDING: Color = Color::Rgb(153, 153, 153);
const DONE: Color = Color::Rgb(78, 186, 101);
const FAILED: Color = Color::Rgb(255, 107, 128);

#[test]
fn test_indicator_running_blinks() {
    let (ch, color) = format_indicator(ToolCallStatus::Running, 0, PENDING, DONE, FAILED);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(153, 153, 153));
    let (ch, _) = format_indicator(ToolCallStatus::Running, 4, PENDING, DONE, FAILED);
    assert_eq!(ch, " ");
}

#[test]
fn test_indicator_pending_stays_solid() {
    // Queued / AwaitingApproval 恒亮不闪（对齐 CC：排队不是「活跃」，
    // 等待语义由 `Waiting…` 文案承载）。逐 tick 验证始终为实心圆。
    for tick in [0, 4, 8, 12, 100, 101] {
        let (ch, color) = format_indicator(ToolCallStatus::Pending, tick, PENDING, DONE, FAILED);
        assert_eq!(ch, "●", "Pending 在 tick={tick} 时不应熄灭");
        assert_eq!(color, Color::Rgb(153, 153, 153));
    }
}

#[test]
fn test_indicator_completed() {
    let (ch, color) = format_indicator(ToolCallStatus::Completed, 0, PENDING, DONE, FAILED);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(78, 186, 101));
}

#[test]
fn test_indicator_failed() {
    let (ch, color) = format_indicator(ToolCallStatus::Failed, 0, PENDING, DONE, FAILED);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(255, 107, 128));
}

#[test]
fn test_format_args_summary_short() {
    assert_eq!(format_args_summary("hello", 40), "hello");
}

#[test]
fn test_format_args_summary_truncated() {
    let long = "a".repeat(50);
    let result = format_args_summary(&long, 10);
    assert_eq!(result.chars().count(), 10);
    assert!(result.ends_with('…'));
}

#[test]
fn test_format_args_summary_exact_width() {
    let s = "1234567890";
    assert_eq!(format_args_summary(s, 10), "1234567890");
}

#[test]
fn test_format_args_summary_empty() {
    assert_eq!(format_args_summary("", 10), "");
}
