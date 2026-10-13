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
        handle.contains(long_marker),
        "工具结束后条目进入最短可见停留（<=300ms），此期间仍应显示摘要"
    );

    // 让最短可见停留立即过期，模拟 300ms 之后的状态
    for entry in app.session_mgr.current_mut().agent.recent_tools.iter_mut() {
        entry.visible_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    }
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

// ── thinking 状态行：第三字段与热度色 ──────────────────────────────

#[test]
fn test_thinking_status_word_time_ladder() {
    use super::message_area::{
        thinking_status_word, DEEP_THOUGHT_SECS, STILL_THINKING_SECS, THINKING_MORE_SECS,
    };
    // Non-thinking phase without prior thought -> empty
    assert_eq!(thinking_status_word(false, 0, 0), "");
    assert_eq!(thinking_status_word(false, 99_000, 0), "");
    // Non-thinking phase with prior thought -> thought for Ns
    assert_eq!(
        thinking_status_word(false, 0, 4_200),
        "thought for 4s"
    );
    // < 5s -> thinking
    assert_eq!(thinking_status_word(true, 2_000, 0), "thinking");
    assert_eq!(
        thinking_status_word(true, (THINKING_MORE_SECS * 1000) - 1, 0),
        "thinking"
    );
    // 5s ~ 15s -> thinking more
    assert_eq!(
        thinking_status_word(true, THINKING_MORE_SECS * 1000, 0),
        "thinking more"
    );
    assert_eq!(
        thinking_status_word(true, (STILL_THINKING_SECS * 1000) - 1, 0),
        "thinking more"
    );
    // 15s ~ 60s -> still thinking
    assert_eq!(
        thinking_status_word(true, STILL_THINKING_SECS * 1000, 0),
        "still thinking"
    );
    assert_eq!(
        thinking_status_word(true, (DEEP_THOUGHT_SECS * 1000) - 1, 0),
        "still thinking"
    );
    // >= 60s -> deep in thought
    assert_eq!(
        thinking_status_word(true, DEEP_THOUGHT_SECS * 1000, 0),
        "deep in thought"
    );
    assert_eq!(
        thinking_status_word(true, 143_000, 0),
        "deep in thought"
    );
}

#[test]
fn test_thinking_heat_color_four_levels() {
    use super::message_area::{thinking_heat_color, HEAT_LV2_SECS, HEAT_LV3_SECS, HEAT_LV4_SECS};
    use crate::ui::theme;
    // Default level
    assert_eq!(thinking_heat_color(0), theme::ACCENT);
    assert_eq!(
        thinking_heat_color((HEAT_LV2_SECS - 1) * 1000),
        theme::ACCENT
    );
    // Step-by-step heating
    assert_eq!(
        thinking_heat_color(HEAT_LV2_SECS * 1000),
        theme::SPINNER_HEAT_LV2
    );
    assert_eq!(
        thinking_heat_color(HEAT_LV3_SECS * 1000),
        theme::SPINNER_HEAT_LV3
    );
    assert_eq!(thinking_heat_color(HEAT_LV4_SECS * 1000), theme::WARNING);
    assert_eq!(thinking_heat_color(999_000), theme::WARNING);
}

#[test]
fn test_thinking_status_style_pure_time_temperature() {
    use super::message_area::thinking_status_style;
    use ratatui::style::Color;
    let verb_heat = theme::SPINNER_HEAT_LV2;

    // 0s ~ 2.5s -> muted gray
    assert_eq!(
        thinking_status_style("thinking", 1_000, verb_heat, false).fg,
        Some(theme::MUTED)
    );
    // 2.5s ~ 5s -> soft white
    assert_eq!(
        thinking_status_style("thinking", 3_500, verb_heat, false).fg,
        Some(theme::TEXT_SOFT)
    );
    // 5s ~ 15s -> verb heat color
    assert_eq!(
        thinking_status_style("thinking more", 8_000, verb_heat, false).fg,
        Some(verb_heat)
    );
    // 15s ~ 60s -> heat lv3 light gold
    assert_eq!(
        thinking_status_style("still thinking", 20_000, verb_heat, false).fg,
        Some(theme::SPINNER_HEAT_LV3)
    );
    // >= 60s -> deep amber gold
    assert_eq!(
        thinking_status_style("deep in thought", 143_000, verb_heat, false).fg,
        Some(Color::Rgb(255, 152, 0))
    );
    // Completed state -> always muted gray
    assert_eq!(
        thinking_status_style("thought for 4s", 4_000, verb_heat, false).fg,
        Some(theme::MUTED)
    );
    // Compact mode -> always thinking purple
    assert_eq!(
        thinking_status_style("thinking", 1_000, verb_heat, true).fg,
        Some(theme::THINKING)
    );
}

use crate::ui::render_thread::WrappedLineInfo;
use ratatui::text::Line;

/// 直接构造可滚动渲染缓存，绕开真实消息管线。
///
/// `scrollbar_max_offset` 在真实路径由 `message_area::render_messages` 依据内容行数与
/// 可见高度算出；而 `view_messages` 为空时该函数会提前返回（渲染 Welcome 页），
/// 不会写入该字段。这里手动补齐"内容溢出可滚动"这一前置信号，作为按钮显示的基线。
fn seed_scrollable_cache(app: &mut App, total_lines: usize) {
    {
        let mut cache = app.session_mgr.current().messages.render_cache.write();
        cache.width = 79;
        cache.total_lines = total_lines;
        cache.lines = (0..total_lines)
            .map(|i| Line::from(format!("row {i}")))
            .collect();
        cache.wrap_map = (0..total_lines)
            .map(|i| WrappedLineInfo {
                line_idx: i,
                visual_row_start: i,
                visual_row_end: i + 1,
                plain_text: format!("row {i}"),
                char_widths: vec![],
            })
            .collect();
    }
    // 可见高度取 10：max = total - visible，只要内容多于可见行即为溢出（> 0）。
    app.session_mgr.current_mut().ui.scrollbar_max_offset = total_lines.saturating_sub(10);
}

#[tokio::test]
async fn test_back_to_bottom_button_visible_when_scrolled_away() {
    let (mut app, mut handle) = crate::app::App::new_headless(80, 20).await;
    seed_scrollable_cache(&mut app, 100);
    {
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = false;
        ui.scroll_offset = 0;
        ui.scroll_baseline_lines = 100; // 基线==当前内容，无新增
    }
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        app.session_mgr.current().ui.scroll_to_bottom_area.is_some(),
        "离开底部且内容溢出时应显示回到最新按钮"
    );
    let label = app.services.lc.tr("scroll-back-to-bottom");
    assert!(handle.contains(&label), "应渲染默认按钮文案: {label}");
}

#[tokio::test]
async fn test_back_to_bottom_button_hidden_at_bottom() {
    let (mut app, mut handle) = crate::app::App::new_headless(80, 20).await;
    seed_scrollable_cache(&mut app, 100);
    app.session_mgr.current_mut().ui.scroll_follow = true;
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        app.session_mgr.current().ui.scroll_to_bottom_area.is_none(),
        "在底部时不显示回到最新按钮"
    );
}

#[tokio::test]
async fn test_back_to_bottom_button_switches_to_new_message_when_content_grows() {
    let (mut app, mut handle) = crate::app::App::new_headless(80, 20).await;
    seed_scrollable_cache(&mut app, 100);
    {
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = false;
        ui.scroll_offset = 0;
        ui.scroll_baseline_lines = 100; // 离开底部时基线 100 行
    }
    seed_scrollable_cache(&mut app, 140); // 离开后内容增长到 140 行
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        app.session_mgr.current().ui.scroll_to_bottom_unseen,
        "离开底部后内容增长应标记未读"
    );
    let label = app.services.lc.tr("scroll-back-to-bottom-new");
    assert!(handle.contains(&label), "文案应切换为有新消息: {label}");
}

#[tokio::test]
async fn test_back_to_bottom_button_is_horizontally_centered() {
    let (mut app, mut handle) = crate::app::App::new_headless(80, 20).await;
    seed_scrollable_cache(&mut app, 100);
    {
        let ui = &mut app.session_mgr.current_mut().ui;
        ui.scroll_follow = false;
        ui.scroll_offset = 0;
        ui.scroll_baseline_lines = 100;
    }
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let area = app
        .session_mgr
        .current()
        .ui
        .scroll_to_bottom_area
        .expect("离开底部时应显示回到最新按钮");
    // 终端宽 80：按钮中心 ×2 应落在屏幕中心 ×2（80）附近，误差 ≤1 列
    // （整数列无法精确到半列，故允许 1 列；旧实现因预留滚动条列会左偏 1 列而超出容差）。
    let center2 = i32::from(area.x) * 2 + i32::from(area.width);
    assert!(
        (center2 - 80).abs() <= 1,
        "按钮未水平居中：{area:?} center2={center2}"
    );
}

/// PRD §4.8 状态栏排版规范回归：token 括号内冒号后必须有空格（严防 `in18s` 这类粘连）、
/// `⏱` 后固定 2 个空格、段落分隔符统一 ` | `、各行首列 1 空格缩进。
#[tokio::test]
async fn test_status_bar_typography_spacing_and_alignment() {
    let (mut app, mut handle) = crate::app::App::new_headless(120, 24).await;

    // 覆盖 1.5M / 388.9k / 18 三档数值（含非整千，验证一位小数分支）
    app.push_agent_event(crate::app::AgentEvent::TokenUsageUpdate {
        usage: cc_agent::llm::types::TokenUsage {
            input_tokens: 388_900,
            output_tokens: 18,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            request_id: None,
        },
        model: "test-model".into(),
        stop_reason: None,
    });
    app.process_pending_events();
    {
        let agent = &mut app.session_mgr.current_mut().agent;
        agent.session_token_tracker.total_input_tokens = 1_500_000;
        agent.session_token_tracker.total_output_tokens = 8_000;
        // 无 start_time 时 `⏱` 段整体不渲染，必须显式设置
        agent.session_start_time = Some(std::time::Instant::now());
        agent.context_window = 200_000;
    }

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snapshot = handle.snapshot();
    let height = super::status_bar::status_bar_height(&app) as usize;
    let rows: Vec<String> = snapshot[snapshot.len() - height..].to_vec();
    let joined = rows.join("\n");
    let first = &rows[0];

    // 1) 冒号后必须有空格 —— 直接防范 PRD 点名的 `in18s` / `in:389k,out:18` 粘连
    assert!(first.contains("tok: "), "缺 `tok: ` 空格：\n{joined}");
    assert!(first.contains("(in: "), "缺 `(in: ` 空格（即 `in18` 类粘连）：\n{joined}");
    assert!(first.contains(", out: "), "缺 `, out: ` 空格：\n{joined}");
    assert!(
        !first.contains("(in:388") && !first.contains("in18"),
        "出现粘连的 token 串：\n{joined}"
    );

    // 2) 时钟 emoji 后固定 2 个半角空格
    assert!(first.contains("\u{23f1}  "), "`\u{23f1}` 后应为 2 空格：\n{joined}");

    // 3) 段落分隔符统一 ` | `
    assert!(first.contains(" | "), "第一行缺 ` | ` 分隔符：\n{joined}");

    // 4) 每一行首列均为 1 个空格缩进（三行左边缘对齐）
    for (index, row) in rows.iter().enumerate() {
        assert!(
            row.starts_with(' ') && !row.starts_with("  "),
            "第 {} 行首列应为 1 空格缩进，实际以 {:?} 开头：\n{joined}",
            index + 1,
            row.chars().take(3).collect::<String>()
        );
    }
}

/// 抽屉提示行随游标位置切换：游标停在「正在查看的那一行」时 `Enter to view`/`x to stop`
/// 无意义，应缩短为 `↑/↓ to select`（对齐参考实现）。
#[tokio::test]
async fn test_drawer_hint_shortens_when_cursor_on_active_row() {
    const SHORT: &str = "\u{2191}/\u{2193} to select \u{b7} ctrl+x ctrl+k to stop all agents";
    const LONG: &str = "Enter to view \u{b7} x to stop \u{b7} ctrl+x ctrl+k to stop all agents";

    let (mut app, mut handle) = crate::app::App::new_headless(120, 30).await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "child-1",
        "agentType": "general-purpose",
        "taskPreview": "查天气",
        "isBackground": true,
    }));

    for (cursor, focus, expected) in [
        (Some(0usize), None, SHORT),
        (Some(1usize), Some("child-1"), SHORT),
        (Some(1usize), None, LONG),
        (None, None, ""),
    ] {
        {
            let session = app.session_mgr.current_mut();
            session.ui.bg_bar_cursor = cursor;
            session.focused_instance_id = focus.map(str::to_string);
        }
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        let joined = handle.snapshot().join("\n");

        if expected.is_empty() {
            assert!(
                !joined.contains(SHORT) && !joined.contains(LONG),
                "游标不在抽屉时不应出现抽屉提示：\n{joined}"
            );
            continue;
        }
        assert!(
            joined.contains(expected),
            "cursor={cursor:?} focus={focus:?} 应含 {expected:?}，实际:\n{joined}"
        );
        let other = if expected == SHORT { LONG } else { SHORT };
        assert!(
            !joined.contains(other),
            "cursor={cursor:?} focus={focus:?} 不应出现另一种提示 {other:?}：\n{joined}"
        );
    }
}

/// 回归：后台 Agent 全部完成后，抽屉收起，第三行必须恢复正常形态。
///
/// 症状：子 Agent 都跑完了，人已回到 main，第三行仍卡在
/// `↑/↓ to select · ctrl+x ctrl+k to stop all agents`。
/// 根因：`BackgroundTaskCompleted` 只在「被移除的正是当前聚焦 Agent」时才清
/// `bg_bar_cursor`，游标停在 main 行时会残留。
#[tokio::test]
async fn test_drawer_hint_clears_after_all_agents_finish() {
    const DRAWER_HINT: &str = "ctrl+x ctrl+k to stop all agents";

    let (mut app, mut handle) = crate::app::App::new_headless(120, 30).await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "child-1",
        "agentType": "general-purpose",
        "taskPreview": "查天气",
        "isBackground": true,
    }));
    // 游标停在 main 行（不是聚焦子 Agent）
    app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let before = handle.snapshot().join("\n");
    assert!(
        before.contains(DRAWER_HINT),
        "抽屉导航态应先显示提示：\n{before}"
    );

    app.push_agent_event(crate::app::AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-1".into(),
        agent_name: "general-purpose".into(),
        success: true,
        output: "LGTM".into(),
        tool_calls_count: 1,
        duration_ms: 1_000,
        child_thread_id: Some("child-1".into()),
    });
    app.process_pending_events();

    assert!(
        app.session_mgr.current().background_agents.is_empty(),
        "完成后后台列表应清空"
    );
    assert_eq!(
        app.session_mgr.current().ui.bg_bar_cursor,
        None,
        "抽屉收起后残留游标必须被清掉"
    );

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let after = handle.snapshot().join("\n");
    assert!(
        !after.contains(DRAWER_HINT),
        "全部完成后第三行应恢复正常形态：\n{after}"
    );

    // 防御：即便游标被别处残留（后台列表已空），也不得再进入抽屉导航态
    app.session_mgr.current_mut().ui.bg_bar_cursor = Some(0);
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let defensive = handle.snapshot().join("\n");
    assert!(
        !defensive.contains(DRAWER_HINT),
        "后台列表为空时不得渲染抽屉导航提示：\n{defensive}"
    );
}

/// 回归：抽屉右侧是**单个**箭头，方向跟着 token 流向走。
///
/// `↓` = 下行 = Agent 回复（产出，取最近一次调用的 output）；`↑` = 上行 = 用户提问（输入，取 input）。
/// 原先用的是 `↓ {input+output} tokens`——箭头标着单向、数字却是合计，口径对不上。
#[tokio::test]
async fn test_drawer_metrics_follows_token_flow_with_single_arrow() {
    let (mut app, mut handle) = crate::app::App::new_headless(140, 30).await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "child-1",
        "agentType": "general-purpose",
        "taskPreview": "查天气",
        "isBackground": true,
    }));
    app.handle_subagent_event(
        "child-1",
        crate::app::AgentEvent::TokenUsageUpdate {
            usage: cc_agent::llm::types::TokenUsage {
                input_tokens: 32_700,
                output_tokens: 259,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                request_id: None,
            },
            model: "m".into(),
            stop_reason: None,
        },
    );

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let joined = handle.snapshot().join("\n");

    // Agent 产出回复 → 只有下行箭头，取最近一次调用的 output。
    assert!(joined.contains("↓ 259"), "Agent 回复应显示下行箭头 + output：\n{joined}");
    assert!(
        !joined.contains("↓ 32.7k") && !joined.contains("↑ "),
        "不得并列显示输入/输出，也不得用累计值冒充单向 token：\n{joined}"
    );

    // 用户向该子 Agent 提问 → 箭头转上行，取最近一次调用的 input。
    app.session_mgr.current_mut().focused_instance_id = Some("child-1".into());
    app.send_subagent_input("再确认一下".into());
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let after_ask = handle.snapshot().join("\n");
    assert!(after_ask.contains("↑ 32.7k"), "用户提问后应转为上行箭头 + input：\n{after_ask}");
    assert!(!after_ask.contains("↓ 259"), "提问后不得再留在下行态：\n{after_ask}");
}

/// 回归：抽屉中间列展示「子 Agent 在说什么」，不是工具调用串。
///
/// 参考实现里这一列是助手的自然语言（"等待 60 秒休眠中"），工具细节留给用户
/// Enter 进视口查看；之前写的是 `Bash(sleep 90 && echo ...)`。
#[tokio::test]
async fn test_drawer_activity_shows_assistant_text_not_tool_call() {
    use crate::app::AgentEvent;

    let (mut app, mut handle) = crate::app::App::new_headless(140, 30).await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "child-1",
        "agentType": "general-purpose",
        "taskPreview": "原始任务描述",
        "isBackground": true,
    }));

    let tool_start = |args: &str| AgentEvent::ToolStart {
        tool_call_id: "t1".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: args.into(),
        input: serde_json::json!({ "command": args }),
        source_agent_id: None,
    };
    let chunk = |text: &str| AgentEvent::AssistantChunk {
        chunk: text.into(),
        source_agent_id: None,
    };
    let render = |app: &mut crate::app::App, handle: &mut crate::ui::headless::HeadlessHandle| {
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, app))
            .unwrap();
        handle.snapshot().join("\n")
    };

    // 只有工具在跑：不得把工具串当正文
    app.handle_subagent_event("child-1", tool_start("sleep 90 && echo done"));
    let snap = render(&mut app, &mut handle);
    assert!(
        !snap.contains("Bash(sleep 90"),
        "工具调用串不应出现在抽屉正文：\n{snap}"
    );

    // 助手开口 → 显示助手文本
    app.handle_subagent_event("child-1", chunk("First I will sleep 60 seconds, "));
    app.handle_subagent_event("child-1", chunk("then query Shenzhen weather."));
    let snap = render(&mut app, &mut handle);
    assert!(
        snap.contains("First I will sleep 60 seconds, then query Shenzhen weather."),
        "应展示助手文本：\n{snap}"
    );

    // 一句话结束 → 复位；下一句替换掉旧句
    app.handle_subagent_event(
        "child-1",
        AgentEvent::ToolEnd {
            tool_call_id: "t1".into(),
            name: "Bash".into(),
            output: "done".into(),
            is_error: false,
            source_agent_id: None,
        },
    );
    app.handle_subagent_event("child-1", chunk("Sleeping for 60 seconds"));
    let snap = render(&mut app, &mut handle);
    assert!(snap.contains("Sleeping for 60 seconds"), "应显示最新一句：\n{snap}");
    assert!(
        !snap.contains("then query Shenzhen weather."),
        "旧句应被替换：\n{snap}"
    );
}
