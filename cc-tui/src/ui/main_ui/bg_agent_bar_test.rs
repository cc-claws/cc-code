use super::*;

#[test]
fn test_drawer_columns_align_with_chinese_and_long_titles() {
    let first = drawer_line(100, true, false, "general-purpose", "正在等待六十秒后查询武汉天气", "32s · ↓ 49 tokens", Color::Cyan);
    let second = drawer_line(100, false, true, "explore", &"长任务名".repeat(100), "32s · ↓ 33 tokens", Color::Cyan);
    let first = first.to_string();
    let second = second.to_string();
    assert_eq!(UnicodeWidthStr::width(first.as_str()), 100, "每行必须占相同显示列宽");
    assert_eq!(UnicodeWidthStr::width(second.as_str()), 100);
    assert!(first.ends_with("32s · ↓ 49 tokens "));
    assert!(second.ends_with("32s · ↓ 33 tokens "));
    assert!(second.contains('…'), "长中文标题必须单行省略");
    assert!(first.starts_with("> ○ "));
    assert!(second.starts_with("  • "), "选中游标和当前视口必须独立");
}

#[test]
fn test_drawer_tiny_width_never_overflows() {
    for width in 0..40 {
        let line = drawer_line(width, true, true, "general-purpose", "中文说明", "1h · ↓ 12345 tokens", Color::Cyan).to_string();
        assert!(UnicodeWidthStr::width(line.as_str()) <= width, "极窄终端不能越界");
        assert!(!line.contains('\n'));
    }
}
