use super::*;
use crate::ui::theme;

#[test]
fn test_drawer_columns_align_with_chinese_and_long_titles() {
    let first = drawer_line(100, true, false, "general-purpose", "正在等待六十秒后查询武汉天气", "32s · ↓ 49 tokens", None);
    let second = drawer_line(100, false, true, "explore", &"长任务名".repeat(100), "32s · ↓ 33 tokens", None);
    let first = first.to_string();
    let second = second.to_string();
    assert_eq!(UnicodeWidthStr::width(first.as_str()), 100, "每行必须占相同显示列宽");
    assert_eq!(UnicodeWidthStr::width(second.as_str()), 100);
    assert!(first.ends_with("32s · ↓ 49 tokens "));
    assert!(second.ends_with("32s · ↓ 33 tokens "));
    assert!(second.contains('…'), "长中文标题必须单行省略");
    assert!(first.starts_with("> ◯ "));
    assert!(second.starts_with("  ● "), "选中游标和当前视口必须独立");
}

#[test]
fn test_drawer_tiny_width_never_overflows() {
    for width in 0..40 {
        let line = drawer_line(width, true, true, "general-purpose", "中文说明", "1h · ↓ 12345 tokens", None).to_string();
        assert!(UnicodeWidthStr::width(line.as_str()) <= width, "极窄终端不能越界");
        assert!(!line.contains('\n'));
    }
}

/// 配色回归：抽屉必须使用参考原型（docs/subagent-mock.html）的白灰配色，
/// 而不是整行按状态染色（Running 整行青 / Completed 整行绿）。
#[test]
fn test_drawer_uses_reference_palette_not_whole_row_status_color() {
    let line = drawer_line(80, false, false, "general-purpose", "查询武汉天气", "32s · ↓ 49 tokens", None);
    let colors: Vec<Color> = line.spans.iter().filter_map(|span| span.style.fg).collect();

    assert!(
        colors.contains(&theme::DRAWER_TEXT),
        "类型名应为参考图柔白 {colors:?}"
    );
    assert!(
        colors.contains(&theme::DRAWER_PREVIEW),
        "任务预览应为参考图浅冷灰 {colors:?}"
    );
    assert!(
        colors.contains(&theme::DRAWER_MUTED),
        "右侧指标/空心态应为参考图冷灰 {colors:?}"
    );
    assert!(
        !colors.contains(&Color::Cyan) && !colors.contains(&Color::Green),
        "正常行不得整行青/绿染色 {colors:?}"
    );
}

/// 示警色只落在圆点与右侧指标上，正文仍保持白灰。
#[test]
fn test_drawer_accent_only_tints_circle_and_metrics() {
    let line = drawer_line(80, false, false, "general-purpose", "查询武汉天气", "✗ failed", Some(theme::ERROR));
    let colors: Vec<Color> = line.spans.iter().filter_map(|span| span.style.fg).collect();

    assert!(colors.contains(&theme::ERROR), "失败行应有红色示警 {colors:?}");
    assert!(
        colors.contains(&theme::DRAWER_TEXT) && colors.contains(&theme::DRAWER_PREVIEW),
        "示警不应把正文整体染红 {colors:?}"
    );
}

/// 键盘游标行使用参考图底色 #1E2436，而非终端反色。
#[test]
fn test_drawer_selected_row_uses_reference_background() {
    let line = drawer_line(80, true, false, "general-purpose", "查询武汉天气", "32s · ↓ 49 tokens", None);
    let has_bg = line
        .spans
        .iter()
        .any(|span| span.style.bg == Some(theme::DRAWER_SELECTED_BG));
    assert!(has_bg, "选中行应使用参考图底色");
    let reversed = line
        .spans
        .iter()
        .any(|span| span.style.add_modifier.contains(Modifier::REVERSED));
    assert!(!reversed, "选中行不应再使用终端反色");
}
