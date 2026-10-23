use super::*;
use crate::ui::message_view::ToolPhase;
use std::time::{Duration, Instant};

fn make_bash(tail: &str, total_lines: usize) -> MessageViewModel {
    let mut vm = MessageViewModel::tool_block_with_id(
        "streamed-bash".into(),
        "Bash".into(),
        "Bash".into(),
        Some("build".into()),
        false,
    );
    if let MessageViewModel::ToolBlock {
        phase,
        live_output,
        started_at,
        timeout_ms,
        ..
    } = &mut vm
    {
        *phase = ToolPhase::Running;
        *live_output = Some(ToolOutputPreview::new(
            tail.into(),
            total_lines,
            2048,
            22_000,
        ));
        *started_at = Some(Instant::now() - Duration::from_secs(22));
        *timeout_ms = Some(300_000);
    }
    vm.recompute_hash();
    vm
}

fn make_text(vm: &MessageViewModel, width: usize, detail: bool) -> String {
    render_view_model(vm, None, width, detail, 4)
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_running_bash_shows_last_five_visual_lines_and_plus_count() {
    let vm = make_bash("one\ntwo\nthree\nfour\nfive\nsix\nseven\n", 7);
    let text = make_text(&vm, 100, false);
    assert!(
        text.contains("  ⎿  three\n     four\n     five\n     six\n     seven\n     … +2 lines"),
        "应展示最后五行及精确省略数量: {text}"
    );
    assert!(
        !text.contains("Running…"),
        "有输出时状态尾注不带Running前缀"
    );
    assert!(
        text.contains("     (22s · timeout 5m · 2.0 KB)\n     (ctrl+b to run in background)"),
        "耗时、显式期限、字节数和后台提示应按序呈现: {text}"
    );
}

#[test]
fn test_running_bash_detail_keeps_entire_tail_and_counts_uncached_lines() {
    let vm = make_bash("one\ntwo\nthree\nfour\nfive\nsix\nseven\n", 100);
    let compact = make_text(&vm, 100, false);
    let detail = make_text(&vm, 100, true);
    assert!(
        compact.contains("… +95 lines"),
        "普通模式应合计缓存外与显示外的行"
    );
    assert!(detail.contains("  ⎿  one\n     two\n     three"));
    assert!(detail.contains("… +93 lines"), "详细模式仍应注明缓存外行数");
    assert!(!detail.contains("… +95 lines"));
}

#[test]
fn test_running_bash_tail_waits_for_two_seconds_and_subagent_has_no_hint() {
    let mut vm = make_bash("preview", 1);
    if let MessageViewModel::ToolBlock { started_at, .. } = &mut vm {
        *started_at = Some(Instant::now());
    }
    let before = make_text(&vm, 100, false);
    assert!(
        !before.contains("preview") && !before.contains("ctrl+b"),
        "不足两秒只显示工具头"
    );
    if let MessageViewModel::ToolBlock {
        started_at,
        source_agent_id,
        ..
    } = &mut vm
    {
        *started_at = Some(Instant::now() - Duration::from_secs(22));
        *source_agent_id = Some("child".into());
    }
    let child = make_text(&vm, 100, false);
    assert!(child.contains("preview") && child.contains("22s"));
    assert!(!child.contains("ctrl+b"), "子Agent没有前台移交快捷键");
}

#[test]
fn test_running_tail_reuses_finished_output_decorators() {
    for tail in [
        "\u{1b}[31mred\u{1b}[0m\nhttps://example.com/build/log\nstderr remains visible".to_owned(),
        "{\"items\":[\"中文日志\",\"https://example.com/data\"],\"ok\":true}".to_owned(),
        "这是很长的中文构建输出，需要使用与完成结果相同的规则折行。".repeat(5),
    ] {
        let running = make_bash(&tail, tail.lines().count());
        let mut finished = running.clone();
        if let MessageViewModel::ToolBlock {
            phase,
            content,
            live_output,
            ..
        } = &mut finished
        {
            *phase = ToolPhase::Finished;
            *content = tail;
            *live_output = None;
        }
        let live_lines = render_view_model(&running, None, 48, true, 4);
        let finished_lines = render_view_model(&finished, None, 48, true, 4);
        let live_body = &live_lines[1..live_lines.len() - 2];
        let final_body = &finished_lines[1..finished_lines.len() - 1];
        assert_eq!(
            live_body, final_body,
            "同一文本的ANSI、URL、JSON与折行样式必须一致"
        );
        let compact = make_text(&running, 48, false);
        if live_body.len() > 5 {
            let omitted = live_body.len() - 5;
            assert!(
                compact.contains(&format!(
                    "… +{omitted} {}",
                    if omitted == 1 { "line" } else { "lines" }
                )),
                "折行与JSON展开应计入视觉省略行数: {compact}"
            );
        }
    }
}

#[test]
fn test_finished_bash_ignores_retained_preview() {
    let mut vm = make_bash("stale-preview", 1);
    if let MessageViewModel::ToolBlock { phase, content, .. } = &mut vm {
        *phase = ToolPhase::Finished;
        *content = "final-result".into();
    }
    let text = make_text(&vm, 100, false);
    assert!(text.contains("final-result"));
    assert!(!text.contains("stale-preview") && !text.contains("ctrl+b"));
}

#[test]
fn test_live_output_all_fields_change_view_equality_and_hash() {
    let original = make_bash("tail", 1);
    for field in 0..4 {
        let mut changed = original.clone();
        if let MessageViewModel::ToolBlock {
            live_output: Some(output),
            ..
        } = &mut changed
        {
            match field {
                0 => output.tail.push('x'),
                1 => output.total_lines += 1,
                2 => output.total_bytes += 1,
                _ => output.elapsed_ms += 1,
            }
        }
        changed.recompute_hash();
        assert_ne!(changed, original, "尾部和每项元数据均影响相等性");
        assert_ne!(
            changed.content_hash(),
            original.content_hash(),
            "尾部和每项元数据均影响渲染缓存键"
        );
    }
}
