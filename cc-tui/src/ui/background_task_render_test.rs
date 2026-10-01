use super::*;

fn make_background_tool(content: &str) -> MessageViewModel {
    MessageViewModel::ToolBlock {
        tool_name: "Bash".into(),
        tool_call_id: "tc-background".into(),
        display_name: "Bash".into(),
        args_display: Some("powershell -File 'D:/temp/scan_bigfiles.ps1'".into()),
        content: content.into(),
        is_error: false,
        collapsed: false,
        color: theme::BASH_BORDER,
        diff_input: None,
        execution_timeout_ms: None,
        shell_backgrounded: false,
        started_at: None,
        content_hash: 0,
    }
}

const BACKGROUND_RESULT: &str = "<background-task-started><task-id>task-123</task-id><command>echo &lt;test&gt; [Exit code: 2]</command><output>C:\\temp\\scan.output</output></background-task-started>";

#[test]
fn test_background_task_render_collapsed_expanded_and_detail() {
    for collapsed in [false, true] {
        for detail in [false, true] {
            let mut vm = make_background_tool(BACKGROUND_RESULT);
            if let MessageViewModel::ToolBlock {
                collapsed: value, ..
            } = &mut vm
            {
                *value = collapsed;
            }
            let lines = render_view_model(&vm, None, 100, detail, 0);
            let text = rendered_text(&lines);
            assert!(
                text.contains("已转入后台（任务 task-123）"),
                "应展示后台移交结果：{text}"
            );
            assert!(
                !text.contains("<background-task-started>"),
                "不能泄露内部协议：{text}"
            );
            assert!(
                !text.contains(CONTROL_B_BACKGROUND_HINT),
                "已后台任务不能再次提示后台化"
            );
            assert_eq!(
                text.contains("C:\\temp\\scan.output"),
                detail,
                "路径仅在详情中展示"
            );
            assert_eq!(
                lines[0].spans[0].style.fg,
                Some(theme::CYAN),
                "不能误报命令成功或把 command 中的 exit 文本判为失败"
            );
            let MessageViewModel::ToolBlock { content, .. } = vm else {
                panic!("应保持工具类型")
            };
            assert_eq!(content, BACKGROUND_RESULT, "不能修改给模型和历史保存的原文");
        }
    }
}

#[test]
fn test_background_task_render_does_not_hide_normal_or_malformed_output() {
    for content in [
            "example: <background-task-started> is a tag",
            "<background-task-started><task-id>task-123</task-id>",
            "<background-task-started><task-id></task-id><command>x</command><output>x</output></background-task-started>",
            "<background-task-started><task-id>task-123</task-id><command>x</command></background-task-started>",
        ] {
            let vm = make_background_tool(content);
            let text = rendered_text(&render_view_model(&vm, None, 200, true, 0));
            assert!(text.contains(content), "普通/不完整输出应原样保留：{text}");
        }
    for (tool_name, is_error) in [("Read", false), ("Bash", true)] {
        let mut vm = make_background_tool(BACKGROUND_RESULT);
        if let MessageViewModel::ToolBlock {
            tool_name: name,
            is_error: error,
            ..
        } = &mut vm
        {
            *name = tool_name.into();
            *error = is_error;
        }
        let text = rendered_text(&render_view_model(&vm, None, 400, false, 0));
        assert!(
            text.contains("<background-task-started>"),
            "不能过滤其他工具或错误结果：{text}"
        );
    }
}

#[test]
fn test_background_task_render_history_and_nested_subagent() {
    let message =
        cc_agent::messages::BaseMessage::tool_result("tc-background", BACKGROUND_RESULT);
    let calls = vec![(
        "tc-background".to_string(),
        "Bash".to_string(),
        serde_json::json!({"command": "sleep 5"}),
    )];
    let history = MessageViewModel::from_base_message(&message, &calls);
    let mut group = MessageViewModel::subagent_group("agent-1".into(), "测试任务".into());
    if let MessageViewModel::SubAgentGroup {
        recent_messages,
        collapsed,
        ..
    } = &mut group
    {
        recent_messages.push(history.clone());
        *collapsed = false;
    }
    for vm in [&history, &group] {
        let text = rendered_text(&render_view_model(vm, None, 100, true, 0));
        assert!(
            text.contains("已转入后台"),
            "恢复/子 Agent 必须走相同投影：{text}"
        );
        assert!(
            !text.contains("<background-task-started>"),
            "恢复/子 Agent 不能泄露协议：{text}"
        );
    }
}

#[test]
fn test_background_task_render_respects_display_width() {
    let vm = make_background_tool(BACKGROUND_RESULT);
    let lines = render_view_model(&vm, None, 28, true, 0);
    for line in lines.iter().skip(1) {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            UnicodeWidthStr::width(text.as_str()) <= 28,
            "中文及路径不能超列宽：{text}"
        );
    }
}

fn rendered_text(lines: &[Line<'static>]) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("")
}
