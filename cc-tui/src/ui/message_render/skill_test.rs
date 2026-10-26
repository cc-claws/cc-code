use super::*;
use crate::{
    app::message_pipeline::MessagePipeline,
    ui::{message_render::render_view_model, message_view::MessageViewModel},
};
use cc_agent::messages::{BaseMessage, ContentBlock};

fn make_skill_result() -> String {
    serde_json::json!({
        "success": true,
        "commandName": "example-skill",
        "status": "inline",
        "allowedTools": ["Read", "Bash", "Skill"],
        "instruction": format!("# 技能原文\n{}", "内部工作流要求\n".repeat(2000))
    })
    .to_string()
}

fn make_skill_view(content: String, is_error: bool) -> MessageViewModel {
    let mut view = MessageViewModel::tool_block_with_id(
        "skill-call".to_string(),
        "Skill".to_string(),
        "Skill".to_string(),
        Some("example-skill".to_string()),
        is_error,
    );
    if let MessageViewModel::ToolBlock { content: body, .. } = &mut view {
        *body = content;
    }
    view.recompute_hash();
    view
}

fn make_rendered_text(lines: &[Line<'static>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_skill_completed_collapsed_renders_header_and_single_status_line() {
    let view = make_skill_view(make_skill_result(), false);
    let lines = render_view_model(&view, None, 120, false, 0);
    let text = make_rendered_text(&lines);
    assert_eq!(lines.len(), 2, "长篇技能正文默认只展示工具头和一行结果");
    assert!(
        text.contains("Skill(example-skill)"),
        "应显示技能名: {text}"
    );
    assert!(
        text.contains("Successfully loaded skill · 3 tools allowed"),
        "应显示成功状态及工具数量: {text}"
    );
    assert!(!text.contains("内部工作流要求"), "默认视图不应显示正文");
}

#[test]
fn test_skill_collapsed_unicode_arguments_fit_terminal_width() {
    let mut view = make_skill_view(make_skill_result(), false);
    if let MessageViewModel::ToolBlock { args_display, .. } = &mut view {
        *args_display = Some(format!("查询技能 {}", "中文参数".repeat(100)));
    }
    view.recompute_hash();
    for width in [0, 1, 7, 8, 9, 10, 12, 20, 40, 80] {
        let lines = render_view_model(&view, None, width, false, 0);
        assert_eq!(
            lines.len(),
            2,
            "窄屏技能卡片仍应只有工具头和一行结果: {width}"
        );
        assert!(
            lines.iter().all(|line| line.width() <= width),
            "中文参数不应超出终端列宽: width={width}, {:?}",
            make_rendered_text(&lines)
        );
    }
}

#[test]
fn test_skill_running_does_not_render_success_status() {
    let mut view = make_skill_view(make_skill_result(), false);
    if let MessageViewModel::ToolBlock { phase, .. } = &mut view {
        *phase = ToolPhase::Running;
    }
    let lines = render_view_model(&view, None, 100, false, 0);
    let text = make_rendered_text(&lines);
    assert_eq!(lines.len(), 2, "加载中的技能也仅展示工具头和一行结果");
    assert!(text.contains("Initializing…"), "应显示运行状态: {text}");
    assert!(!text.contains("Successfully"), "执行中不能提前报成功");
}

#[test]
fn test_skill_failed_renders_error_without_success_status() {
    let view = make_skill_view("Error: Skill not found: missing-skill".to_string(), true);
    let lines = render_view_model(&view, None, 100, false, 0);
    let text = make_rendered_text(&lines);
    assert!(text.contains("Skill failed"), "应明确显示失败: {text}");
    assert!(text.contains("not found"), "错误原因不能被折叠吞掉: {text}");
    assert!(!text.contains("Successfully"), "失败不能显示加载成功");
}

#[test]
fn test_skill_invalid_or_unsuccessful_result_does_not_claim_loaded() {
    for content in [
        "unexpected output".to_string(),
        serde_json::json!({"success": false, "instruction": "denied"}).to_string(),
    ] {
        let view = make_skill_view(content, false);
        let text = make_rendered_text(&render_view_model(&view, None, 100, false, 0));
        assert!(
            !text.contains("Successfully loaded"),
            "仅合法成功结果才可报加载成功: {text}"
        );
    }
}

#[test]
fn test_skill_detail_mode_and_explicit_expansion_show_markdown() {
    let mut view = make_skill_view(make_skill_result(), false);
    let detail = make_rendered_text(&render_view_model(&view, None, 100, true, 0));
    assert!(detail.contains("# 技能原文"), "详细模式应能审阅技能原文");
    assert!(detail.contains("内部工作流要求"), "详细模式应显示正文");
    assert!(
        !detail.contains("\"instruction\":"),
        "正文不应显示 JSON 外壳"
    );
    view.toggle_collapse();
    let expanded = make_rendered_text(&render_view_model(&view, None, 100, false, 0));
    assert!(expanded.contains("内部工作流要求"), "显式展开应能审阅正文");
}

#[test]
fn test_skill_history_projection_stays_separate_from_read_groups() {
    let messages = vec![
        BaseMessage::ai_from_blocks(vec![ContentBlock::ToolUse {
            id: "skill-call".to_string(),
            name: "Skill".to_string(),
            input: serde_json::json!({"skill": "example-skill"}),
        }]),
        BaseMessage::tool_result("skill-call", make_skill_result()),
    ];
    let views = MessagePipeline::messages_to_view_models(&messages, ".");
    assert_eq!(views.len(), 1, "历史恢复应保留独立技能卡片");
    assert!(
        matches!(&views[0], MessageViewModel::ToolBlock { tool_name, collapsed: true, .. } if tool_name == "Skill"),
        "技能不能被当成 Read 聚合后隐藏"
    );
    let lines = render_view_model(&views[0], None, 120, false, 0);
    assert_eq!(
        lines.len(),
        2,
        "历史恢复与实时卡片应使用相同的工具头和单行结果"
    );
}

#[test]
fn test_skill_forked_result_shows_done() {
    let result = serde_json::json!({
        "success": true,
        "commandName": "review",
        "status": "forked",
        "allowedTools": ["Read"],
        "model": "custom-model",
        "instruction": "子代理返回的摘要"
    })
    .to_string();
    let view = make_skill_view(result, false);
    let lines = render_view_model(&view, None, 100, false, 0);
    let text = make_rendered_text(&lines);
    assert_eq!(lines.len(), 2, "隔离执行也应只展示一行结果");
    assert!(text.contains("Done"), "隔离执行完成应对齐 CC Done: {text}");
    assert!(
        !text.contains("Successfully"),
        "隔离执行不走 inline 加载提示"
    );
    assert!(
        !text.contains("tool allowed"),
        "隔离执行不显示 inline 元信息"
    );
}

#[test]
fn test_skill_inline_result_shows_singular_tool_and_model() {
    let result = serde_json::json!({
        "success": true,
        "commandName": "review",
        "status": "inline",
        "allowedTools": ["Read"],
        "model": "custom-model",
        "instruction": "正文"
    })
    .to_string();
    let view = make_skill_view(result, false);
    let text = make_rendered_text(&render_view_model(&view, None, 120, false, 0));
    assert!(
        text.contains("Successfully loaded skill · 1 tool allowed · custom-model"),
        "工具数和模型应按 CC Byline 展示: {text}"
    );
}

#[test]
fn test_skill_inline_empty_allowed_tools_omits_count() {
    let result = serde_json::json!({
        "success": true,
        "commandName": "review",
        "status": "inline",
        "allowedTools": [],
        "instruction": "正文"
    })
    .to_string();
    let view = make_skill_view(result, false);
    let text = make_rendered_text(&render_view_model(&view, None, 120, false, 0));
    assert!(text.contains("Successfully loaded skill"));
    assert!(
        !text.contains("allowed"),
        "空白工具集合不应显示 0 tools allowed"
    );
}

#[test]
fn test_skill_slash_human_marker_projects_card_without_fake_tool_messages() {
    let mut output: serde_json::Value = serde_json::from_str(&make_skill_result()).unwrap();
    output["display"] = serde_json::json!(true);
    output["arguments"] = serde_json::json!("工单 #123");
    let marker = cc_agent::agent::compact::re_inject::skill_invocation_message(&output);
    let messages = vec![BaseMessage::human("/example-skill 工单 #123"), marker];
    let views = MessagePipeline::messages_to_view_models(&messages, ".");
    assert_eq!(views.len(), 2, "Slash 应保留用户输入并投影出技能加载卡片");
    assert!(matches!(&views[0], MessageViewModel::UserBubble { .. }));
    assert!(
        matches!(&views[1], MessageViewModel::ToolBlock { tool_name, args_display: Some(args), collapsed: true, .. } if tool_name == "Skill" && args == "example-skill"),
        "投影只显示技能名称，参数保留在原始消息中"
    );
    assert!(
        messages
            .iter()
            .all(|message| matches!(message, BaseMessage::Human { .. })),
        "界面投影不能向模型历史伪造 Assistant 或 Tool 消息"
    );
    assert_eq!(render_view_model(&views[1], None, 120, false, 0).len(), 2);
    let restored = MessagePipeline::messages_to_view_models(&messages, ".");
    assert_eq!(
        views[1].content_hash(),
        restored[1].content_hash(),
        "恢复时卡片标识与内容应保持稳定"
    );
}

#[test]
fn test_skill_compact_human_marker_is_hidden_without_display_flag() {
    let output: serde_json::Value = serde_json::from_str(&make_skill_result()).unwrap();
    let messages = vec![cc_agent::agent::compact::re_inject::skill_invocation_message(&output)];
    let views = MessagePipeline::messages_to_view_models(&messages, ".");
    assert!(
        views.is_empty(),
        "compact 恢复的技能正文不应重复渲染加载卡片"
    );
}

#[test]
fn test_skill_marker_partial_or_invalid_user_text_remains_visible() {
    let invalid = [
        "<skill-invocation>\nnot json\n</skill-invocation>".to_string(),
        "<skill-invocation>\n{}\n</skill-invocation>".to_string(),
        format!(
            "用户在讨论这个标记：{}",
            cc_agent::agent::compact::re_inject::format_skill_invocation(&serde_json::json!({
                "commandName": "review", "success": true, "status": "inline", "instruction": "正文"
            }))
        ),
    ];
    for content in invalid {
        let views = MessagePipeline::messages_to_view_models(&[BaseMessage::human(content)], ".");
        assert!(
            matches!(views.as_slice(), [MessageViewModel::UserBubble { .. }]),
            "不完整元消息不能吞掉普通用户输入"
        );
    }
}
