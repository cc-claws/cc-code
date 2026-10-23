use super::*;
use crate::ui::message_view::ToolPhase;
use std::time::{Duration, Instant};

fn make_bash(phase: ToolPhase, output: &str) -> MessageViewModel {
    let mut vm = MessageViewModel::tool_block_with_id(
        "bash-status".into(),
        "Bash".into(),
        "Bash".into(),
        Some("true".into()),
        false,
    );
    if let MessageViewModel::ToolBlock {
        phase: current,
        content,
        started_at,
        ..
    } = &mut vm
    {
        *current = phase;
        *content = output.into();
        *started_at = Some(Instant::now() - Duration::from_secs(22));
    }
    vm.recompute_hash();
    vm
}

fn make_text_with_tick(vm: &MessageViewModel, detail: bool, tick: u64) -> String {
    render_view_model(vm, None, 100, detail, tick)
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

fn make_text(vm: &MessageViewModel, detail: bool) -> String {
    make_text_with_tick(vm, detail, 4)
}

#[test]
fn test_tool_phase_finished_with_empty_output_shows_done() {
    for output in [
        "",
        " \r\n\t ",
        "[Command completed with exit code 0]",
        "\n[Exit code: 0]",
    ] {
        let vm = make_bash(ToolPhase::Finished, output);
        let text = make_text(&vm, false);
        assert!(
            text.starts_with("● Bash(true)"),
            "完成后指示器应停止闪烁: {text}"
        );
        assert!(text.contains("  ⎿  Done"), "空输出应显示 Done: {text}");
        assert!(!text.contains("Running…"), "完成后不能保留计时: {text}");
        assert!(!text.contains("ctrl+b"), "完成后不能保留后台提示: {text}");
        let detail = make_text(&vm, true);
        assert!(
            detail.contains("  ⎿  (No output)"),
            "详细模式应显示空输出文案: {detail}"
        );
    }
}

#[test]
fn test_shell_running_text_includes_timeout_with_dot() {
    assert_eq!(
        shell_running_text(Instant::now() - Duration::from_secs(22), Some(300_000)),
        "Running… (22s · timeout 5m)"
    );
}

#[test]
fn test_shell_running_text_without_timeout() {
    assert_eq!(
        shell_running_text(Instant::now() - Duration::from_secs(22), None),
        "Running… (22s)"
    );
}

#[test]
fn test_ctrl_b_hint_follows_running_line() {
    let text = make_text(&make_bash(ToolPhase::Running, ""), false);
    assert!(
        text.contains("  ⎿  Running… (22s)\n     (ctrl+b to run in background)"),
        "后台提示应在状态之后以五空格悬挂: {text}"
    );
}

#[test]
fn test_tui_renders_waiting_for_permission_during_hitl() {
    let bash = make_bash(ToolPhase::AwaitingApproval, "");
    // 审批等待期恒亮不闪（对齐 CC）：任意 tick 都应显示实心圆。
    for tick in [0, 4, 8] {
        let text = make_text_with_tick(&bash, false, tick);
        assert!(
            text.starts_with("● Bash"),
            "审批阶段圆点应恒亮（tick={tick}）: {text}"
        );
    }
    let text = make_text(&bash, false);
    assert!(
        text.contains("  ⎿  Waiting for permission…\n     (ctrl+b to run in background)"),
        "审批状态应紧接后台提示: {text}"
    );
    assert!(!text.contains("22s"), "审批阶段不能计入执行耗时: {text}");
}

#[test]
fn test_tui_renders_waiting_during_jev_judge() {
    let bash = make_bash(ToolPhase::Queued, "");
    // Queued 恒亮不闪（对齐 CC）：排队不是「活跃」，等待语义由文案承载。
    for tick in [0, 4, 8] {
        let text = make_text_with_tick(&bash, false, tick);
        assert!(
            text.starts_with("● Bash"),
            "Queued 阶段圆点应恒亮（tick={tick}）: {text}"
        );
    }
    let text = make_text(&bash, false);
    assert!(
        text.contains("  ⎿  Waiting…\n     (ctrl+b to run in background)"),
        "自动判定保持Queued状态: {text}"
    );
    assert!(
        !text.contains("permission"),
        "自动判定不应伪造人工审批: {text}"
    );
    assert!(!text.contains("22s"), "Queued不应计时: {text}");
}

#[test]
fn test_subagent_bash_has_no_background_hint() {
    let mut vm = make_bash(ToolPhase::Running, "");
    if let MessageViewModel::ToolBlock {
        source_agent_id, ..
    } = &mut vm
    {
        *source_agent_id = Some("child".into());
    }
    let text = make_text(&vm, false);
    assert!(
        text.contains("Running…"),
        "子 Agent 应显示同样的运行态: {text}"
    );
    assert!(!text.contains("ctrl+b"), "子 Agent 不提供后台入口: {text}");
}

#[test]
fn test_non_bash_tool_has_no_running_line() {
    let mut vm = make_bash(ToolPhase::Running, "");
    if let MessageViewModel::ToolBlock {
        tool_name,
        display_name,
        ..
    } = &mut vm
    {
        *tool_name = "Write".into();
        *display_name = "Write".into();
    }
    let text = make_text(&vm, false);
    assert!(!text.contains("Running…"), "非 Bash 不新增状态行: {text}");
    assert!(!text.contains("ctrl+b"), "非 Bash 不新增后台提示: {text}");
}

#[test]
fn test_finished_bash_timeout_is_separate_branch_line() {
    let mut vm = make_bash(ToolPhase::Finished, "hello");
    if let MessageViewModel::ToolBlock { timeout_ms, .. } = &mut vm {
        *timeout_ms = Some(300_000);
    }
    let text = make_text(&vm, false);
    assert!(
        text.contains("  ⎿  hello\n  ⎿  (timeout 5m)"),
        "完成后应单独保留 timeout 尾注: {text}"
    );
    assert!(!text.contains("Done"), "有输出时不能显示 Done: {text}");
}

#[test]
fn test_tool_phase_changes_equality_and_hash() {
    let mut running = make_bash(ToolPhase::Running, "");
    let mut finished = running.clone();
    if let MessageViewModel::ToolBlock { phase, .. } = &mut finished {
        *phase = ToolPhase::Finished;
    }
    running.recompute_hash();
    finished.recompute_hash();
    assert_ne!(running, finished, "阶段必须参与相等性");
    assert_ne!(
        running.content_hash(),
        finished.content_hash(),
        "阶段必须参与渲染 hash"
    );
}
