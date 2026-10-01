use super::*;
use ratatui::style::Color;

#[test]
fn test_indicator_running_blinks() {
    let (ch, color) = format_indicator(ToolCallStatus::Running, 0);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(153, 153, 153));
    let (ch, _) = format_indicator(ToolCallStatus::Running, 4);
    assert_eq!(ch, " ");
}

#[test]
fn test_indicator_pending() {
    let (ch, color) = format_indicator(ToolCallStatus::Pending, 0);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(153, 153, 153));
}

#[test]
fn test_indicator_completed() {
    let (ch, color) = format_indicator(ToolCallStatus::Completed, 0);
    assert_eq!(ch, "●");
    assert_eq!(color, Color::Rgb(78, 186, 101));
}

#[test]
fn test_indicator_failed() {
    let (ch, color) = format_indicator(ToolCallStatus::Failed, 0);
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
