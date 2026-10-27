use super::*;
use crate::app::message_pipeline::MessagePipeline;
use crate::ui::message_view::ToolPhase;
use cc_agent::messages::{BaseMessage, ContentBlock};
use cc_middlewares::middleware::format_command_output;
use serde_json::json;

fn make_bash_inspect_views(command: &str, output: &str) -> (MessageViewModel, MessageViewModel) {
    let mut standalone = MessageViewModel::tool_block_with_id(
        "inspect".into(),
        "Bash".into(),
        "Bash".into(),
        Some(command.into()),
        false,
    );
    if let MessageViewModel::ToolBlock { content, phase, .. } = &mut standalone {
        *content = output.into();
        *phase = ToolPhase::Finished;
    }
    let group = MessageViewModel::ToolCallGroup {
        thinking: None,
        category: ToolCategory::Inspect,
        tools: vec![ToolEntry {
            tool_name: "Bash".into(),
            display_name: "Bash".into(),
            args_display: Some(command.into()),
            content: output.into(),
            is_error: false,
            file_inspect: true,
        }],
        collapsed: true,
        content_hash: 0,
        standalone_action: Some("ran 1 shell command".into()),
    };
    (standalone, group)
}

fn make_rendered_text(views: &[MessageViewModel], detail: bool) -> String {
    views
        .iter()
        .enumerate()
        .flat_map(|(index, view)| render_view_model(view, Some(index), 200, detail, 0))
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn make_inspection_messages() -> (Vec<BaseMessage>, String) {
    let read_path = std::env::temp_dir()
        .join("inspect-fixture.txt")
        .to_string_lossy()
        .replace('\\', "/");
    let calls = [
        (
            "list",
            "Bash",
            json!({"command": "ls src", "file_inspect": true}),
        ),
        (
            "head",
            "Bash",
            json!({"command": "head -n 1 Cargo.toml", "file_inspect": true}),
        ),
        (
            "grep",
            "Bash",
            json!({"command": "grep needle missing.txt", "file_inspect": true}),
        ),
        ("read", "Read", json!({"file_path": read_path})),
        ("build", "Bash", json!({"command": "cargo check"})),
    ];
    let mut blocks = vec![ContentBlock::reasoning_with_duration(
        "先检查文件。",
        19_000,
    )];
    blocks.extend(
        calls
            .into_iter()
            .map(|(id, name, input)| ContentBlock::ToolUse {
                id: id.into(),
                name: name.into(),
                input,
            }),
    );
    let messages = vec![
        BaseMessage::ai_from_blocks(blocks),
        BaseMessage::tool_result("list", format_command_output("lib.rs\nmain.rs", "", 0)),
        BaseMessage::tool_result("head", format_command_output("[package]", "", 0)),
        // Bash 非零退出仍通过成功 ToolResult 返回，不能用 tool_error 掩盖渲染漏报。
        BaseMessage::tool_result(
            "grep",
            format_command_output("", "grep: missing.txt: No such file or directory", 2),
        ),
        BaseMessage::tool_result("read", "Read 2 lines"),
        BaseMessage::tool_result("build", "Finished dev profile"),
    ];
    (messages, read_path)
}

fn make_restored_views(messages: &[BaseMessage]) -> Vec<MessageViewModel> {
    let cwd = std::env::current_dir().expect("测试工作目录应可读");
    MessagePipeline::messages_to_view_models(messages, &cwd.to_string_lossy())
}

#[test]
fn test_bash_inspect_group_detail_matches_standalone_output_and_status() {
    for output in [
        format_command_output("", "grep: missing.txt: No such file or directory", 2),
        format_command_output("first\nsecond", "", 0),
        format_command_output("", "", 0),
        "\n[Exit code: 0]".into(),
        " \r\n\t ".into(),
        String::new(),
        "{\"items\":[1,2],\"ok\":true}".into(),
    ] {
        let (standalone, group) = make_bash_inspect_views("grep needle file.txt", &output);
        let expected = render_view_model(&standalone, None, 80, true, 0);
        let actual = render_view_model(&group, None, 80, true, 0);
        assert_eq!(
            actual, expected,
            "组内 Bash 的命令、输出、状态色与单条必须一致: {output:?}"
        );
        if parse_exit_code(&output).is_some_and(|code| code != 0) {
            assert_eq!(
                actual[0].spans[0].style.fg,
                Some(theme::cur().error),
                "非零退出即使 is_error=false 也必须显示失败状态"
            );
            assert!(
                actual
                    .iter()
                    .skip(1)
                    .flat_map(|line| &line.spans)
                    .all(|span| span.style.fg == Some(theme::cur().error)),
                "失败输出与分支前缀都必须使用错误色"
            );
        }
    }
}

#[test]
fn test_bash_inspect_group_detail_truncates_long_header_to_display_width() {
    for command in [
        format!(
            "grep -n needle {}/tail-marker.txt",
            "very/long/path/".repeat(18)
        ),
        format!(
            "grep -n needle {}/tail-marker.txt",
            "很长的目录/".repeat(18)
        ),
    ] {
        let (_, group) = make_bash_inspect_views(&command, "found");
        for width in [20, 40, 60, 80, 120] {
            let actual = render_view_model(&group, None, width, true, 0);
            let header_rows = actual
                .iter()
                .take_while(|line| !BranchPrefix::is_branch_line(line))
                .count();
            assert_eq!(header_rows, 1, "详细态长命令头必须保持单行，不能折行");
            let header = actual[0].to_string();
            assert!(
                header.starts_with("● Bash("),
                "截断后必须保留工具名称: {header}"
            );
            assert!(
                header.ends_with("…)"),
                "超宽命令必须以省略号和右括号闭合: {header}"
            );
            assert!(
                actual[0].width() <= width,
                "ASCII 和中文都必须按显示列宽截断: width={width}, {header}"
            );
            assert!(!header.contains("tail-marker.txt"), "超宽命令尾部应省略");
            let shown_args = header
                .strip_prefix("● Bash(")
                .and_then(|text| text.strip_suffix("…)"))
                .expect("命令头应具有完整的括号与省略号");
            assert!(
                command.starts_with(shown_args),
                "截断应保留原始命令前缀，不能破坏中文字符"
            );
            assert_eq!(actual.len(), 2, "单行命令头后应紧接一行输出");
            assert_eq!(
                actual[1].to_string(),
                "  ⎿  found",
                "命令头截断不得影响工具输出"
            );
        }
    }
}

#[test]
fn test_bash_inspect_group_collapsed_promotes_exit_code_before_summary_limit() {
    let output = format_command_output(&"context\n".repeat(100), "missing file", 2);
    let (_, group) = make_bash_inspect_views("grep needle missing.txt", &output);
    let actual = make_rendered_text(&[group], false);
    assert!(
        actual.starts_with("  Ran 1 shell command (ctrl+o to expand)\n  ⎿  Error: Exit code 2\n"),
        "退出码必须先提升为错误头，不能被摘要长度上限吞掉: {actual}"
    );
    assert!(
        actual.contains("(ctrl+o to expand)"),
        "截断的错误摘要应提示展开"
    );
    assert!(
        !actual.contains("[Exit code: 2]"),
        "退出码标记应转换成错误头"
    );
    assert!(!actual.contains("● Bash("), "折叠态不得展开命令头");
}

#[test]
fn test_render_bash_inspect_group_collapsed_snapshot() {
    let (messages, _) = make_inspection_messages();
    let views = make_restored_views(&messages);
    assert!(views.iter().any(|view| matches!(view, MessageViewModel::ToolCallGroup { tools, .. } if tools.len() == 4 && tools.iter().all(|tool| !tool.is_error))), "应经过生产 pipeline 聚合，保持真实工具结果的 is_error=false");
    let actual = make_rendered_text(&views, false);
    assert_eq!(
        actual,
        concat!(
            "  Thought for 19s, read 1 file, ran 3 shell commands (ctrl+o to expand)\n",
            "  ⎿  Error: Exit code 2\n",
            "     [stderr]\n",
            "     grep: missing.txt: No such file or directory\n",
            "● Bash(cargo check)\n",
            "  ⎿  Finished dev profile"
        ),
        "折叠态应隐藏文件查询明细、保留错误，并保持普通 Bash 展示"
    );
}

#[test]
fn test_render_bash_inspect_group_expanded_snapshot() {
    let (messages, read_path) = make_inspection_messages();
    let views = make_restored_views(&messages);
    let actual = make_rendered_text(&views, true);
    assert_eq!(
        actual,
        format!(
            concat!(
                "  Thought for 19s, read 1 file, ran 3 shell commands\n",
                "  ⎿  先检查文件。\n",
                "● Bash(ls src)\n",
                "  ⎿  lib.rs\n",
                "     main.rs\n",
                "● Bash(head -n 1 Cargo.toml)\n",
                "  ⎿  [package]\n",
                "● Bash(grep needle missing.txt)\n",
                "  ⎿  Error: Exit code 2\n",
                "     [stderr]\n",
                "     grep: missing.txt: No such file or directory\n",
                "● Read({read_path})\n",
                "  ⎿  Read 2 lines\n",
                "● Bash(cargo check)\n",
                "  ⎿  Finished dev profile"
            ),
            read_path = read_path
        ),
        "详细态必须逐条恢复命令、输出、错误头和思考正文，且不显示展开提示"
    );
}

#[test]
fn test_render_single_bash_inspect_group_standalone_summary_snapshot() {
    let messages = vec![
        BaseMessage::ai_from_blocks(vec![ContentBlock::ToolUse {
            id: "single".into(),
            name: "Bash".into(),
            input: json!({"command": "ls", "file_inspect": true}),
        }]),
        BaseMessage::tool_result("single", "file.txt"),
    ];
    let views = make_restored_views(&messages);
    assert_eq!(
        make_rendered_text(&views, false),
        "  Ran 1 shell command (ctrl+o to expand)",
        "无思考的单条文件查询也应折叠并提示展开"
    );
    assert_eq!(
        make_rendered_text(&views, true),
        "● Bash(ls)\n  ⎿  file.txt",
        "展开后应显示完整明细且不残留展开提示"
    );
}
