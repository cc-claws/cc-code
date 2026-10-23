use super::*;
use crate::app::events::AgentEvent;
use serde_json::json;

fn make_start(id: &str, name: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolStart {
        tool_call_id: id.into(),
        name: name.into(),
        display: name.into(),
        args: "sleep 3".into(),
        input: json!({"command": "sleep 3"}),
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_output(id: &str, tail: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolOutput {
        tool_call_id: id.into(),
        tail: tail.into(),
        total_lines: tail.lines().count(),
        total_bytes: tail.len(),
        elapsed_ms: 1500,
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_preview(vm: &MessageViewModel) -> Option<&ToolOutputPreview> {
    match vm {
        MessageViewModel::ToolBlock { live_output, .. } => live_output.as_ref(),
        _ => None,
    }
}

#[test]
fn test_tool_output_updates_only_running_bash_without_changing_history_or_clock() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(AgentEvent::ToolPending {
        tool_call_id: "queued".into(),
        name: "Bash".into(),
        input: json!({"command": "sleep 3"}),
        source_agent_id: None,
    });
    pipeline.handle_event(make_start("bash", "Bash", None));
    pipeline.handle_event(make_start("read", "Read", None));
    let started_at = pipeline.pending_tools["bash"].started_at;
    for id in ["queued", "missing", "read", "bash"] {
        pipeline.handle_event(make_output(id, "preview", None));
    }
    assert!(pipeline.pending_tools["queued"].live_output.is_none());
    assert!(pipeline.pending_tools["read"].live_output.is_none());
    assert_eq!(pipeline.pending_tools.len(), 3, "未知输出不能创建工具");
    assert_eq!(pipeline.pending_tools["bash"].started_at, started_at);
    assert!(pipeline.completed.is_empty(), "输出不应写入BaseMessage历史");
    assert_eq!(
        pipeline.pending_tools["bash"]
            .live_output
            .as_ref()
            .map(|output| output.tail.as_str()),
        Some("preview")
    );
    let views = pipeline.build_tail_vms();
    assert_eq!(
        make_preview(&views[1]).map(|output| output.tail.as_str()),
        Some("preview")
    );
    assert!(
        matches!(&views[1], MessageViewModel::ToolBlock { content, .. } if content.is_empty()),
        "展示尾部不能覆盖最终结果正文"
    );
}

#[test]
fn test_tool_output_finished_and_interrupted_calls_ignore_late_events() {
    for is_error in [false, true] {
        let mut pipeline = MessagePipeline::new(".".into());
        pipeline.handle_event(make_start("bash", "Bash", None));
        pipeline.handle_event(make_output("bash", "before", None));
        pipeline.handle_event(AgentEvent::ToolEnd {
            tool_call_id: "bash".into(),
            name: "Bash".into(),
            output: if is_error {
                "interrupted by user"
            } else {
                "final"
            }
            .into(),
            is_error,
            source_agent_id: None,
        });
        pipeline.handle_event(make_output("bash", "late", None));
        let views = pipeline.build_tail_vms();
        assert!(
            matches!(&views[0], MessageViewModel::ToolBlock { phase:ToolPhase::Finished, live_output:None, content, .. } if content == if is_error { "interrupted by user" } else { "final" }),
            "迟到输出不得复活已完成或中断的工具"
        );
    }
}

#[test]
fn test_tool_output_main_and_child_with_same_id_have_independent_caches() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "child".into(),
        instance_id: "child-id".into(),
        task_preview: "task".into(),
        is_background: false,
        prompt: None,
    });
    pipeline.handle_event(make_start("same", "Bash", None));
    pipeline.handle_event(make_start("same", "Bash", Some("child-id")));
    pipeline.handle_event(make_output("same", "main preview", None));
    pipeline.handle_event(make_output("same", "child preview", Some("child-id")));
    pipeline.handle_event(make_output("same", "unknown source", Some("other-child")));
    assert_eq!(
        pipeline.pending_tools["same"]
            .live_output
            .as_ref()
            .map(|output| output.tail.as_str()),
        Some("main preview")
    );
    assert_eq!(
        make_preview(&pipeline.subagent_stack[0].recent_messages[0])
            .map(|output| output.tail.as_str()),
        Some("child preview")
    );
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "same".into(),
        name: "Bash".into(),
        output: "child final".into(),
        is_error: false,
        source_agent_id: Some("child-id".into()),
    });
    pipeline.handle_event(make_output("same", "late child", Some("child-id")));
    assert!(make_preview(&pipeline.subagent_stack[0].recent_messages[0]).is_none());
    assert!(
        pipeline.pending_tools["same"].live_output.is_some(),
        "子工具完成不能清空主工具尾部"
    );
}

#[test]
fn test_tool_output_rejects_older_progress() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(make_start("bash", "Bash", None));
    pipeline.handle_event(make_output("bash", "newest preview", None));
    pipeline.handle_event(make_output("bash", "old", None));
    assert_eq!(
        pipeline.pending_tools["bash"]
            .live_output
            .as_ref()
            .map(|output| output.tail.as_str()),
        Some("newest preview"),
        "较旧计数不能覆盖新尾部"
    );
}

#[test]
fn test_tool_output_background_handoff_removes_preview_and_rejects_more_output() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(make_start("bash", "Bash", None));
    pipeline.handle_event(make_output("bash", "foreground", None));
    pipeline.sync_shell_runtime(None, "bash", std::time::Instant::now(), 600_000, true);
    assert!(
        pipeline.pending_tools["bash"].live_output.is_none(),
        "后台移交应释放缓存而非仅隐藏"
    );
    pipeline.handle_event(make_output("bash", "background output", None));
    let mut views = pipeline.build_tail_vms();
    pipeline.apply_shell_runtime(&mut views);
    assert!(
        matches!(
            &views[0],
            MessageViewModel::ToolBlock {
                shell_backgrounded: true,
                live_output: None,
                ..
            }
        ),
        "后台化后不能残留前台输出尾部"
    );
}

#[test]
fn test_tool_output_large_input_keeps_bounded_utf8_tail_and_original_totals() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(make_start("bash", "Bash", None));
    let text = (0..100_000)
        .map(|index| format!("日志{index}\n"))
        .collect::<String>();
    let total_bytes = text.len();
    pipeline.handle_event(make_output("bash", &text, None));
    let views = pipeline.build_tail_vms();
    let preview = make_preview(&views[0]).expect("运行工具应保留展示缓存");
    assert!(
        preview.tail.len() <= 64 * 1024,
        "100000行不能变成无界VM缓存"
    );
    assert!(text.ends_with(&preview.tail), "截断需保留完整UTF-8字符尾部");
    assert!(preview.tail.ends_with("日志99999\n"));
    assert_eq!(preview.total_bytes, total_bytes);
    assert_eq!(preview.total_lines, 100_000);
    assert!(pipeline.completed.is_empty(), "大量进度事件也不能写历史");
}
