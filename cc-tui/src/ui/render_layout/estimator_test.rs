use super::*;

#[test]
fn test_skill_collapsed_height_is_header_and_one_result_row() {
    let mut view = MessageViewModel::tool_block(
        "Skill".to_string(),
        "Skill".to_string(),
        Some("中文技能".repeat(100)),
        false,
    );
    if let MessageViewModel::ToolBlock { content, .. } = &mut view {
        *content = "技能正文\n".repeat(2000);
    }
    let summary = summarize(&view);
    for width in [20, 40, 80, 120] {
        assert_eq!(
            summary.height(width, false, false),
            2,
            "普通模式技能高度不能随原文长度或列宽膨胀"
        );
    }
    assert!(
        summary.height(80, true, false) > 2,
        "详细模式仍应估算完整正文高度"
    );
}
