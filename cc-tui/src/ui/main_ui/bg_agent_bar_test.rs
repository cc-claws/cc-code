use super::*;
use crate::ui::theme;

#[test]
fn test_drawer_columns_align_with_chinese_and_long_titles() {
    let first = drawer_line(100, true, false, true, "general-purpose", "正在等待六十秒后查询武汉天气", "32s · ↓ 49 tokens", None);
    let second = drawer_line(100, false, true, false, "explore", &"长任务名".repeat(100), "32s · ↓ 33 tokens", None);
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
        let line = drawer_line(width, true, true, true, "general-purpose", "中文说明", "1h · ↓ 12345 tokens", None).to_string();
        assert!(UnicodeWidthStr::width(line.as_str()) <= width, "极窄终端不能越界");
        assert!(!line.contains('\n'));
    }
}

/// 配色回归：抽屉必须使用参考原型（docs/subagent-mock.html）的白灰配色，
/// 而不是整行按状态染色（Running 整行青 / Completed 整行绿）。
#[test]
fn test_drawer_uses_reference_palette_not_whole_row_status_color() {
    let line = drawer_line(80, false, false, false, "general-purpose", "查询武汉天气", "32s · ↓ 49 tokens", None);
    let colors: Vec<Color> = line.spans.iter().filter_map(|span| span.style.fg).collect();

    assert!(
        colors.iter().filter(|c| **c == theme::DRAWER_MUTED).count() >= 3,
        "未选中行的类型名/预览/指标应统一为灰 {colors:?}"
    );
    assert!(
        !colors.contains(&theme::DRAWER_TEXT_ACTIVE),
        "未选中行不应使用亮白 {colors:?}"
    );
    assert!(
        !colors.contains(&Color::Cyan) && !colors.contains(&Color::Green),
        "正常行不得整行青/绿染色 {colors:?}"
    );
}

/// 示警色只落在圆点与右侧指标上，正文仍保持白灰。
#[test]
fn test_drawer_accent_only_tints_circle_and_metrics() {
    let line = drawer_line(80, false, false, false, "general-purpose", "查询武汉天气", "✗ failed", Some(theme::ERROR));
    let colors: Vec<Color> = line.spans.iter().filter_map(|span| span.style.fg).collect();

    assert!(colors.contains(&theme::ERROR), "失败行应有红色示警 {colors:?}");
    assert!(
        colors.contains(&theme::DRAWER_MUTED),
        "示警不应把正文整体染红，正文应保持灰 {colors:?}"
    );
}

/// 配色收敛：抽屉只有「亮白（游标行）+ 灰（其余）」两档。
///
/// 真机反馈：不要青色，也不要一堆中间灰阶（"你搞那么多颜色干嘛"）。
#[test]
fn test_drawer_palette_is_bright_white_or_muted_only() {
    let selected = drawer_line(80, true, true, true, "general-purpose", "查询武汉天气", "32s · ↓ 49", None);
    let colors: Vec<Color> = selected.spans.iter().filter_map(|span| span.style.fg).collect();
    assert!(
        colors.contains(&theme::DRAWER_TEXT_ACTIVE),
        "游标行正文应亮白 {colors:?}"
    );
    assert!(
        !colors.contains(&Color::Cyan) && !colors.contains(&theme::DRAWER_MUTED),
        "游标行不得混青色或灰阶 {colors:?}"
    );

    let plain = drawer_line(80, false, false, false, "general-purpose", "查询武汉天气", "32s · ↓ 49", None);
    let plain_colors: Vec<Color> = plain.spans.iter().filter_map(|span| span.style.fg).collect();
    assert!(
        !plain_colors.contains(&theme::DRAWER_TEXT_ACTIVE) && !plain_colors.contains(&Color::Cyan),
        "非游标行只能灰，不得亮白或青色 {plain_colors:?}"
    );
    assert!(
        plain_colors.iter().all(|color| *color == theme::DRAWER_MUTED || *color == Color::Reset),
        "非游标行只允许一种灰 {plain_colors:?}"
    );
}

/// 键盘游标行只用**前景色**区分，不得出现整行底色。
///
/// 真机反馈：按 ↑/↓ 时选中行有背景色，会让人以为「已经进这个视口了」——
/// Enter 才是进入；游标移动只能靠字体变亮白表达。也禁止终端反色。
#[test]
fn test_drawer_cursor_row_uses_foreground_color_only() {
    let line = drawer_line(80, true, false, true, "general-purpose", "查询武汉天气", "32s · ↓ 49", None);
    let has_bg = line
        .spans
        .iter()
        .any(|span| span.style.bg.is_some());
    assert!(!has_bg, "游标行不得设置任何底色");
    let reversed = line
        .spans
        .iter()
        .any(|span| span.style.add_modifier.contains(Modifier::REVERSED));
    assert!(!reversed, "游标行不得使用终端反色");
    assert!(
        line.spans
            .iter()
            .any(|span| span.style.fg == Some(theme::DRAWER_TEXT_ACTIVE)),
        "游标行必须靠亮白前景色表达"
    );
}
