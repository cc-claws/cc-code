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

/// 活跃思考行必须算 live：否则虚拟布局把 tick 固定成 0，走秒与相位闪烁会停住。
#[test]
fn test_is_live_covers_active_thinking_rows() {
    let reasoning = |active: bool| ContentBlockView::Reasoning {
        char_count: 2,
        duration_ms: Some(1000),
        action_summary: None,
        text: "思考".to_string(),
        active,
        active_since: active.then(std::time::Instant::now),
        hint: None,
    };

    let active_bubble = MessageViewModel::AssistantBubble {
        blocks: vec![reasoning(true)],
        is_streaming: true,
        collapsed: false,
        content_hash: 0,
    };
    assert!(
        is_live(&active_bubble),
        "活跃思考气泡必须算 live（走秒/闪烁依赖每帧 tick）"
    );

    let frozen_bubble = MessageViewModel::AssistantBubble {
        blocks: vec![reasoning(false)],
        is_streaming: false,
        collapsed: false,
        content_hash: 0,
    };
    assert!(
        !is_live(&frozen_bubble),
        "已冻结且不在留存窗口内的思考不再需要每帧刷新"
    );

    let group = MessageViewModel::ToolCallGroup {
        category: crate::ui::message_view::ToolCategory::Inspect,
        tools: Vec::new(),
        thinking: Some(crate::ui::message_view::GroupThinking {
            text: "思考".to_string(),
            duration_ms: Some(1000),
            action_summary: None,
            hint: None,
            active: false,
            active_since: None,
        }),
        collapsed: true,
        standalone_action: None,
        content_hash: 0,
    };
    assert!(
        is_live(&group),
        "留存窗口内的组思考行必须算 live（到期帧需要重算）"
    );

    let frozen_group = MessageViewModel::ToolCallGroup {
        category: crate::ui::message_view::ToolCategory::Inspect,
        tools: Vec::new(),
        thinking: None,
        collapsed: true,
        standalone_action: None,
        content_hash: 0,
    };
    assert!(!is_live(&frozen_group), "无思考行的冻结组不需要每帧刷新");
}
