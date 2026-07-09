use super::*;

#[test]
fn test_textarea_shell_mode_detects_command_and_reset() {
    assert_eq!(
        textarea_shell_mode_from_text("!git log", false),
        TextareaShellMode::Command,
        "以 ! 开头时应进入本地 shell 命令态"
    );
    assert_eq!(
        textarea_shell_mode_from_text("git log", false),
        TextareaShellMode::None,
        "撤销 ! 后应恢复普通输入态"
    );
    assert_eq!(
        textarea_shell_mode_from_text("!ignored", true),
        TextareaShellMode::Stdin,
        "命令运行中应优先显示 stdin 输入态"
    );
}

#[test]
fn test_textarea_shell_command_uses_danger_prompt_and_border() {
    let (prompt, style) = textarea_prompt(TextareaShellMode::Command, false);
    assert_eq!(prompt, "!", "shell 命令态左侧提示符应显示 !");
    assert_eq!(
        style.fg,
        Some(theme::ERROR),
        "shell 命令态提示符应使用 danger 颜色"
    );
    assert_eq!(
        textarea_shell_border_color(TextareaShellMode::Command),
        theme::ERROR,
        "shell 命令态边框应使用 danger 颜色"
    );
    assert_eq!(
        textarea_shell_border_color(TextareaShellMode::None),
        theme::MUTED,
        "普通输入态边框应恢复默认颜色"
    );
}

#[test]
fn test_hide_shell_prefix_for_display_keeps_original_textarea_intact() {
    let mut original = TextArea::default();
    original.insert_str("!git log");
    let mut display = original.clone();

    hide_shell_prefix_for_display(&mut display);

    assert_eq!(
        original.lines(),
        ["!git log"],
        "真实输入内容应继续保留 ! 供提交识别"
    );
    assert_eq!(
        display.lines(),
        ["git log"],
        "shell 命令态展示副本应隐藏用户输入的 !"
    );
    assert_eq!(display.cursor(), (0, 7), "隐藏 ! 后展示光标应同步左移一列");
}

#[test]
fn test_hide_shell_prefix_for_display_handles_only_bang() {
    let mut display = TextArea::default();
    display.insert_str("!");

    hide_shell_prefix_for_display(&mut display);

    assert_eq!(display.lines(), [""], "只输入 ! 时文本域展示应为空");
    assert_eq!(display.cursor(), (0, 0), "只输入 ! 时光标应回到行首");
}

#[tokio::test]
async fn test_status_area_clears_long_agent_shell_text_after_tool_finishes() {
    let (mut app, mut handle) = crate::app::App::new_headless(100, 24).await;
    let long_marker = "residue-mark";
    app.push_agent_event(crate::app::AgentEvent::ToolStart {
        tool_call_id: "tc_status_residue".to_string(),
        name: "Bash".to_string(),
        display: "Bash".to_string(),
        args: format!("echo {long_marker}"),
        input: serde_json::json!({ "command": format!("echo {long_marker}") }),
        source_agent_id: None,
    });
    app.process_pending_events();

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains(long_marker),
        "首帧应显示模拟的 agent shell 命令摘要"
    );

    app.push_agent_event(crate::app::AgentEvent::ToolEnd {
        tool_call_id: "tc_status_residue".to_string(),
        name: "Bash".to_string(),
        output: "ok".to_string(),
        is_error: false,
        source_agent_id: None,
    });
    app.process_pending_events();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        !handle.contains(long_marker),
        "agent shell 状态消失后，底部状态区不应残留旧命令文本"
    );
}

#[tokio::test]
async fn test_status_bar_tool_history_aligns_with_other_rows() {
    let (mut app, mut handle) = crate::app::App::new_headless(100, 24).await;
    app.push_agent_event(crate::app::AgentEvent::ToolStart {
        tool_call_id: "tc_status_align".to_string(),
        name: "Bash".to_string(),
        display: "Bash".to_string(),
        args: "echo ok".to_string(),
        input: serde_json::json!({ "command": "echo ok" }),
        source_agent_id: None,
    });
    app.push_agent_event(crate::app::AgentEvent::ToolEnd {
        tool_call_id: "tc_status_align".to_string(),
        name: "Bash".to_string(),
        output: "ok".to_string(),
        is_error: false,
        source_agent_id: None,
    });
    app.process_pending_events();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snapshot = handle.snapshot();
    let status_rows = &snapshot[snapshot.len() - 3..];
    assert!(
        status_rows[1].contains("Bash"),
        "第二行应显示完成态工具统计，实际状态栏:\n{}",
        status_rows.join("\n")
    );
    let columns: Vec<_> = status_rows
        .iter()
        .map(|line| line.chars().position(|ch| ch != ' '))
        .collect();
    assert_eq!(
        columns,
        vec![Some(1), Some(1), Some(1)],
        "状态栏三行应从同一列开始，实际状态栏:\n{}",
        status_rows.join("\n")
    );
}

#[tokio::test]
async fn test_status_bar_first_row_uses_codebuddy_compact_shape() {
    let (mut app, mut handle) = crate::app::App::new_headless(120, 24).await;
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snapshot = handle.snapshot();
    let height = super::status_bar::status_bar_height(&app) as usize;
    let status_rows = &snapshot[snapshot.len() - height..];
    let first_row = &status_rows[0];
    assert!(
        first_row.contains('[') && first_row.contains(']'),
        "第一行应显示 codebuddy-hud 风格的 [model]，实际:\n{}",
        status_rows.join("\n")
    );
    assert!(
        first_row.contains("░░░░░░░░░░ 0%"),
        "无上下文数据时应显示 0% context bar，实际:\n{}",
        status_rows.join("\n")
    );
    let model_end = first_row.find(']').unwrap();
    let context_pos = first_row.find("0%").unwrap();
    let separator_pos = first_row.find(" | ").unwrap();
    assert!(
        model_end < context_pos && context_pos < separator_pos,
        "第一行应按 [model] context | project 排列，实际:\n{}",
        status_rows.join("\n")
    );
}

#[tokio::test]
async fn test_status_bar_activity_shows_last_two_running_tools() {
    let (mut app, mut handle) = crate::app::App::new_headless(120, 24).await;
    for (id, name, args) in [
        ("tc1", "Read", "src/first.rs"),
        ("tc2", "Bash", "cargo test"),
        ("tc3", "Grep", "needle"),
    ] {
        app.push_agent_event(crate::app::AgentEvent::ToolStart {
            tool_call_id: id.to_string(),
            name: name.to_string(),
            display: name.to_string(),
            args: args.to_string(),
            input: serde_json::json!({}),
            source_agent_id: None,
        });
    }
    app.process_pending_events();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snapshot = handle.snapshot();
    let status_rows = &snapshot[snapshot.len() - 3..];
    let activity = &status_rows[1];
    assert!(
        !activity.contains("first.rs"),
        "activity 行最多显示最后两个 running tools，实际:\n{}",
        status_rows.join("\n")
    );
    assert!(
        activity.contains("◐ Bash : cargo test") && activity.contains("◐ Grep : needle"),
        "activity 行应显示最后两个 running tools，实际:\n{}",
        status_rows.join("\n")
    );

    app.push_agent_event(crate::app::AgentEvent::ToolEnd {
        tool_call_id: "tc2".to_string(),
        name: "Bash".to_string(),
        output: "ok".to_string(),
        is_error: false,
        source_agent_id: None,
    });
    app.process_pending_events();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snapshot = handle.snapshot();
    let status_rows = &snapshot[snapshot.len() - 3..];
    let activity = &status_rows[1];
    assert!(
        activity.contains("◐ Grep : needle") && activity.contains("✓ Bash ×1"),
        "tool end 后应保留仍在 running 的工具并增加完成计数，实际:\n{}",
        status_rows.join("\n")
    );
}
