use super::*;

#[test]
fn test_desired_height_adaptive_with_few_rows() {
    let mut panel = CommandPalettePanel::from_config(&crate::config::PeriConfig::default());
    panel.rows.clear();
    for index in 0..5 {
        panel.rows.push(ListRow::Header(format!("h{index}")));
    }
    assert_eq!(panel.desired_height(80, 120), 10, "5 行内容 + 5 行 chrome = 10");
}

#[test]
fn test_desired_height_caps_at_panel_height() {
    let mut panel = CommandPalettePanel::from_config(&crate::config::PeriConfig::default());
    panel.rows.clear();
    for index in 0..100 {
        panel.rows.push(ListRow::Header(format!("h{index}")));
    }
    assert_eq!(
        panel.desired_height(80, 120),
        PANEL_HEIGHT,
        "内容超出上限时应封顶为 PANEL_HEIGHT"
    );
}
