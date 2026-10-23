use super::*;
use serde_json::json;

fn make_pending(id: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolPending {
        tool_call_id: id.into(),
        name: "Bash".into(),
        input: json!({"command":"sleep 3"}),
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_start(id: &str) -> AgentEvent {
    AgentEvent::ToolStart {
        tool_call_id: id.into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "sleep 3".into(),
        input: json!({"command":"sleep 3"}),
        source_agent_id: None,
    }
}

fn make_phases(pipeline: &MessagePipeline) -> Vec<(String, ToolPhase)> {
    pipeline
        .build_tail_vms()
        .iter()
        .filter_map(|vm| match vm {
            MessageViewModel::ToolBlock {
                tool_call_id,
                phase,
                ..
            } => Some((tool_call_id.clone(), *phase)),
            _ => None,
        })
        .collect()
}

#[test]
fn test_queued_tools_keep_original_order_during_approval() {
    let mut pipeline = MessagePipeline::new(".".into());
    for id in ["first", "second", "third"] {
        pipeline.handle_event(make_pending(id, None));
    }
    assert!(
        pipeline.mark_tool_awaiting_approval(None, "second"),
        "应推进准确匹配的审批工具"
    );
    assert_eq!(
        make_phases(&pipeline),
        vec![
            ("first".into(), ToolPhase::Queued),
            ("second".into(), ToolPhase::AwaitingApproval),
            ("third".into(), ToolPhase::Queued)
        ]
    );
    assert!(
        pipeline
            .pending_tools
            .values()
            .all(|tool| tool.started_at.is_none()),
        "审批和排队不应开始执行计时"
    );
}

#[test]
fn test_pending_then_start_has_one_running_block_and_never_regresses() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(make_pending("one", None));
    pipeline.handle_event(make_start("one"));
    let started_at = pipeline.pending_tools["one"].started_at;
    pipeline.handle_event(make_start("one"));
    assert_eq!(
        pipeline.pending_tools["one"].started_at, started_at,
        "重复Start不能重置计时起点"
    );
    pipeline.handle_event(make_pending("one", None));
    assert!(
        !pipeline.mark_tool_awaiting_approval(None, "one"),
        "迟到审批不能回退运行阶段"
    );
    assert_eq!(
        make_phases(&pipeline),
        vec![("one".into(), ToolPhase::Running)]
    );
    assert_eq!(
        pipeline.current_ai_tool_calls.len(),
        1,
        "Pending/Start不能重复登记tool_use"
    );
    assert!(
        pipeline.pending_tools["one"].started_at.is_some(),
        "Start必须开始独立计时"
    );
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "one".into(),
        name: "Bash".into(),
        output: String::new(),
        is_error: false,
        source_agent_id: None,
    });
    pipeline.handle_event(make_start("one"));
    assert_eq!(
        make_phases(&pipeline),
        vec![("one".into(), ToolPhase::Finished)],
        "迟到Start不能回退已完成工具"
    );
}

#[test]
fn test_completed_concurrent_tools_keep_original_order() {
    let mut pipeline = MessagePipeline::new(".".into());
    for id in ["first", "second"] {
        pipeline.handle_event(make_pending(id, None));
        pipeline.handle_event(make_start(id));
    }
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "second".into(),
        name: "Bash".into(),
        output: "second output".into(),
        is_error: false,
        source_agent_id: None,
    });
    assert_eq!(
        make_phases(&pipeline),
        vec![
            ("first".into(), ToolPhase::Running),
            ("second".into(), ToolPhase::Finished)
        ]
    );
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "first".into(),
        name: "Bash".into(),
        output: "first output".into(),
        is_error: false,
        source_agent_id: None,
    });
    assert_eq!(
        make_phases(&pipeline),
        vec![
            ("first".into(), ToolPhase::Finished),
            ("second".into(), ToolPhase::Finished)
        ]
    );
}

#[test]
fn test_rejected_pending_tool_finishes_without_start_or_history_mutation() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(make_pending("one", None));
    pipeline.mark_tool_awaiting_approval(None, "one");
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "one".into(),
        name: "Bash".into(),
        output: "User rejected".into(),
        is_error: true,
        source_agent_id: None,
    });
    assert_eq!(
        make_phases(&pipeline),
        vec![("one".into(), ToolPhase::Finished)]
    );
    assert!(
        pipeline.completed.is_empty(),
        "排队/审批只影响UI缓存，不应写入BaseMessage历史"
    );
    assert!(
        matches!(&pipeline.build_tail_vms()[0], MessageViewModel::ToolBlock { is_error:true, content, .. } if content == "User rejected")
    );
}

#[test]
fn test_subagent_pending_tools_keep_source_and_live_blocks() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "child".into(),
        instance_id: "child-id".into(),
        task_preview: "task".into(),
        is_background: false,
        prompt: None,
    });
    for index in 0..6 {
        pipeline.handle_event(make_pending(&format!("tool-{index}"), Some("child-id")));
    }
    assert_eq!(
        pipeline.subagent_stack[0].recent_messages.len(),
        6,
        "不能因四条预览上限丢失未完成调用"
    );
    assert!(pipeline.mark_tool_awaiting_approval(Some("child-id"), "tool-2"));
    assert!(
        !pipeline.mark_tool_awaiting_approval(None, "tool-3"),
        "主Agent审批不能串入子Agent"
    );
    assert!(pipeline.subagent_stack[0].recent_messages.iter().all(|vm| matches!(vm, MessageViewModel::ToolBlock { source_agent_id:Some(source), .. } if source == "child-id")));
    for index in 0..6 {
        let mut event = make_start(&format!("tool-{index}"));
        if let AgentEvent::ToolStart {
            source_agent_id, ..
        } = &mut event
        {
            *source_agent_id = Some("child-id".into());
        }
        pipeline.handle_event(event);
    }
    assert_eq!(
        pipeline.subagent_stack[0].recent_messages.len(),
        6,
        "不能丢失运行中的工具"
    );
    assert!(
        pipeline.subagent_stack[0]
            .recent_messages
            .iter()
            .all(|vm| matches!(
                vm,
                MessageViewModel::ToolBlock {
                    phase: ToolPhase::Running,
                    ..
                }
            )),
        "每个工具应独立进入Running"
    );
    for index in 0..6 {
        pipeline.handle_event(AgentEvent::ToolEnd {
            tool_call_id: format!("tool-{index}"),
            name: "Bash".into(),
            output: "done".into(),
            is_error: false,
            source_agent_id: Some("child-id".into()),
        });
        assert_eq!(
            pipeline.subagent_stack[0]
                .recent_messages
                .iter()
                .filter(|vm| matches!(
                    vm,
                    MessageViewModel::ToolBlock {
                        phase: ToolPhase::Running,
                        ..
                    }
                ))
                .count(),
            5 - index,
            "完成时裁剪不能丢失其他活跃工具"
        );
        assert!(
            pipeline.subagent_stack[0]
                .recent_messages
                .iter()
                .filter_map(|vm| match vm {
                    MessageViewModel::ToolBlock {
                        tool_call_id,
                        phase: ToolPhase::Running,
                        ..
                    } => Some(tool_call_id),
                    _ => None,
                })
                .all(|id| id != &format!("tool-{index}")),
            "刚结束工具不应保留Running状态"
        );
    }
    assert_eq!(
        pipeline.subagent_stack[0].recent_messages.len(),
        4,
        "全部结束后恢复四条预览上限"
    );
}

#[test]
fn test_main_and_subagent_concurrent_tools_keep_independent_results() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "child".into(),
        instance_id: "child-id".into(),
        task_preview: "task".into(),
        is_background: false,
        prompt: None,
    });
    pipeline.handle_event(make_pending("same", Some("child-id")));
    pipeline.handle_event(make_pending("same", None));
    pipeline.handle_event(make_start("same"));
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "same".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "sleep 3".into(),
        input: json!({"command":"sleep 3"}),
        source_agent_id: Some("child-id".into()),
    });
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "same".into(),
        name: "Bash".into(),
        output: "main output".into(),
        is_error: false,
        source_agent_id: None,
    });
    assert_eq!(
        make_phases(&pipeline),
        vec![("same".into(), ToolPhase::Finished)]
    );
    assert!(
        matches!(&pipeline.subagent_stack[0].recent_messages[0], MessageViewModel::ToolBlock { phase:ToolPhase::Running, content, .. } if content.is_empty()),
        "主工具结束不能改变子工具"
    );
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "same".into(),
        name: "Bash".into(),
        output: "child output".into(),
        is_error: false,
        source_agent_id: Some("child-id".into()),
    });
    assert!(
        matches!(&pipeline.subagent_stack[0].recent_messages[0], MessageViewModel::ToolBlock { phase:ToolPhase::Finished, content, .. } if content == "child output")
    );
    assert!(
        matches!(&pipeline.build_tail_vms()[0], MessageViewModel::ToolBlock { content, .. } if content == "main output"),
        "子工具结束不能覆盖主工具结果"
    );
}
