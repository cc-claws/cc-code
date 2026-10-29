use super::*;
use crate::ui::message_view::aggregate_batch_groups;
use crate::ui::message_view::ContentBlockView;
use cc_agent::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};
use serde_json::json;

fn _normalize_vms(vms: Vec<MessageViewModel>) -> Vec<String> {
    vms.iter().map(|vm| format!("{:?}", vm)).collect()
}

#[test]
fn test_readonly_action_summary_shell_command_count() {
    assert_eq!(
        super::transform::readonly_action_summary_ext(0, 0, 0, 1, 0, 0).as_deref(),
        Some("ran 1 shell command")
    );
    assert_eq!(
        super::transform::readonly_action_summary_ext(1, 1, 1, 3, 0, 0).as_deref(),
        Some("read 1 file, listed 1 directory, searched for 1 pattern, ran 3 shell commands")
    );
}

#[test]
fn test_tool_end_without_pending_keeps_file_inspect() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_inspect".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: json!({"command": "ls", "file_inspect": true}),
        source_agent_id: None,
    });
    pipeline.pending_tools.remove("tc_inspect");
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_inspect".into(),
        name: "Bash".into(),
        output: "file.txt".into(),
        is_error: false,
        source_agent_id: None,
    });

    assert!(matches!(
        pipeline.build_tail_vms().as_slice(),
        [.., MessageViewModel::ToolCallGroup { tools, .. }]
            if tools.iter().any(|tool| tool.tool_name == "Bash" && tool.file_inspect)
    ));
}

#[test]
fn test_restore_path_file_inspect_matches_streaming() {
    let background = "<background-task-started><task-id>shell-1</task-id><command>ls</command><output>out.log</output></background-task-started>";
    let cases = [
        (json!({"command": "ls", "file_inspect": true}), "files"),
        (json!({"command": "ls", "file_inspect": false}), "files"),
        (json!({"command": "ls"}), "files"),
        (
            json!({"command": "ls", "file_inspect": true, "timeout": 1000}),
            "files",
        ),
        (json!({"command": "ls", "file_inspect": true}), background),
        (
            json!({"command": "ls", "file_inspect": true}),
            "missing file\n[Exit code: 2]",
        ),
    ];
    for (input, output) in cases {
        let messages = vec![
            BaseMessage::ai_from_blocks(vec![ContentBlock::tool_use(
                "inspect",
                "Bash",
                input.clone(),
            )]),
            BaseMessage::tool_result("inspect", output),
        ];
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        pipeline.handle_event(AgentEvent::ToolStart {
            tool_call_id: "inspect".into(),
            name: "Bash".into(),
            display: "Bash".into(),
            args: "ls".into(),
            input: input.clone(),
            source_agent_id: None,
        });
        pipeline.handle_event(AgentEvent::ToolEnd {
            tool_call_id: "inspect".into(),
            name: "Bash".into(),
            output: output.into(),
            is_error: false,
            source_agent_id: None,
        });
        let restored = MessagePipeline::messages_to_view_models(&messages, "/tmp");
        assert_eq!(
            pipeline.build_tail_vms(),
            restored,
            "流式与恢复路径应等价：{input}"
        );
        pipeline.set_completed(messages);
        assert_eq!(
            pipeline.build_tail_vms(),
            restored,
            "快照 reconcile 后形态应保持一致：{input}"
        );
    }
}

#[test]
fn test_restore_path_background_marker_not_aggregated() {
    let messages = vec![
        BaseMessage::ai_from_blocks(vec![ContentBlock::tool_use(
            "inspect",
            "Bash",
            json!({"command": "ls", "file_inspect": true, "run_in_background": true}),
        )]),
        BaseMessage::tool_result(
            "inspect",
            "<background-task-started><task-id>shell-1</task-id><command>ls</command><output>out.log</output></background-task-started>",
        ),
    ];
    let restored = MessagePipeline::messages_to_view_models(&messages, "/tmp");
    assert!(
        matches!(
            restored.as_slice(),
            [MessageViewModel::ToolBlock {
                file_inspect: true,
                ..
            }]
        ),
        "恢复路径没有运行时后台状态，必须通过合法 marker 阻止折叠"
    );
}

#[test]
fn test_file_inspect_snapshot_boundary_matches_history_grouping() {
    let input = json!({"command": "ls", "file_inspect": true});
    let messages = vec![
        BaseMessage::ai_from_blocks(vec![ContentBlock::tool_use("first", "Bash", input.clone())]),
        BaseMessage::tool_result("first", "files"),
        BaseMessage::ai_from_blocks(vec![ContentBlock::tool_use(
            "second",
            "Bash",
            input.clone(),
        )]),
        BaseMessage::tool_result("second", "files"),
    ];
    let mut pipeline = MessagePipeline::new("/tmp".into());
    pipeline.set_completed(messages[..2].to_vec());
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "second".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input,
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "second".into(),
        name: "Bash".into(),
        output: "files".into(),
        is_error: false,
        source_agent_id: None,
    });
    let restored = MessagePipeline::messages_to_view_models(&messages, "/tmp");
    assert_eq!(
        pipeline.build_tail_vms(),
        restored,
        "已完成快照和新增流式工具必须共同聚合，不能拆成两行摘要"
    );
    pipeline.set_completed(messages[2..].to_vec());
    assert_eq!(
        pipeline.build_tail_vms(),
        restored,
        "下一次快照不得改变分组"
    );
}

#[test]
fn test_merge_file_inspect_preserves_reasoning_and_duration() {
    let mut messages = Vec::new();
    for (id, text, duration) in [
        ("first", "先列举文件", 10_000),
        ("second", "再查看内容", 20_000),
    ] {
        messages.push(BaseMessage::ai_from_blocks(vec![
            ContentBlock::reasoning_with_duration(text, duration),
            ContentBlock::tool_use(id, "Bash", json!({"command": "ls", "file_inspect": true})),
        ]));
        messages.push(BaseMessage::tool_result(id, "files"));
    }
    let vms = MessagePipeline::messages_to_view_models(&messages, "/tmp");
    // 一等公民：思考行可能挂在折叠组上
    let reasoning: Vec<_> = vms
        .iter()
        .flat_map(|vm| match vm {
            MessageViewModel::ToolCallGroup {
                thinking: Some(t), ..
            } => vec![(
                t.text.as_str(),
                t.duration_ms,
                t.action_summary.as_deref(),
            )],
            MessageViewModel::AssistantBubble { blocks, .. } => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlockView::Reasoning {
                        text,
                        duration_ms,
                        action_summary,
                        ..
                    } => Some((text.as_str(), *duration_ms, action_summary.as_deref())),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(
        reasoning,
        vec![(
            "先列举文件\n\n再查看内容",
            Some(20_000),
            Some("ran 2 shell commands")
        )],
        "工具聚合不能吞掉后续思考正文；完成态耗时取最后一段（对齐参考实现）"
    );
}

#[test]
fn test_merge_thinking_counts_shell_commands_only_from_groups() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_reasoning("检查文件");
    for (id, timeout, finished) in [
        ("grouped", None, true),
        ("running", None, false),
        ("timed", Some(1000), true),
    ] {
        let mut input = json!({"command": "ls", "file_inspect": true});
        if let Some(timeout) = timeout {
            input["timeout"] = json!(timeout);
        }
        pipeline.handle_event(AgentEvent::ToolStart {
            tool_call_id: id.into(),
            name: "Bash".into(),
            display: "Bash".into(),
            args: "ls".into(),
            input,
            source_agent_id: None,
        });
        if finished {
            pipeline.handle_event(AgentEvent::ToolEnd {
                tool_call_id: id.into(),
                name: "Bash".into(),
                output: "files".into(),
                is_error: false,
                source_agent_id: None,
            });
        }
    }
    let vms = pipeline.build_tail_vms();
    // §13.1：运行中 / 带 timeout 的文件查询 Bash 都入组，因此三条全部计入
    assert_eq!(
        first_action_summary(&vms).as_deref(),
        Some("ran 3 shell commands"),
        "运行中与带 timeout 的 file_inspect Bash 也应计入组内"
    );
    assert!(
        !vms.iter().any(|vm| matches!(vm, MessageViewModel::ToolBlock { .. })),
        "三条都应进组，不应残留单条 ToolBlock"
    );
    let entries = vms
        .iter()
        .find_map(|vm| match vm {
            MessageViewModel::ToolCallGroup { tools, .. } => Some(tools.len()),
            _ => None,
        })
        .expect("应有只读工具组");
    assert_eq!(entries, 3, "组内应包含三条 Bash 条目");
}

#[test]
fn test_file_inspect_background_runtime_preserves_standalone_state() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_reasoning("检查文件");
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "inspect".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: json!({"command": "ls", "file_inspect": true}),
        source_agent_id: None,
    });
    let started_at = std::time::Instant::now();
    pipeline.sync_shell_runtime(None, "inspect", started_at, 120_000, true);
    let running = pipeline.build_tail_vms();
    assert!(
        matches!(
            running.last(),
            Some(MessageViewModel::ToolBlock {
                phase: ToolPhase::Running,
                shell_backgrounded: true,
                file_inspect: true,
                ..
            })
        ),
        "运行时后台状态必须在聚合前生效"
    );
    assert_eq!(first_action_summary(&running), None, "后台调用不得计入摘要");
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "inspect".into(),
        name: "Bash".into(),
        output: "<background-task-started><task-id>shell-1</task-id><command>ls</command><output>out.log</output></background-task-started>".into(),
        is_error: false,
        source_agent_id: None,
    });
    let backgrounded = pipeline.build_tail_vms();
    assert!(
        matches!(
            backgrounded.last(),
            Some(MessageViewModel::ToolBlock {
                phase: ToolPhase::Finished,
                shell_backgrounded: true,
                file_inspect: true,
                ..
            })
        ),
        "后台启动结果到达后也必须保持单条"
    );
    assert_eq!(
        first_action_summary(&backgrounded),
        None,
        "后台启动结果不得计入摘要"
    );
}

/// 测试：流式路径和恢复路径对简单文本回复产生一致的输出
#[test]
fn test_streaming_vs_restore_text_only() {
    let cwd = "/Users/test/project";

    // 恢复路径
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("world")];
    let restore_vms = MessagePipeline::messages_to_view_models(&msgs, cwd);

    // 流式路径：模拟事件序列
    let mut pipeline = MessagePipeline::new(cwd.to_string());
    pipeline.push_chunk("world");
    pipeline.done();
    // 模拟 StateSnapshot 填充 completed
    pipeline.set_completed(vec![BaseMessage::ai("world")]);
    let stream_vms = pipeline.reconcile();

    // 比较非系统消息
    assert_eq!(restore_vms.len(), 2);
    assert_eq!(stream_vms.len(), 1); // 流式路径没有用户消息（由 handle_agent_event 添加）
}

/// 测试：工具调用的 cwd 一致性（核心修复验证）
#[test]
fn test_tool_args_cwd_consistency() {
    let cwd = "/Users/test/project";

    // 模拟恢复路径：Tool 消息从 BaseMessage 转换
    // Ai 消息带文本 + tool_calls，确保不会被过滤
    let msgs = vec![
        BaseMessage::human("read file"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("I'll read the file"),
            vec![ToolCallRequest::new(
                "tc1",
                "Read",
                json!({"file_path": "/Users/test/project/src/main.rs"}),
            )],
        ),
        BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: "tc1".to_string(),
            content: MessageContent::text("file content here"),
            is_error: false,
        },
    ];
    let restore_vms = MessagePipeline::messages_to_view_models(&msgs, cwd);

    // 找到 ToolBlock 或 ToolCallGroup
    let tool_vm = restore_vms.iter().find(|vm| {
        matches!(vm, MessageViewModel::ToolBlock { .. })
            || matches!(vm, MessageViewModel::ToolCallGroup { .. })
    });
    assert!(
        tool_vm.is_some(),
        "应有 ToolBlock/ToolCallGroup，实际 VMs: {:?}",
        restore_vms
    );

    if let Some(MessageViewModel::ToolBlock { args_display, .. }) = tool_vm {
        // 应该显示相对路径而非绝对路径
        assert!(args_display.is_some(), "args_display 应有值");
        let args = args_display.as_ref().unwrap();
        assert!(
            args.contains("src/main.rs"),
            "应显示相对路径，实际: {}",
            args
        );
        assert!(
            !args.contains("/Users/test/project"),
            "不应包含 cwd 前缀，实际: {}",
            args
        );
    }
}

/// 测试：恢复路径的 cwd=None 仍能正常工作（向后兼容）
#[test]
fn test_restore_without_cwd() {
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("hi")];
    // cwd=None → fallback 行为
    let vms = MessagePipeline::messages_to_view_models(&msgs, "");
    assert_eq!(vms.len(), 2);
}

/// 测试：流式 pipeline 的 finalize 清理流式缓冲（completed 由 StateSnapshot 填充）
#[test]
fn test_pipeline_finalize_clears_buffers() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_reasoning("thinking...");
    pipeline.push_chunk("Hello world");
    pipeline.done();

    // finalize 不再 push 到 completed（StateSnapshot 是唯一数据源）
    assert!(pipeline.completed_messages().is_empty());
    // done() 不再清空流式缓冲（set_completed 到达时才清空），
    // 但 current_ai_finalized 被重置为 false，所以流式状态仍然存在
    assert!(pipeline.has_streaming_content());
    // set_completed 到达后才清空流式缓冲
    pipeline.set_completed(vec![
        BaseMessage::human("hi"),
        BaseMessage::ai("Hello world"),
    ]);
    assert!(!pipeline.has_streaming_content());
}

/// 测试：set_completed 是 completed 的唯一数据源
#[test]
fn test_pipeline_set_completed_single_source() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("world")];
    pipeline.set_completed(msgs.clone());

    assert_eq!(pipeline.completed_messages().len(), 2);
}

/// 测试：tool_start/tool_end 不直接写入 completed
#[test]
fn test_pipeline_tool_end_no_duplicate() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "test.txt".into(),
        input: json!({"file_path": "/tmp/test.txt"}),
        source_agent_id: None,
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        output: "content here".into(),
        is_error: false,
        source_agent_id: None,
    });

    // tool_end 不 push 到 completed
    assert!(pipeline.completed_messages().is_empty());

    // 模拟 StateSnapshot 填充
    let snapshot = vec![
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("reading"),
            vec![ToolCallRequest::new(
                "tc1",
                "Read",
                json!({"file_path": "/tmp/test.txt"}),
            )],
        ),
        BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: "tc1".to_string(),
            content: MessageContent::text("content here"),
            is_error: false,
        },
    ];
    pipeline.set_completed(snapshot);
    assert_eq!(
        pipeline.completed_messages().len(),
        2,
        "StateSnapshot 应无重复地填充 completed"
    );
}

/// 测试：from_base_message_with_cwd 与 from_base_message 向后兼容
#[test]
fn test_from_base_message_backward_compat() {
    let msg = BaseMessage::ai("hello");
    let vm1 = MessageViewModel::from_base_message(&msg, &[]);
    let vm2 = MessageViewModel::from_base_message_with_cwd(&msg, &[], None);
    // 两者应产生相同结果
    assert_eq!(format!("{:?}", vm1), format!("{:?}", vm2));
}

// ─── handle_event 测试 ─────────────────────────────────────────────────

/// 测试：handle_event AssistantChunk 更新内部状态并通过策略跟踪积压
#[test]
fn test_handle_event_assistant_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let actions = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
    assert_eq!(pipeline.current_ai_text, "hello");
    assert!(
        pipeline.adaptive_policy.pending_lines > 0,
        "AssistantChunk 应通过策略跟踪积压"
    );
}

/// 测试：handle_event 空 chunk 不产生 AppendChunk
#[test]
fn test_handle_event_empty_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let actions = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: String::new(),
        source_agent_id: None,
    });
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
}

/// 测试：handle_event ToolStart + ToolEnd + Done 更新内部状态（所有返回 None）
#[test]
fn test_handle_event_tool_lifecycle() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // ToolStart → None，但内部状态更新
    let actions = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/src/main.rs"}),
        source_agent_id: None,
    });
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
    assert!(
        pipeline.pending_tools.contains_key("tc1"),
        "ToolStart 后 pending_tools 应包含 tc1"
    );
    // ToolEnd → None，pending_tools 移除
    let actions = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        output: "file content".into(),
        is_error: false,
        source_agent_id: None,
    });
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
    assert!(
        !pipeline.pending_tools.contains_key("tc1"),
        "ToolEnd 后 pending_tools 应不包含 tc1"
    );
    // Done → None
    let actions = pipeline.handle_event(AgentEvent::Done);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
}

#[test]
fn test_bash_tool_start_keeps_event_started_at() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "bash1".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "sleep 10".into(),
        input: json!({"command": "sleep 10"}),
        source_agent_id: None,
    });
    let tail_vms = pipeline.build_tail_vms();
    let Some(MessageViewModel::ToolBlock { started_at, .. }) = tail_vms.last() else {
        panic!("Bash ToolStart 应生成 pending ToolBlock");
    };
    let event_started_at = *started_at;
    assert!(event_started_at.is_some(), "ToolStart 应立即开始执行计时");

    let spawned_at = std::time::Instant::now() - std::time::Duration::from_secs(3);
    pipeline.register_shell_runtime(None, "bash1", spawned_at, 600_000);
    let tail_vms = pipeline.build_tail_vms();
    let Some(MessageViewModel::ToolBlock { started_at, .. }) = tail_vms.last() else {
        panic!("Bash ToolStart 应仍生成 pending ToolBlock");
    };
    assert_eq!(
        *started_at, event_started_at,
        "spawn 注册不能重置 ToolStart 的计时起点"
    );
}

#[test]
fn test_build_tool_start_vm_reads_timeout_from_input() {
    let pipeline = MessagePipeline::new(".".into());
    let vm = pipeline.build_tool_start_vm(
        "bash",
        "Bash",
        &json!({"command":"true", "timeout":300_000}),
        Some(std::time::Instant::now()),
    );
    assert!(
        matches!(
            vm,
            MessageViewModel::ToolBlock {
                phase: crate::ui::message_view::ToolPhase::Running,
                timeout_ms: Some(300_000),
                ..
            }
        ),
        "运行态必须保留模型显式的 timeout"
    );
    let vm = pipeline.build_tool_start_vm(
        "bash",
        "Bash",
        &json!({"command":"true"}),
        Some(std::time::Instant::now()),
    );
    assert!(
        matches!(
            vm,
            MessageViewModel::ToolBlock {
                timeout_ms: None,
                ..
            }
        ),
        "缺省 timeout 不能显示执行器默认值"
    );
}

/// 测试：handle_event StateSnapshot 更新 completed
#[test]
fn test_handle_event_state_snapshot() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("world")];
    let actions = pipeline.handle_event(AgentEvent::StateSnapshot(msgs.clone()));
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
    assert_eq!(pipeline.completed_messages().len(), 2);
}

/// 测试：SubAgent 内部并行相同工具的 tool_call_id 精确匹配
#[test]
fn test_subagent_parallel_same_tool_matches_by_call_id() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 启动 SubAgent
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "test-agent".into(),
        instance_id: "test-instance".into(),
        task_preview: "parallel reads".into(),
        is_background: false,
        prompt: None,
    });

    // SubAgent 内部并行启动两个 read_file（带 source_agent_id）
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_a".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "a.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/a.rs"}),
        source_agent_id: Some("test-instance".into()),
    });
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_b".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "b.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/b.rs"}),
        source_agent_id: Some("test-instance".into()),
    });

    // ToolEnd 按不同顺序到达（tc_b 先完成）
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_b".into(),
        name: "Read".into(),
        output: "content of b".into(),
        is_error: false,
        source_agent_id: Some("test-instance".into()),
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_a".into(),
        name: "Read".into(),
        output: "content of a".into(),
        is_error: false,
        source_agent_id: Some("test-instance".into()),
    });

    // 验证 recent_messages 中两个 ToolBlock 被正确更新
    let sub = pipeline.subagent_stack.last().unwrap();
    assert_eq!(sub.recent_messages.len(), 2);

    // 找到 tc_a 和 tc_b 对应的 ToolBlock
    let mut found_a = false;
    let mut found_b = false;
    for vm in &sub.recent_messages {
        if let MessageViewModel::ToolBlock {
            tool_call_id,
            content,
            ..
        } = vm
        {
            match tool_call_id.as_str() {
                "tc_a" => {
                    assert_eq!(content, "content of a", "tc_a 应匹配自己的结果");
                    found_a = true;
                }
                "tc_b" => {
                    assert_eq!(content, "content of b", "tc_b 应匹配自己的结果");
                    found_b = true;
                }
                _ => {}
            }
        }
    }
    assert!(found_a, "应找到 tc_a 的 ToolBlock");
    assert!(found_b, "应找到 tc_b 的 ToolBlock");
}

// ─── 并发 SubAgent 路由测试 ──────────────────────────────────────────────

/// 测试：并发 SubAgent 通过 source_agent_id 精确路由工具调用事件
/// 验证 ToolStart/ToolEnd/AssistantChunk 事件路由到正确的 SubAgentGroup
#[test]
fn test_concurrent_subagents_route_by_source_agent_id() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 启动两个并发 SubAgent
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "agent-a".into(),
        instance_id: "inst-a".into(),
        task_preview: "task a".into(),
        is_background: false,
        prompt: None,
    });
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "agent-b".into(),
        instance_id: "inst-b".into(),
        task_preview: "task b".into(),
        is_background: false,
        prompt: None,
    });

    // Agent A 的 ToolStart → source_agent_id = instance_id
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_a1".into(),
        name: "Read".into(),
        display: "Read".into(),
        args: "a.rs".into(),
        input: json!({"file_path": "/tmp/a.rs"}),
        source_agent_id: Some("inst-a".into()),
    });
    // Agent B 的 ToolStart → source_agent_id = instance_id
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_b1".into(),
        name: "Grep".into(),
        display: "Grep".into(),
        args: "pattern".into(),
        input: json!({"pattern": "fn main"}),
        source_agent_id: Some("inst-b".into()),
    });

    // 验证：agent-a 的 recent_messages 有 Read，agent-b 有 Grep
    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.agent_id == "agent-a")
        .unwrap();
    assert_eq!(sub_a.recent_messages.len(), 1);
    if let MessageViewModel::ToolBlock { tool_name, .. } = &sub_a.recent_messages[0] {
        assert_eq!(tool_name, "Read", "agent-a 应包含 Read 工具调用");
    } else {
        panic!("agent-a 的 recent_messages[0] 应为 ToolBlock");
    }

    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.agent_id == "agent-b")
        .unwrap();
    assert_eq!(sub_b.recent_messages.len(), 1);
    if let MessageViewModel::ToolBlock { tool_name, .. } = &sub_b.recent_messages[0] {
        assert_eq!(tool_name, "Grep", "agent-b 应包含 Grep 工具调用");
    } else {
        panic!("agent-b 的 recent_messages[0] 应为 ToolBlock");
    }

    // Agent A 的 ToolEnd
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_a1".into(),
        name: "Read".into(),
        output: "content of a".into(),
        is_error: false,
        source_agent_id: Some("inst-a".into()),
    });
    // Agent B 的 ToolEnd
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_b1".into(),
        name: "Grep".into(),
        output: "match in b".into(),
        is_error: false,
        source_agent_id: Some("inst-b".into()),
    });

    // 验证 ToolEnd 结果路由到正确的 SubAgent
    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.agent_id == "agent-a")
        .unwrap();
    if let MessageViewModel::ToolBlock {
        tool_call_id,
        content,
        ..
    } = &sub_a.recent_messages[0]
    {
        assert_eq!(tool_call_id, "tc_a1");
        assert_eq!(
            content, "content of a",
            "agent-a 的 ToolEnd 应路由到 agent-a"
        );
    }

    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.agent_id == "agent-b")
        .unwrap();
    if let MessageViewModel::ToolBlock {
        tool_call_id,
        content,
        ..
    } = &sub_b.recent_messages[0]
    {
        assert_eq!(tool_call_id, "tc_b1");
        assert_eq!(content, "match in b", "agent-b 的 ToolEnd 应路由到 agent-b");
    }

    // AssistantChunk 也应精确路由
    let _ = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "chunk for a".into(),
        source_agent_id: Some("inst-a".into()),
    });
    let _ = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "chunk for b".into(),
        source_agent_id: Some("inst-b".into()),
    });

    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "inst-a")
        .unwrap();
    // agent-a 有 ToolBlock + AssistantBubble = 2 个
    assert_eq!(sub_a.recent_messages.len(), 2);

    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "inst-b")
        .unwrap();
    assert_eq!(sub_b.recent_messages.len(), 2);

    // SubAgentEnd 带 instance_id 精确匹配
    let _ = pipeline.handle_event(AgentEvent::SubAgentEnd {
        agent_id: Some("agent-a".into()),
        instance_id: Some("inst-a".into()),
        result: "done a".into(),
        is_error: false,
    });
    let _ = pipeline.handle_event(AgentEvent::SubAgentEnd {
        agent_id: Some("agent-b".into()),
        instance_id: Some("inst-b".into()),
        result: "done b".into(),
        is_error: false,
    });

    // 验证两个 SubAgent 都被冻结
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        2,
        "两个 SubAgent 都应被冻结"
    );
}

/// 测试：无来源信封的主工具不能误投正在运行的子 Agent。
#[test]
fn test_main_tool_without_source_stays_out_of_subagent() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 启动子 Agent 后，主 Agent 仍可能有并发工具。
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "legacy-agent".into(),
        instance_id: "test-instance".into(),
        task_preview: "legacy task".into(),
        is_background: false,
        prompt: None,
    });
    // ToolStart 无 source_agent_id 表示主管线，不能根据 last_mut() 猜来源。
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_legacy".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: json!({"command": "ls"}),
        source_agent_id: None,
    });

    let sub = pipeline.subagent_stack.last().unwrap();
    assert_eq!(sub.recent_messages.len(), 0, "主工具不能投递到子 Agent");
    assert!(pipeline.pending_tools.contains_key("tc_legacy"));
}

// ─── build_tail_vms 测试 ──────────────────────────────────────────────────

/// 场景1: has_snapshot=true, completed 有消息 → 从最后一条 Human 开始 reconcile
#[test]
fn test_build_tail_vms_with_snapshot() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.completed = vec![
        BaseMessage::human("q1"),
        BaseMessage::ai("a1"),
        BaseMessage::human("q2"),
        BaseMessage::ai("a2"),
    ];
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 2;

    let tail_vms = pipeline.build_tail_vms();
    let expected =
        MessagePipeline::messages_to_view_models(&pipeline.completed[2..], &pipeline.cwd);
    assert_eq!(format!("{:?}", tail_vms), format!("{:?}", expected));
}

/// 场景2: has_snapshot=false（Case 1）→ 跳过 reconcile，只输出 streaming + pending tools
#[test]
fn test_build_tail_vms_case1_no_snapshot() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.completed = vec![BaseMessage::human("old q"), BaseMessage::ai("old a")];
    pipeline.has_snapshot_this_round = false;
    pipeline.completed_len_at_round_start = 2;

    // 有流式内容
    pipeline.push_chunk("streaming text");

    let tail_vms = pipeline.build_tail_vms();
    // Case 1 不应包含 old q / old a
    assert!(
        tail_vms.iter().all(
            |vm| !matches!(vm, MessageViewModel::UserBubble { content, .. } if content == "old q")
        ),
        "Case 1 不应包含上一轮消息"
    );
    // 应包含 streaming bubble
    assert!(
        tail_vms.iter().any(|vm| matches!(
            vm,
            MessageViewModel::AssistantBubble {
                is_streaming: true,
                ..
            }
        )),
        "Case 1 应包含 streaming bubble"
    );
}

/// 场景3: 空 completed + 无 streaming → 空 tail
#[test]
fn test_build_tail_vms_empty() {
    let pipeline = MessagePipeline::new("/tmp".to_string());
    let tail_vms = pipeline.build_tail_vms();
    assert!(tail_vms.is_empty());
}

/// 场景：AssistantChunk → ToolStart 后，build_tail_vms 应包含 AI 文本 + ToolBlock
#[test]
fn test_build_tail_vms_text_visible_with_pending_tool() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 模拟真实事件流：AI 先输出文本，再调用工具
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "I'll read the file".into(),
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: json!({"file_path": "/tmp/src/main.rs"}),
        source_agent_id: None,
    });

    let tail_vms = pipeline.build_tail_vms();

    // 应包含 streaming bubble 且有文本内容
    let has_text = tail_vms.iter().any(|vm| {
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            blocks.iter().any(
                |b| matches!(b, ContentBlockView::Text { raw, .. } if raw.contains("I'll read")),
            )
        } else {
            false
        }
    });
    assert!(
        has_text,
        "ToolStart 后 streaming bubble 应包含 AI 文本，实际 VMs: {:?}",
        tail_vms
    );

    // Read 工具被 aggregate_tool_groups 折叠为 ToolCallGroup
    let has_tool = tail_vms.iter().any(|vm| {
            matches!(
                vm,
                MessageViewModel::ToolCallGroup { tools, .. } if tools.iter().any(|t| t.tool_name == "Read")
            )
        });
    assert!(
        has_tool,
        "ToolStart 后应有 ToolCallGroup(Read)，实际 VMs: {:?}",
        tail_vms
    );
}

/// 端到端：多轮工具调用中 AI 文本可见性
/// Chunk → ToolStart → ToolEnd → StateSnapshot → Chunk → ToolStart → Done
#[test]
fn test_e2e_text_visible_between_tool_calls() {
    use cc_agent::messages::{MessageContent, MessageId, ToolCallRequest};

    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.begin_round();

    // 1. AI 输出文本
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "Let me check the file".into(),
        source_agent_id: None,
    });
    let tail1 = pipeline.build_tail_vms();
    assert!(has_text(&tail1, "Let me check"), "步骤1: chunk 后应有文本");

    // 2. ToolStart
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "main.rs".into(),
        input: json!({"path": "/tmp/main.rs"}),
        source_agent_id: None,
    });
    let tail2 = pipeline.build_tail_vms();
    assert!(
        has_text(&tail2, "Let me check"),
        "步骤2: ToolStart 后文本应保留"
    );

    // 3. ToolEnd
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        output: "fn main() {}".into(),
        is_error: false,
        source_agent_id: None,
    });
    let tail3 = pipeline.build_tail_vms();
    assert!(
        has_text(&tail3, "Let me check"),
        "步骤3: ToolEnd 后文本应保留"
    );

    // 4. StateSnapshot（清空流式缓冲，切换到 reconcile 路径）
    pipeline.set_completed(vec![
        BaseMessage::human("read file"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("Let me check the file"),
            vec![ToolCallRequest::new(
                "tc1",
                "Read",
                json!({"path": "/tmp/main.rs"}),
            )],
        ),
        BaseMessage::Tool {
            id: MessageId::new(),
            tool_call_id: "tc1".to_string(),
            content: MessageContent::text("fn main() {}"),
            is_error: false,
        },
    ]);
    let tail4 = pipeline.build_tail_vms();
    assert!(
        has_text(&tail4, "Let me check"),
        "步骤4: StateSnapshot 后 reconcile 应包含文本, VMs: {:?}",
        tail4
    );

    // 5. 新的 AI 文本（工具之间）
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "Now let me write tests".into(),
        source_agent_id: None,
    });
    let tail5 = pipeline.build_tail_vms();
    assert!(
        has_text(&tail5, "Now let me write tests"),
        "步骤5: 新 chunk 后应有新文本"
    );
    assert!(
        has_text(&tail5, "Let me check"),
        "步骤5: 旧文本也应保留（reconcile）"
    );

    // 6. 第二个 ToolStart
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc2".into(),
        name: "Write".into(),
        display: "WriteFile".into(),
        args: "test.rs".into(),
        input: json!({"path": "/tmp/test.rs"}),
        source_agent_id: None,
    });
    let tail6 = pipeline.build_tail_vms();
    assert!(
        has_text(&tail6, "Now let me write tests"),
        "步骤6: 第二个 ToolStart 后新文本应保留"
    );
    assert!(
        has_text(&tail6, "Let me check"),
        "步骤6: 旧文本也应保留（reconcile）"
    );
}

fn has_text(vms: &[MessageViewModel], text: &str) -> bool {
    vms.iter().any(|vm| {
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            blocks
                .iter()
                .any(|b| matches!(b, ContentBlockView::Text { raw, .. } if raw.contains(text)))
        } else {
            false
        }
    })
}

/// 验证尾部重建与全量转换一致性
#[test]
fn test_build_tail_vms_consistency() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.restore_completed(vec![
        BaseMessage::human("q1"),
        BaseMessage::ai("a1"),
        BaseMessage::human("q2"),
        BaseMessage::ai("a2"),
    ]);
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 2;

    let tail_vms = pipeline.build_tail_vms();

    // 尾部重建应从本轮起点开始，上一轮消息属于不变前缀。
    let expected_tail = MessagePipeline::messages_to_view_models(
        &pipeline.completed_messages()[pipeline.completed_len_at_round_start..],
        &pipeline.cwd,
    );

    assert_eq!(format!("{:?}", tail_vms), format!("{:?}", expected_tail));
}

/// 验证工具调用场景的尾部重建
#[test]
fn test_build_tail_vms_with_tools() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.restore_completed(vec![
        BaseMessage::human("read file"),
        BaseMessage::ai_from_blocks(vec![ContentBlock::ToolUse {
            id: "tc1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        }]),
        BaseMessage::tool_result("tc1", "file content here"),
    ]);
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 0;

    let tail_vms = pipeline.build_tail_vms();

    // 全量转换对比
    let full_vms =
        MessagePipeline::messages_to_view_models(pipeline.completed_messages(), &pipeline.cwd);

    assert_eq!(format!("{:?}", tail_vms), format!("{:?}", full_vms));
}

/// 验证 pending tools 在 build_tail_vms 中生成 ToolBlock VMs
#[test]
fn test_build_tail_vms_with_pending_tools() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 0;
    pipeline.completed = vec![BaseMessage::human("hi")];

    // 模拟 ToolStart（通过 handle_event）
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        source_agent_id: None,
    });

    let tail_vms = pipeline.build_tail_vms();
    // 应包含 UserBubble + pending ToolBlock（Read 可能被聚合为 ToolCallGroup）
    let has_tool = tail_vms.iter().any(|vm| match vm {
        MessageViewModel::ToolBlock { tool_name, .. } => tool_name == "Read",
        MessageViewModel::ToolCallGroup { tools, .. } => {
            tools.iter().any(|t| t.tool_name == "Read")
        }
        _ => false,
    });
    assert!(
        has_tool,
        "build_tail_vms 应包含 pending tool 的 ToolBlock 或 ToolCallGroup"
    );
}

/// 验证 set_completed 清除 pending_tools
#[test]
fn test_set_completed_clears_pending_tools() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        source_agent_id: None,
    });
    assert!(pipeline.pending_tools.contains_key("tc1"));

    pipeline.set_completed(vec![BaseMessage::human("hi"), BaseMessage::ai("result")]);
    assert!(
        !pipeline.pending_tools.contains_key("tc1"),
        "set_completed 应清除 pending_tools"
    );
    assert!(pipeline.has_snapshot_this_round);
}

/// 验证 Interrupted 后 build_tail_vms 产生一致结果（可用于后续 RebuildAll）
///
/// 场景：agent 回复了文本后被中断，Interrupted 处理器调用 build_rebuild_all
/// 然后 Done 到达，如果重复 build_rebuild_all 并 RebuildAll，会覆盖 Interrupted 添加的通知消息。
#[test]
fn test_build_tail_vms_interrupted_then_done_consistency() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 0;

    // 模拟流式：用户发送消息，agent 回复了文本，然后开始工具调用
    pipeline.push_chunk("I'll read the file");
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        source_agent_id: None,
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        output: "file content here".into(),
        is_error: false,
        source_agent_id: None,
    });

    // 模拟 StateSnapshot 填充 completed
    pipeline.set_completed(vec![
        BaseMessage::human("read file"),
        BaseMessage::ai_from_blocks(vec![
            ContentBlock::text("I'll read the file"),
            ContentBlock::tool_use("tc1", "Read", json!({"file_path": "/tmp/test.rs"})),
        ]),
        BaseMessage::tool_result("tc1", "file content here"),
    ]);

    // Interrupted 处理器调用 build_rebuild_all
    let action1 = pipeline.build_rebuild_all(0);
    if let PipelineAction::RebuildAll {
        prefix_len,
        tail_vms,
    } = action1
    {
        assert_eq!(prefix_len, 0);
        assert!(
            tail_vms.len() >= 3,
            "build_tail_vms 应包含 UserBubble + AssistantBubble + ToolBlock/Group"
        );

        // Done 到达时，再次 build_rebuild_all 应产生相同结果
        let action2 = pipeline.build_rebuild_all(0);
        if let PipelineAction::RebuildAll {
            prefix_len: p2,
            tail_vms: tail_vms2,
        } = action2
        {
            assert_eq!(prefix_len, p2);
            assert_eq!(tail_vms.len(), tail_vms2.len());
            for (a, b) in tail_vms.iter().zip(tail_vms2.iter()) {
                assert_eq!(a, b, "两次 build_rebuild_all 结果应一致");
            }
        } else {
            panic!("Expected RebuildAll");
        }
    } else {
        panic!("Expected RebuildAll");
    }
}

/// 验证 Done 后 pipeline.done() 是幂等的（不改变 build_tail_vms 结果）
#[test]
fn test_done_idempotent_build_tail_vms() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 0;

    pipeline.push_chunk("Hello world");
    pipeline.set_completed(vec![
        BaseMessage::human("hi"),
        BaseMessage::ai("Hello world"),
    ]);

    // 第一次 done
    pipeline.done();
    let action1 = pipeline.build_rebuild_all(0);
    let tail_vms1 = match action1 {
        PipelineAction::RebuildAll { tail_vms, .. } => tail_vms,
        _ => panic!("Expected RebuildAll"),
    };

    // 第二次 done（模拟 Interrupted -> Done 双重调用）
    pipeline.done();
    let action2 = pipeline.build_rebuild_all(0);
    let tail_vms2 = match action2 {
        PipelineAction::RebuildAll { tail_vms, .. } => tail_vms,
        _ => panic!("Expected RebuildAll"),
    };

    assert_eq!(tail_vms1.len(), tail_vms2.len());
    for (a, b) in tail_vms1.iter().zip(tail_vms2.iter()) {
        assert_eq!(a, b, "多次 done 后 build_tail_vms 结果应一致");
    }
}

// ── 批次聚合集成测试 ──

/// 创建一个包含 N 个 Agent ToolUse/ToolResult 对的 BaseMessage 历史
fn make_agent_history(agent_count: usize, separated: bool) -> Vec<BaseMessage> {
    use cc_agent::messages::MessageId;
    let mut tool_calls: Vec<ToolCallRequest> = Vec::new();
    for i in 0..agent_count {
        tool_calls.push(ToolCallRequest::new(
            format!("tc_{}", i),
            "Agent",
            json!({"subagent_type": format!("agent-{}", i), "prompt": format!("task {}", i)}),
        ));
    }
    let mut msgs = vec![
        BaseMessage::human("dispatch agents"),
        BaseMessage::ai_with_tool_calls(MessageContent::text(""), tool_calls.clone()),
    ];
    for i in 0..agent_count {
        msgs.push(BaseMessage::Tool {
            id: MessageId::new(),
            tool_call_id: format!("tc_{}", i),
            content: MessageContent::text(format!(
                "[Sub-agent executed {} tool calls: done{}]",
                i + 1,
                i
            )),
            is_error: false,
        });
    }
    // 在两组 agent 之间插入非 Agent 消息（测试批次分离）
    if separated {
        msgs.push(BaseMessage::human("continue"));
        let tool_calls2 = vec![ToolCallRequest::new(
            "tc_sep".to_string(),
            "Agent",
            json!({"subagent_type": "agent-sep", "prompt": "separated task"}),
        )];
        msgs.push(BaseMessage::ai_with_tool_calls(
            MessageContent::text(""),
            tool_calls2,
        ));
        msgs.push(BaseMessage::Tool {
            id: MessageId::new(),
            tool_call_id: "tc_sep".to_string(),
            content: MessageContent::text("[Sub-agent executed 2 tool calls: done_sep]"),
            is_error: false,
        });
    }
    msgs
}

#[test]
fn test_batch_info_single_agent_no_batch() {
    let msgs = make_agent_history(1, false);
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    // 单个 agent 不应聚合
    let subagent_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { .. }))
        .collect();
    assert_eq!(subagent_vms.len(), 1, "单个 agent 应保持独立");
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = subagent_vms[0] {
        assert!(batch_agents.is_empty(), "单个 agent 的 batch_agents 应为空");
    }
}

#[test]
fn test_batch_info_multi_agent_triggers() {
    let msgs = make_agent_history(3, false);
    let mut vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    aggregate_batch_groups(&mut vms);
    // 3 个连续 Agent Tool 结果应聚合为 1 个批次
    let batch_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { batch_agents, .. } if !batch_agents.is_empty()))
        .collect();
    assert_eq!(batch_vms.len(), 1, "3 个连续 agent 完成后应聚合为 1 个批次");
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = batch_vms[0] {
        assert_eq!(batch_agents.len(), 3);
    }
}

#[test]
fn test_batch_info_reset_on_done() {
    // 模拟 Done 后新起一批 agent
    let mut msgs = make_agent_history(2, false);
    // 模拟 Done（新的 Ai 回复分隔两批）
    msgs.push(BaseMessage::ai("I'll continue"));
    let more = make_agent_history(1, false);
    // 跳过 human 和 ai 消息，只追加 Tool 消息
    msgs.extend(more.iter().skip(2).cloned());
    let mut vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    aggregate_batch_groups(&mut vms);
    // Ai 消息分隔了两批 agent，不应跨批次聚合
    let batch_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { batch_agents, .. } if !batch_agents.is_empty()))
        .collect();
    // 第一批 2 个 agent 聚合为 1 个批次，第二批 1 个 agent 保持独立
    assert_eq!(
        batch_vms.len(),
        1,
        "Done 后新起的 agent 不应与前面合并，只有第一批聚合"
    );
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = batch_vms[0] {
        assert_eq!(batch_agents.len(), 2, "第一批应聚合 2 个 agent");
    }
}

#[test]
fn test_batch_info_reset_on_non_agent_tool() {
    // 非 Agent 工具打断连续的 Agent Tool 结果
    use cc_agent::messages::MessageId;
    let mut msgs = vec![
        BaseMessage::human("go"),
        BaseMessage::ai_with_tool_calls(
            MessageContent::text(""),
            vec![
                ToolCallRequest::new(
                    "tc1",
                    "Agent",
                    json!({"subagent_type": "a1", "prompt": "t1"}),
                ),
                ToolCallRequest::new("tc_bash", "Bash", json!({"command": "ls"})),
                ToolCallRequest::new(
                    "tc2",
                    "Agent",
                    json!({"subagent_type": "a2", "prompt": "t2"}),
                ),
            ],
        ),
    ];
    // Agent Tool 结果
    msgs.push(BaseMessage::Tool {
        id: MessageId::new(),
        tool_call_id: "tc1".to_string(),
        content: MessageContent::text("[Sub-agent executed 3 tool calls: done1]"),
        is_error: false,
    });
    // Bash Tool 结果（打断连续区间）
    msgs.push(BaseMessage::Tool {
        id: MessageId::new(),
        tool_call_id: "tc_bash".to_string(),
        content: MessageContent::text("file1.txt"),
        is_error: false,
    });
    // Agent Tool 结果
    msgs.push(BaseMessage::Tool {
        id: MessageId::new(),
        tool_call_id: "tc2".to_string(),
        content: MessageContent::text("[Sub-agent executed 5 tool calls: done2]"),
        is_error: false,
    });
    let mut vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    aggregate_batch_groups(&mut vms);
    let batch_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { batch_agents, .. } if !batch_agents.is_empty()))
        .collect();
    // Bash Tool 结果打断了连续区间，两个 Agent 各自独立
    assert_eq!(batch_vms.len(), 0, "非 Agent 工具打断后 agent 不应聚合");
}

#[test]
fn test_batch_info_different_batches_no_merge() {
    // 两组 agent 被用户消息分隔
    let msgs = make_agent_history(2, true);
    let mut vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    aggregate_batch_groups(&mut vms);
    let batch_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { batch_agents, .. } if !batch_agents.is_empty()))
        .collect();
    // 第一批 2 个 agent 聚合为 1 个批次，第二批 1 个 agent 保持独立
    assert_eq!(batch_vms.len(), 1, "用户消息分隔后只有第一批聚合");
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = batch_vms[0] {
        assert_eq!(batch_agents.len(), 2, "第一批应聚合 2 个 agent");
    }
}

#[test]
fn test_build_tail_vms_aggregate_after_all_done() {
    // 通过 reconcile 路径验证聚合（reconcile 内部调用 messages_to_view_models）
    let msgs = make_agent_history(3, false);
    let mut vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    aggregate_batch_groups(&mut vms);
    let batch_vms: Vec<_> = vms
        .iter()
        .filter(|vm| matches!(vm, MessageViewModel::SubAgentGroup { batch_agents, .. } if !batch_agents.is_empty()))
        .collect();
    assert_eq!(batch_vms.len(), 1, "reconcile 路径应触发聚合");
}

/// frozen_subagent_vms 跨轮次累积：begin_round() 应清空上一轮冻结的 VMs，
/// 防止 merge_frozen_subagents 按位置错误匹配到旧轮次的数据。
#[test]
fn test_frozen_subagent_vms_cleared_on_begin_round() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.has_snapshot_this_round = true;
    pipeline.completed_len_at_round_start = 0;

    // ── 轮次 1：并发 2 个 SubAgent ──
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "sa1".into(),
        instance_id: "test-instance".into(),
        task_preview: "task one".into(),
        is_background: false,
        prompt: None,
    });
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "sa2".into(),
        instance_id: "test-instance".into(),
        task_preview: "task two".into(),
        is_background: false,
        prompt: None,
    });
    pipeline.handle_event(AgentEvent::SubAgentEnd {
        result: "result sa1".into(),
        is_error: false,
        agent_id: Some("sa1".into()),
        instance_id: None,
    });
    pipeline.handle_event(AgentEvent::SubAgentEnd {
        result: "result sa2".into(),
        is_error: false,
        agent_id: Some("sa2".into()),
        instance_id: None,
    });

    // 验证轮次 1 的 frozen_subagent_vms 包含 2 个冻结 VM
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        2,
        "轮次 1 结束后应有 2 个 frozen VM"
    );

    pipeline.done();

    // done() 不应清空 frozen_subagent_vms（它们会被 build_tail_vms 消费）
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        2,
        "done() 后 frozen VMs 仍在（等待 build_tail_vms 消费）"
    );

    // ── 轮次 2 开始 ──
    pipeline.begin_round();

    // begin_round() 应清空上一轮的 frozen_subagent_vms
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        0,
        "begin_round() 后 frozen_subagent_vms 应被清空"
    );

    // ── 轮次 2：单个 SubAgent sa3 ──
    pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "sa3".into(),
        instance_id: "test-instance".into(),
        task_preview: "task three".into(),
        is_background: false,
        prompt: None,
    });
    pipeline.handle_event(AgentEvent::SubAgentEnd {
        result: "result sa3".into(),
        is_error: false,
        agent_id: None,
        instance_id: None,
    });

    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        1,
        "轮次 2 应只有 1 个 frozen VM（sa3）"
    );

    // 验证 sa3 的 frozen VM 内容正确（不是 sa1/sa2 的数据）
    if let MessageViewModel::SubAgentGroup { agent_id, .. } = &pipeline.frozen_subagent_vms[0] {
        assert_eq!(agent_id, "sa3", "frozen VM 应属于 sa3，而非 sa1/sa2");
    } else {
        panic!("frozen_subagent_vms[0] 应为 SubAgentGroup");
    }
}

/// merge_frozen_subagents 在 frozen_vms 为空时不应修改 new_vms。
#[test]
fn test_merge_frozen_subagents_empty_is_noop() {
    let mut new_vms = vec![{
        let mut vm = MessageViewModel::SubAgentGroup {
            agent_id: "sa1".into(),
            task_preview: "task".into(),
            total_steps: 3,
            recent_messages: Vec::new(),
            is_running: false,
            collapsed: false,
            final_result: Some("result".into()),
            is_error: false,
            is_background: false,
            bg_hash: Some("abc123".to_string()),
            batch_agents: Vec::new(),
            instance_id: None,
            content_hash: 0,
        };
        vm.recompute_hash();
        vm
    }];
    let original = new_vms.clone();
    merge_frozen_subagents(&[], &mut new_vms);
    assert_eq!(new_vms, original, "空 frozen_vms 不应修改 new_vms");
}

/// 回归测试：ToolStart(Agent) + SubAgentStart 不应创建重复 SubAgentState
///
/// 修复前：ToolStart 调用 tool_start_internal 创建 SubAgentState #1，
/// SubAgentStart 再次调用 tool_start_internal 创建 SubAgentState #2。
/// 导致 build_tail_vms 输出两个 SubAgentGroup（一个 frozen + 一个 running），
/// 造成显示闪烁和聚合异常。
#[test]
fn test_no_duplicate_subagent_state_on_tool_start_plus_subagent_start() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 1. ToolStart(Agent) — 父 Agent 调用 Agent 工具
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "call_abc123".into(),
        name: "Agent".into(),
        display: "Agent".into(),
        args: "".into(),
        input: json!({"subagent_type": "code-reviewer", "prompt": "review code"}),
        source_agent_id: None,
    });

    // ToolStart 不应创建 SubAgentState，只注册 pending_tool
    assert_eq!(
        pipeline.subagent_stack.len(),
        0,
        "ToolStart(Agent) 不应创建 SubAgentState"
    );
    assert!(
        pipeline.pending_tools.contains_key("call_abc123"),
        "ToolStart 应注册 pending_tool"
    );

    // 2. SubAgentStart — 创建 SubAgentState
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "code-reviewer".into(),
        instance_id: "test-instance".into(),
        task_preview: "review code".into(),
        is_background: false,
        prompt: None,
    });

    // SubAgentStart 创建唯一一个 SubAgentState
    assert_eq!(
        pipeline.subagent_stack.len(),
        1,
        "SubAgentStart 应创建恰好一个 SubAgentState"
    );
    assert_eq!(pipeline.subagent_stack[0].agent_id, "code-reviewer");

    // 3. SubAgentEnd — 冻结
    let _ = pipeline.handle_event(AgentEvent::SubAgentEnd {
        agent_id: Some("code-reviewer".into()),
        instance_id: None,
        result: "review done".into(),
        is_error: false,
    });

    // 只有一个 frozen VM
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        1,
        "应恰好冻结一个 SubAgentGroup"
    );
    assert!(
        !pipeline.subagent_stack[0].is_running,
        "SubAgentState 应标记为非运行中"
    );

    // 4. ToolEnd — 清理 pending_tool，不产生额外副作用
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "call_abc123".into(),
        name: "Agent".into(),
        output: "review done".into(),
        is_error: false,
        source_agent_id: None,
    });

    assert!(
        !pipeline.pending_tools.contains_key("call_abc123"),
        "ToolEnd 应清理 pending_tool"
    );
    // frozen_count 不应增加
    assert_eq!(
        pipeline.frozen_subagent_vms_count(),
        1,
        "ToolEnd 不应产生额外的 frozen VM"
    );
}

// ─── diff_input 集成测试 ──────────────────────────────────────────────────

/// 测试：Edit 工具的 ToolEnd 产生 diff_input（old_content/new_content/file_path）
#[test]
fn test_edit_tool_end_produces_diff_input() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // Arrange：发送 AssistantChunk + ToolStart + ToolEnd
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "I'll edit the file".into(),
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_edit1".into(),
        name: "Edit".into(),
        display: "Edit".into(),
        args: "src/main.rs".into(),
        input: json!({
            "file_path": "/tmp/src/main.rs",
            "old_string": "fn old() {}",
            "new_string": "fn new() {}"
        }),
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_edit1".into(),
        name: "Edit".into(),
        output: "Replaced text".into(),
        is_error: false,
        source_agent_id: None,
    });

    // Act：build_tail_vms 从 completed_tools 构建 ToolBlock
    let tail_vms = pipeline.build_tail_vms();

    // Assert：找到 Edit ToolBlock，验证 diff_input 有值且字段正确
    let edit_block = tail_vms.iter().find(
        |vm| matches!(vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "Edit"),
    );
    assert!(
        edit_block.is_some(),
        "应包含 Edit ToolBlock，实际 VMs: {:?}",
        tail_vms
    );

    if let MessageViewModel::ToolBlock {
        diff_input,
        args_display,
        ..
    } = edit_block.unwrap()
    {
        let input = diff_input
            .as_ref()
            .expect("Edit 工具应产生 diff_input（供渲染时按 width 计算）");
        assert_eq!(input.file_path, "/tmp/src/main.rs");
        assert_eq!(input.old_content, "fn old() {}");
        assert_eq!(input.new_content, "fn new() {}");
        assert!(!input.is_new_file);
        // 文件路径在 args_display 中（header 摘要），实际 diff 渲染时再去掉标题行避免重复
        let args = args_display.as_ref().expect("Edit 工具应有 args_display");
        assert!(
            args.contains("src/main.rs"),
            "args_display 应包含文件路径，实际: {}",
            args
        );
    }
}

/// 测试：非 Edit/Write 工具不产生 diff_input
#[test]
fn test_non_edit_tool_no_diff_input() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // Arrange：发送 Bash 工具的 ToolStart + ToolEnd
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_bash1".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "cargo build -p cc-tui".into(),
        input: json!({"command": "cargo build -p cc-tui"}),
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_bash1".into(),
        name: "Bash".into(),
        output: "file1.txt\nfile2.txt".into(),
        is_error: false,
        source_agent_id: None,
    });

    // Act
    let tail_vms = pipeline.build_tail_vms();

    // Assert：Bash ToolBlock 的 diff_input 应为 None
    let bash_block = tail_vms.iter().find(
        |vm| matches!(vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "Bash"),
    );
    assert!(bash_block.is_some(), "应包含 Bash ToolBlock");

    if let MessageViewModel::ToolBlock { diff_input, .. } = bash_block.unwrap() {
        assert!(
            diff_input.is_none(),
            "Bash 工具不应产生 diff_input，实际: {:?}",
            diff_input
        );
    }
}

/// 测试：is_error=true 的 ToolEnd 不产生 diff_input（即使工具名是 Edit）
#[test]
fn test_error_tool_end_no_diff_input() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // Arrange：Edit 工具执行失败
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_err1".into(),
        name: "Edit".into(),
        display: "Edit".into(),
        args: "missing.rs".into(),
        input: json!({
            "file_path": "/tmp/missing.rs",
            "old_string": "old code",
            "new_string": "new code"
        }),
        source_agent_id: None,
    });
    pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_err1".into(),
        name: "Edit".into(),
        output: "File not found".into(),
        is_error: true,
        source_agent_id: None,
    });

    // Act
    let tail_vms = pipeline.build_tail_vms();

    // Assert：错误 ToolBlock 的 diff_input 应为 None
    let err_block = tail_vms.iter().find(|vm| {
        matches!(vm, MessageViewModel::ToolBlock { tool_name, is_error, .. } if tool_name == "Edit" && *is_error)
    });
    assert!(err_block.is_some(), "应包含 Edit 错误 ToolBlock");

    if let MessageViewModel::ToolBlock { diff_input, .. } = err_block.unwrap() {
        assert!(
            diff_input.is_none(),
            "错误 ToolEnd 不应产生 diff_input，实际: {:?}",
            diff_input
        );
    }
}

// ─── AdaptiveChunkingPolicy 测试 ───────────────────────────────────────

use super::{AdaptiveChunkingPolicy, ChunkingMode, DrainPlan};

/// 辅助：创建策略并用指定数量的行填充
fn make_policy_with_lines(line_count: usize) -> AdaptiveChunkingPolicy {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 每行一个 chunk，模拟逐行到达
    for _ in 0..line_count {
        policy.on_chunk("hello\n");
    }
    policy
}

/// 测试：新策略初始状态为 Smooth，无积压，check 返回 None
#[test]
fn test_policy_initial_state() {
    let policy = AdaptiveChunkingPolicy::new();
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(policy.pending_lines, 0);
    assert!(policy.oldest_chunk_at.is_none());
}

/// 测试：单个 chunk 正确累积行数
#[test]
fn test_policy_on_chunk_single_line() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("hello");
    assert_eq!(policy.pending_lines, 1);
    assert!(policy.oldest_chunk_at.is_some());
}

/// 测试：多行 chunk 正确累积行数
#[test]
fn test_policy_on_chunk_multi_line() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("line1\nline2\nline3");
    assert_eq!(policy.pending_lines, 3);
}

/// 测试：空 chunk 仍记为 1 行（min 1）
#[test]
fn test_policy_on_chunk_empty() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("");
    assert_eq!(policy.pending_lines, 1);
}

/// 测试：积压为 0 时 check 返回 None
#[test]
fn test_policy_check_no_pending() {
    let mut policy = AdaptiveChunkingPolicy::new();
    assert!(policy.check().is_none());
}

/// 测试：Smooth 模式下少量积压返回 Single
#[test]
fn test_policy_smooth_returns_single() {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 低于 queue_depth_threshold(8)，仍在 Smooth
    policy.on_chunk("hello\n");
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Single));
}

/// 测试：队列深度达到阈值（8 行）触发 CatchUp
#[test]
fn test_policy_catchup_by_depth() {
    let mut policy = make_policy_with_lines(8);
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：队列深度超过阈值（10 行）也触发 CatchUp
#[test]
fn test_policy_catchup_by_depth_overflow() {
    let mut policy = make_policy_with_lines(10);
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
}

/// 测试：最老行年龄达到阈值（120ms）触发 CatchUp
#[test]
fn test_policy_catchup_by_age() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("hello\n");
    // 手动设置 oldest_chunk_at 为 150ms 前
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(150));
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：队列深度 7 + 年龄 119ms → 不触发 CatchUp（两个条件都未达阈值）
#[test]
fn test_policy_no_catchup_below_threshold() {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 7 行，低于阈值 8
    for _ in 0..7 {
        policy.on_chunk("hello\n");
    }
    // 年龄 119ms，低于阈值 120ms
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(119));
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
}

/// 测试：CatchUp 模式下 drain 后队列清空，但模式保持 CatchUp
/// （需要同时满足 exit_depth 和 exit_age 才退出）
#[test]
fn test_policy_catchup_drain_stays_catchup() {
    let mut policy = make_policy_with_lines(10);
    // 触发 CatchUp
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后队列为空
    policy.drain();
    assert_eq!(policy.pending_lines, 0);
    // 下一个 check：无积压，返回 None
    assert!(policy.check().is_none());
}

/// 测试：CatchUp → Smooth 退出条件：队列深度 <= 2 且年龄 <= 40ms
#[test]
fn test_policy_exit_catchup() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充少量数据（2 行，年龄 < 40ms）
    policy.drain();
    policy.on_chunk("a\n");
    policy.on_chunk("b\n");
    let plan = policy.check();
    // 退出条件满足：depth=2 <= 2 且年龄 ≈ 0ms <= 40ms
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(plan, Some(DrainPlan::Single));
}

/// 测试：CatchUp 不退出：深度 <= 2 但年龄 > 40ms
#[test]
fn test_policy_no_exit_age_too_old() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充 2 行，但设置年龄为 50ms（> 40ms）
    policy.drain();
    policy.on_chunk("a\n");
    policy.on_chunk("b\n");
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(50));
    let _ = policy.check();
    // 不满足退出：年龄 50ms > 40ms
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：CatchUp 不退出：年龄 <= 40ms 但深度 > 2
#[test]
fn test_policy_no_exit_depth_too_high() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充 3 行（> exit_depth=2），年龄约 0ms
    policy.drain();
    for _ in 0..3 {
        policy.on_chunk("a\n");
    }
    let _ = policy.check();
    // 不满足退出：depth=3 > 2
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：reset 恢复到初始状态
#[test]
fn test_policy_reset() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.reset();
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(policy.pending_lines, 0);
    assert!(policy.oldest_chunk_at.is_none());
}

/// 测试：drain 只清空积压，不改变模式
#[test]
fn test_policy_drain_preserves_mode() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.drain();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    assert_eq!(policy.pending_lines, 0);
}

/// 测试：on_reasoning_chunk 正确累积
#[test]
fn test_policy_on_reasoning_chunk() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_reasoning_chunk();
    assert_eq!(policy.pending_lines, 1);
    assert!(policy.oldest_chunk_at.is_some());
}

/// 测试：连续多轮 chunk → check → drain → chunk 的完整生命周期
#[test]
fn test_policy_lifecycle_smooth_to_catchup_and_back() {
    let mut policy = AdaptiveChunkingPolicy::new();

    // 第一轮：Smooth，3 行
    for _ in 0..3 {
        policy.on_chunk("hello\n");
    }
    assert_eq!(policy.check(), Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    policy.drain();

    // 第二轮：积压爆发，12 行 → CatchUp
    for _ in 0..12 {
        policy.on_chunk("burst\n");
    }
    assert_eq!(policy.check(), Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.drain();

    // 第三轮：恢复正常，1 行 → 应回到 Smooth
    policy.on_chunk("normal\n");
    // drain 后 oldest_chunk_at 已清空，重新填充后年龄约 0ms
    // depth=1 <= 2 且年龄 ≈ 0 <= 40ms → 退出 CatchUp
    assert_eq!(policy.check(), Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
}

// ─── check_throttle 集成测试 ────────────────────────────────────────────

/// 测试：无流式内容时 check_throttle 返回 None
#[test]
fn test_check_throttle_no_content() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：单次 chunk 后 check_throttle 返回 RebuildAll
#[test]
fn test_check_throttle_single_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
    assert!(matches!(result.unwrap(), PipelineAction::RebuildAll { .. }));
}

/// 测试：连续 chunk 积压触发 CatchUp 模式
#[test]
fn test_check_throttle_catchup_on_burst() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // 发送 10 个 chunk（超过 queue_depth_threshold=8）
    for i in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: format!("line {}\n", i),
            source_agent_id: None,
        });
    }
    // check_throttle 会触发 update_mode，此时应切换到 CatchUp 并立即返回
    let result = pipeline.check_throttle(0);
    assert!(result.is_some(), "CatchUp 模式应立即返回 RebuildAll");
    // drain 后 pending_lines 归零
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
}

/// 测试：ToolStart 消费积压后 check_throttle 返回 None
#[test]
fn test_check_throttle_drained_after_tool_start() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    // ToolStart 会 drain 积压
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/src/main.rs"}),
        source_agent_id: None,
    });
    // 积压已被消费
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：done() 重置策略状态
#[test]
fn test_done_resets_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // 积压数据
    for _ in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: "burst\n".into(),
            source_agent_id: None,
        });
    }
    assert!(pipeline.adaptive_policy.pending_lines > 0);
    pipeline.done();
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    assert_eq!(pipeline.adaptive_policy.mode, ChunkingMode::Smooth);
    assert!(pipeline.check_throttle(0).is_none());
}

/// 测试：begin_round() 重置策略状态
#[test]
fn test_begin_round_resets_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    for _ in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: "burst\n".into(),
            source_agent_id: None,
        });
    }
    pipeline.begin_round();
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    assert_eq!(pipeline.adaptive_policy.mode, ChunkingMode::Smooth);
}

// ─── StreamingMode 测试 ──────────────────────────────────────────────────

use super::StreamingMode;

#[test]
fn test_streaming_mode_default() {
    let pipeline = MessagePipeline::new("/tmp".to_string());
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Streaming);
}

#[test]
fn test_set_streaming_mode() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Block);
    pipeline.set_streaming_mode(StreamingMode::None);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::None);
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    assert_eq!(pipeline.streaming_mode(), StreamingMode::Streaming);
}

#[test]
fn test_push_chunk_streaming_updates_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    pipeline.push_chunk("hello");
    assert_eq!(pipeline.current_ai_text, "hello");
    assert!(pipeline.adaptive_policy.pending_lines > 0);
}

#[test]
fn test_push_chunk_none_only_accumulates() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::None);
    pipeline.push_chunk("hello");
    assert_eq!(pipeline.current_ai_text, "hello");
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
}

#[test]
fn test_push_chunk_block_buffering() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello");
    assert_eq!(pipeline.current_ai_text, "");
    assert_eq!(pipeline.block_buffer, "hello");
}

#[test]
fn test_push_chunk_block_flush_on_double_newline() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("paragraph one\n\n");
    assert_eq!(pipeline.current_ai_text, "paragraph one\n\n");
    assert!(pipeline.block_buffer.is_empty());
    assert!(pipeline.block_pending_flush);
}

#[test]
fn test_push_chunk_block_code_fence() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("```rust\n");
    assert!(pipeline.block_scanner.in_code_fence());
    assert_eq!(pipeline.current_ai_text, "");
    pipeline.push_chunk("fn main() {}\n");
    assert_eq!(pipeline.current_ai_text, "");
    pipeline.push_chunk("```\n");
    assert!(!pipeline.block_scanner.in_code_fence());
    assert_eq!(pipeline.current_ai_text, "```rust\nfn main() {}\n```\n");
    assert!(pipeline.block_pending_flush);
}

/// 围栏在同一 chunk 内打开、且围栏内出现空行时，不能把空行当成块边界提前提交。
/// 旧实现先检查 `contains("\\n\\n")` 再检测围栏，会把这类文本提前 flush。
#[test]
fn test_push_chunk_block_fence_opened_in_same_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("intro\n```rust\n\n");
    assert_eq!(
        pipeline.current_ai_text, "",
        "围栏内的空行不应触发块提交，实际: {}",
        pipeline.current_ai_text
    );
    assert!(!pipeline.block_pending_flush);
    assert!(pipeline.block_scanner.in_code_fence());

    // 围栏闭合 = 块结束，整段（含围栏前的段落）一次提交
    pipeline.push_chunk("fn main() {}\n```\n");
    assert_eq!(
        pipeline.current_ai_text,
        "intro\n```rust\n\nfn main() {}\n```\n"
    );
    assert!(!pipeline.block_scanner.in_code_fence());
    assert!(pipeline.block_pending_flush);
}

#[test]
fn test_check_throttle_none_mode() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::None);
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

#[test]
fn test_check_throttle_block_no_flush() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello");
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

#[test]
fn test_check_throttle_block_after_flush() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("hello\n\n");
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
    let result2 = pipeline.check_throttle(0);
    assert!(result2.is_none());
}

#[test]
fn test_set_streaming_mode_flushes_on_exit_block() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.push_chunk("unflushed content");
    assert_eq!(pipeline.current_ai_text, "");
    pipeline.set_streaming_mode(StreamingMode::Streaming);
    assert_eq!(pipeline.current_ai_text, "unflushed content");
    assert!(pipeline.block_buffer.is_empty());
}

#[test]
fn test_done_flushes_block_buffer() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.set_streaming_mode(StreamingMode::Block);
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    assert_eq!(pipeline.current_ai_text, "");
    pipeline.done();
    // done() calls finalize_current_ai() which clears current_ai_text, but block_buffer was flushed
    assert!(pipeline.block_buffer.is_empty());
}

#[test]
fn test_steering_snapshot_preserves_entire_current_round() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.restore_completed(vec![
        BaseMessage::human("历史"),
        BaseMessage::ai("历史答复"),
    ]);
    pipeline.begin_round();
    pipeline.set_completed(vec![BaseMessage::ai("补充前已完成的工作")]);
    pipeline.set_completed(vec![
        BaseMessage::human("新截图"),
        BaseMessage::ai("结合截图继续"),
    ]);
    let actual = pipeline.build_tail_vms();
    let expected =
        MessagePipeline::messages_to_view_models(&pipeline.completed[2..], &pipeline.cwd);
    assert_eq!(
        format!("{actual:?}"),
        format!("{expected:?}"),
        "补充信息不能截掉同一轮之前的回答或工具"
    );
    assert_eq!(actual.len(), 3);
}

// ── 改动 B：消息区「思考 + 只读工具计数」 ──────────────────────────

/// 构造带 reasoning 的 assistant 消息 + 其后跟若干工具结果
fn make_ai_with_tools(tools: &[(&str, &str)]) -> Vec<BaseMessage> {
    let mut tool_calls = Vec::new();
    for (i, &(name, arg)) in tools.iter().enumerate() {
        tool_calls.push(ToolCallRequest {
            id: format!("tc{i}"),
            name: name.to_string(),
            arguments: if name == "Bash" {
                json!({ "command": arg })
            } else if name == "Glob" || name == "Grep" {
                json!({ "pattern": arg })
            } else {
                json!({ "file_path": arg })
            },
        });
    }
    let mut msgs = vec![
        BaseMessage::human("查一下"),
        BaseMessage::Ai {
            id: cc_agent::messages::MessageId::new(),
            content: MessageContent::blocks(vec![ContentBlock::Reasoning {
                text: "先看看文件".to_string(),
                signature: None,
                duration_ms: Some(4000),
            }]),
            tool_calls: tool_calls.clone(),
        },
    ];
    for tc in &tool_calls {
        msgs.push(BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: tc.id.clone(),
            content: MessageContent::text("结果"),
            is_error: false,
        });
    }
    msgs
}

/// 构造 Read 结果在前、Reasoning + 后续工具调用在后的消息序列。
fn make_read_then_reasoning_messages() -> Vec<BaseMessage> {
    let read_call = ToolCallRequest {
        id: "read-1".to_string(),
        name: "Read".to_string(),
        arguments: json!({ "file_path": "/a.rs" }),
    };
    let next_call = ToolCallRequest {
        id: "bash-1".to_string(),
        name: "Bash".to_string(),
        arguments: json!({ "command": "cargo check" }),
    };

    vec![
        BaseMessage::human("检查文件"),
        BaseMessage::Ai {
            id: cc_agent::messages::MessageId::new(),
            content: MessageContent::blocks(Vec::new()),
            tool_calls: vec![read_call.clone()],
        },
        BaseMessage::Tool {
            id: cc_agent::messages::MessageId::new(),
            tool_call_id: read_call.id,
            content: MessageContent::text("文件内容"),
            is_error: false,
        },
        BaseMessage::Ai {
            id: cc_agent::messages::MessageId::new(),
            content: MessageContent::blocks(vec![ContentBlock::Reasoning {
                text: "根据文件内容继续检查".to_string(),
                signature: None,
                duration_ms: Some(1200),
            }]),
            tool_calls: vec![next_call],
        },
    ]
}

/// 提取首个 Reasoning block 的 action_summary
fn first_action_summary(vms: &[MessageViewModel]) -> Option<String> {
    for vm in vms {
        // 一等公民：思考行可能挂在折叠组上
        if let MessageViewModel::ToolCallGroup {
            thinking: Some(t), ..
        } = vm
        {
            return t.action_summary.clone();
        }
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            for b in blocks {
                if let ContentBlockView::Reasoning { action_summary, .. } = b {
                    return action_summary.clone();
                }
            }
        }
    }
    None
}

#[test]
fn test_action_summary_injected_for_read_and_grep() {
    // Arrange：一个 Read + 一个 Grep（均只读）
    let msgs = make_ai_with_tools(&[("Read", "/a.rs"), ("Grep", "todo")]);

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");

    // Assert
    assert_eq!(
        first_action_summary(&vms).as_deref(),
        Some("read 1 file, searched for 1 pattern"),
        "应注入 Read/Grep 计数"
    );
}

fn read_group_standalone_action(vms: &[MessageViewModel]) -> Option<Option<String>> {
    vms.iter().find_map(|vm| match vm {
        MessageViewModel::ToolCallGroup {
            thinking: None,
            tools,
            standalone_action,
            ..
        } if tools.iter().any(|tool| tool.tool_name == "Read") => Some(standalone_action.clone()),
        _ => None,
    })
}

#[test]
fn test_action_summary_absorbs_read_group_before_thought() {
    let msgs = make_read_then_reasoning_messages();

    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");

    // §13.6：不再跨段吸收——前置 Read 组保留自己的摘要行，Thought 行不带计数
    assert_eq!(
        first_action_summary(&vms).as_deref(),
        None,
        "Thought 不应吸收前置 Read 计数"
    );
    assert_eq!(
        read_group_standalone_action(&vms),
        Some(Some("read 1 file".to_string())),
        "前置工具组应保留自己的独立动作行"
    );
}

#[test]
fn test_build_tail_merges_completed_read_with_streaming_thought() {
    let mut completed = make_read_then_reasoning_messages();
    completed.pop(); // 当前 Reasoning 仍在流式状态，尚未进入 completed。

    let mut pipeline = MessagePipeline::new("/p".to_string());
    pipeline.completed = completed;
    pipeline.has_snapshot_this_round = true;
    pipeline.current_ai_reasoning = "根据文件内容继续检查".to_string();

    let tail_vms = pipeline.build_tail_vms();

    // §13.6：实时流式 Thought 同样不吸收前缀计数
    assert_eq!(
        first_action_summary(&tail_vms).as_deref(),
        None,
        "实时流式 Thought 不应吸收 completed 前缀中的 Read 计数"
    );
    assert_eq!(
        read_group_standalone_action(&tail_vms),
        Some(Some("read 1 file".to_string())),
        "前缀工具组保留自己的动作行"
    );
}

#[test]
fn test_action_summary_plural_and_glob() {
    // Arrange：两个 Read + 两个 Glob
    let msgs = make_ai_with_tools(&[
        ("Read", "/a.rs"),
        ("Read", "/b.rs"),
        ("Glob", "*.rs"),
        ("Glob", "*.md"),
    ]);

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");

    // Assert
    assert_eq!(
        first_action_summary(&vms).as_deref(),
        Some("read 2 files, listed 2 directories"),
        "复数形式应正确"
    );
}

#[test]
fn test_bash_not_counted_as_readonly() {
    // Arrange：只有 Bash（不属只读工具）
    let msgs = make_ai_with_tools(&[("Bash", "cargo build -p cc-tui")]);

    // Act
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");

    // Assert：非只读命令 → 不注入计数（`ls` 这类会被确定性判定折叠，见下一个用例）
    assert_eq!(first_action_summary(&vms), None, "非只读 Bash 不应产生计数");
}

#[test]
fn test_bash_readonly_heuristic_folds_without_flag() {
    // 模型没声明 file_inspect，但命令本身是文件查看 → 确定性判定应折叠并计数
    let msgs = make_ai_with_tools(&[("Bash", "ls -la")]);
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/p");
    assert_eq!(
        first_action_summary(&vms).as_deref(),
        Some("ran 1 shell command"),
        "只读命令应由启发式判为文件查看"
    );
}

#[test]
fn test_readonly_action_summary_pure_fn() {
    use super::transform::readonly_action_summary;
    assert_eq!(readonly_action_summary(0, 0, 0), None);
    assert_eq!(
        readonly_action_summary(1, 0, 0).as_deref(),
        Some("read 1 file")
    );
    assert_eq!(
        readonly_action_summary(0, 1, 0).as_deref(),
        Some("listed 1 directory")
    );
    assert_eq!(
        readonly_action_summary(0, 0, 2).as_deref(),
        Some("searched for 2 patterns")
    );
}

/// 前台 `!` 回流的上下文片段在展示层应被跳过（否则与 ShellCommand VM 重复）。
#[test]
fn test_bash_context_fragments_skipped_in_view() {
    let msgs = vec![
        BaseMessage::human("<local-command-caveat>Caveat: ...</local-command-caveat>"),
        BaseMessage::human("<bash-input>deploy --prod</bash-input>"),
        BaseMessage::human("<bash-stdout>done</bash-stdout><bash-stderr></bash-stderr>"),
    ];
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    assert!(
        vms.is_empty(),
        "三类上下文片段都不应渲染，实际产生 {} 个 VM: {:?}",
        vms.len(),
        vms
    );
}

/// 反例：正常用户消息不得被误跳过（含以 `<` 开头的边角输入）。
#[test]
fn test_normal_user_messages_not_skipped() {
    let msgs = vec![
        BaseMessage::human("bash-input is a tag"),
        BaseMessage::human("<bash-input>"),
        BaseMessage::human("<b>粗体</b>"),
        BaseMessage::human("<local-command-caveat"),
    ];
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    assert_eq!(
        vms.len(),
        msgs.len(),
        "正常用户消息不应被跳过（仅完整前缀匹配才跳过），实际: {:?}",
        vms
    );
}

/// 回答「退出 TUI 后恢复会话，之前 `!` 命令能否正常渲染」：
/// 片段在展示层被跳过，真实展示由 ShellCommand VM 从 shell-commands.jsonl 提供。
#[test]
fn test_resumed_session_shows_shell_command_once() {
    // 模拟 resume 时的 base_msgs：正常对话 + 落盘的 ! 回流片段
    let base_msgs = vec![
        BaseMessage::human("帮我部署"),
        BaseMessage::ai("好的"),
        BaseMessage::human("<local-command-caveat>Caveat: ...</local-command-caveat>"),
        BaseMessage::human("<bash-input>deploy --prod</bash-input>"),
        BaseMessage::human("<bash-stdout>done</bash-stdout><bash-stderr></bash-stderr>"),
    ];
    let vms = MessagePipeline::messages_to_view_models(&base_msgs, "/tmp");

    // 只有正常对话渲染；三条片段全部跳过（否则会与 ShellCommand VM 重复）
    assert_eq!(vms.len(), 2, "应仅渲染 2 条正常消息，实际: {vms:?}");
    assert!(
        !vms.iter()
            .any(|vm| format!("{vm:?}").contains("bash-input")),
        "片段不应出现在视图中"
    );
    assert!(
        !vms.iter()
            .any(|vm| format!("{vm:?}").contains("local-command-caveat")),
        "caveat 不应出现在视图中"
    );
}

/// 取流式气泡里文本块的 raw（无文本块时返回空串）
fn streaming_text_raw(pipeline: &MessagePipeline) -> String {
    let MessageViewModel::AssistantBubble { blocks, .. } = pipeline.build_streaming_bubble() else {
        panic!("流式气泡应为 AssistantBubble");
    };
    blocks
        .iter()
        .rev()
        .find_map(|b| match b {
            ContentBlockView::Text { raw, .. } => Some(raw.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// 流式气泡不在 pipeline 侧解析 markdown：交出去的是脏空块，由渲染线程增量解析
#[test]
fn test_build_streaming_bubble_defers_markdown_parse() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_chunk("hello world");
    let vm = pipeline.build_streaming_bubble();
    let MessageViewModel::AssistantBubble {
        blocks,
        is_streaming,
        ..
    } = vm
    else {
        panic!("流式气泡应为 AssistantBubble");
    };
    assert!(is_streaming);
    let ContentBlockView::Text {
        raw,
        rendered,
        dirty,
        rendered_prefix_len,
        rendered_prefix_lines,
        rendered_width,
        ..
    } = &blocks[0]
    else {
        panic!("首个块应为 Text");
    };
    assert_eq!(raw, "hello world");
    assert!(*dirty, "流式文本块必须标脏，交给渲染线程解析");
    assert_eq!(*rendered_prefix_len, 0, "pipeline 不应声称已渲染");
    assert_eq!(*rendered_prefix_lines, 0);
    assert_eq!(*rendered_width, 0);
    let has_text = rendered
        .lines
        .iter()
        .any(|l| l.spans.iter().any(|s| !s.content.is_empty()));
    assert!(!has_text, "pipeline 不应预解析 markdown");
}

/// 未完成的长行暂不进入视图，短尾照常显示（首个 token 立即可见）
#[test]
fn test_build_streaming_bubble_holds_long_unfinished_line() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let long_tail = "x".repeat(250);
    pipeline.push_chunk(&format!("first line\n{long_tail}"));
    assert_eq!(
        streaming_text_raw(&pipeline),
        "first line\n",
        "超过阈值的未完成行应暂不显示"
    );

    let mut short = MessagePipeline::new("/tmp".to_string());
    short.push_chunk("short tail");
    assert_eq!(
        streaming_text_raw(&short),
        "short tail",
        "未出现换行时整段照常显示"
    );
}

#[test]
fn test_merged_segment_keeps_single_thinking_owner() {
    // 思考 + Read（无正文）：思考行必须只由组持有，否则同一段思考会被渲染两次
    let msgs = make_ai_with_tools(&[("Read", "/a.rs")]);
    let vms = MessagePipeline::messages_to_view_models(&msgs, "/tmp");
    let group_thinking = vms
        .iter()
        .filter(|vm| {
            matches!(
                vm,
                MessageViewModel::ToolCallGroup {
                    thinking: Some(_),
                    ..
                }
            )
        })
        .count();
    let bubble_reasoning = vms
        .iter()
        .filter(|vm| {
            matches!(
                vm,
                MessageViewModel::AssistantBubble { blocks, .. }
                    if blocks.iter().any(|b| matches!(b, ContentBlockView::Reasoning { .. }))
            )
        })
        .count();
    assert_eq!(group_thinking, 1, "无正文时思考行应由折叠组持有");
    assert_eq!(bubble_reasoning, 0, "无正文时不得再保留思考气泡");
}

/// 中断后当前思考段必须冻结。
///
/// 否则 `live_thinking_active()`（thinking_now || 未结束工具）仍为真，视图装配点会
/// 保留最后一行的活跃标记 —— 真机表现是"取消后 `∴ Thinking for 54s…` 继续走秒、符号还在闪"。
#[test]
fn test_interrupt_freezes_active_thinking() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_reasoning("正在思考");
    assert!(pipeline.live_thinking_active(), "思考中应判为活跃");

    pipeline.interrupt();

    assert!(
        !pipeline.live_thinking_active(),
        "中断后不应再有活跃思考（否则会继续走秒闪烁）"
    );
}

/// PreToolUse 的 hook 行早于 ToolStart 到达：先暂存，工具块建出来后再挂上。
#[test]
fn test_hook_rows_pending_before_tool_start_are_attached_after() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_hook_result(
        "tc_pre",
        HookExec::new("PreToolUse", 1, 8, Vec::new(), false),
    );
    assert!(
        pipeline
            .build_tail_vms()
            .iter()
            .all(|vm| !matches!(vm, MessageViewModel::ToolBlock { .. })),
        "工具块未建立时不应凭空造出工具块"
    );

    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_pre".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "sleep 10".into(),
        input: json!({"command": "sleep 10"}),
        source_agent_id: None,
    });

    for _ in 0..2 {
        let tail = pipeline.build_tail_vms();
        let rows = tail
            .iter()
            .find_map(|vm| match vm {
                MessageViewModel::ToolBlock {
                    tool_call_id,
                    hook_rows,
                    ..
                } if tool_call_id == "tc_pre" => Some(hook_rows.clone()),
                _ => None,
            })
            .expect("ToolStart 之后应生成工具块");
        assert_eq!(rows.len(), 1, "暂存的 hook 行必须被挂上");
        assert_eq!(rows[0].text, "1 PreToolUse hooks ran · 8ms");
    }
}

/// 只读工具带 hook 行时不进聚合组：组不承载 hook 行，进组会把阻断/失败行吞掉。
#[test]
fn test_hook_rows_keep_readonly_tool_out_of_group() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_read".into(),
        name: "Read".into(),
        display: "Read".into(),
        args: "Cargo.toml".into(),
        input: json!({"file_path": "Cargo.toml"}),
        source_agent_id: None,
    });
    let tail = pipeline.build_tail_vms();
    assert!(
        !tail
            .iter()
            .any(|vm| matches!(vm, MessageViewModel::ToolBlock { .. })),
        "对照：无 hook 行的只读工具应被聚合成组"
    );

    pipeline.push_hook_result(
        "tc_read",
        HookExec::new(
            "PreToolUse",
            1,
            3,
            vec![cc_agent::agent::events::HookFailureInfo {
                kind: cc_agent::agent::events::HookFailureKind::Block,
                reason: "policy".into(),
            }],
            false,
        ),
    );
    let tail = pipeline.build_tail_vms();
    assert!(
        tail.iter().any(|vm| matches!(
            vm,
            MessageViewModel::ToolBlock { tool_call_id, hook_rows, .. }
                if tool_call_id == "tc_read" && !hook_rows.is_empty()
        )),
        "带 hook 行的只读工具块必须留在单条里"
    );
}

/// 建一个 Bash 工具块并取回挂在它上面的 hook 行。
fn tool_hook_rows(
    pipeline: &mut MessagePipeline,
    tool_call_id: &str,
) -> Vec<crate::ui::message_view::HookRow> {
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: tool_call_id.into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: json!({"command": "ls"}),
        source_agent_id: None,
    });
    pipeline
        .build_tail_vms()
        .iter()
        .find_map(|vm| match vm {
            MessageViewModel::ToolBlock {
                tool_call_id: id,
                hook_rows,
                ..
            } if id == tool_call_id => Some(hook_rows.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// 同一工具调用的同一事件分两次到达（global + project 各配一个 hook）：
/// 只占一行，计数与耗时累加（PRD C4）。
#[test]
fn test_hook_result_merges_same_event_for_same_tool_call() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_hook_result(
        "tc_merge",
        HookExec::new("PreToolUse", 1, 5, Vec::new(), false),
    );
    pipeline.push_hook_result(
        "tc_merge",
        HookExec::new("PreToolUse", 1, 7, Vec::new(), false),
    );

    let rows = tool_hook_rows(&mut pipeline, "tc_merge");
    assert_eq!(rows.len(), 1, "同一事件的两次上报必须合并成一行");
    assert_eq!(rows[0].text, "2 PreToolUse hooks ran · 12ms");
    assert!(rows[0].detail_only, "汇总行只在详细模式出现");
}

/// 合并时 failures 也拼接：第二个 middleware 实例的失败不覆盖第一个。
#[test]
fn test_hook_result_merge_concats_failures() {
    use cc_agent::agent::events::{HookFailureInfo, HookFailureKind};
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_hook_result(
        "tc_fail",
        HookExec::new(
            "PostToolUse",
            2,
            4,
            vec![HookFailureInfo {
                kind: HookFailureKind::Error,
                reason: "exit 1".into(),
            }],
            false,
        ),
    );
    pipeline.push_hook_result(
        "tc_fail",
        HookExec::new(
            "PostToolUse",
            1,
            6,
            vec![HookFailureInfo {
                kind: HookFailureKind::Block,
                reason: "denied".into(),
            }],
            false,
        ),
    );

    let rows = tool_hook_rows(&mut pipeline, "tc_fail");
    assert_eq!(rows.len(), 3, "两条失败行 + 一条汇总行");
    assert_eq!(rows[0].text, "PostToolUse hook failed: exit 1");
    assert_eq!(rows[1].text, "PostToolUse hook blocked: denied");
    assert_eq!(rows[2].text, "3 PostToolUse hooks ran · 10ms");
}

/// 异步完成行不参与合并：每个异步 hook 各占一行。
#[test]
fn test_async_hook_completions_stay_separate_rows() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.push_hook_result(
        "tc_async",
        HookExec::new("PostToolUse", 1, 6, Vec::new(), true),
    );
    pipeline.push_hook_result(
        "tc_async",
        HookExec::new("PostToolUse", 1, 9, Vec::new(), true),
    );

    let rows = tool_hook_rows(&mut pipeline, "tc_async");
    assert_eq!(rows.len(), 2, "每个异步 hook 各占一行");
    assert!(rows.iter().all(|row| row.async_row));
    assert!(rows
        .iter()
        .all(|row| row.text == "Async hook PostToolUse completed"));
}

/// 取当前尾部工具块上已挂的 hook 行（不新建工具块）。
fn tail_hook_rows(
    pipeline: &MessagePipeline,
    tool_call_id: &str,
) -> Vec<crate::ui::message_view::HookRow> {
    pipeline
        .build_tail_vms()
        .iter()
        .find_map(|vm| match vm {
            MessageViewModel::ToolBlock {
                tool_call_id: id,
                hook_rows,
                ..
            } if id == tool_call_id => Some(hook_rows.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// 异步 hook 开始时先挂一条 Running 占位行。
#[test]
fn test_async_hook_started_shows_running_row() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.start_hook_result("tc_running", "PostToolUse");

    let rows = tool_hook_rows(&mut pipeline, "tc_running");
    assert_eq!(rows.len(), 1, "开始事件应写入一条占位行");
    assert_eq!(rows[0].text, "Async hook PostToolUse running…");
    assert!(rows[0].detail_only, "占位行只在详细模式出现");
    assert!(rows[0].async_row, "占位行不参与汇总行折叠");
}

/// 完成事件就地替换同事件的占位行：位置不变、行数不增。
#[test]
fn test_async_hook_completed_replaces_running_row_in_place() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.start_hook_result("tc_replace", "PostToolUse");
    let _ = tool_hook_rows(&mut pipeline, "tc_replace");
    let before = tail_hook_rows(&pipeline, "tc_replace");
    assert_eq!(before.len(), 1, "替换前应只有占位行");

    let replaced = pipeline.finish_hook_result(
        "tc_replace",
        HookExec::new("PostToolUse", 1, 9, Vec::new(), true),
    );

    assert!(replaced, "命中占位时应就地替换");
    let after = tail_hook_rows(&pipeline, "tc_replace");
    assert_eq!(after.len(), before.len(), "替换不得增加行数");
    assert_eq!(after[0].text, "Async hook PostToolUse completed");
}

/// 没有占位时（跨轮到达的完成事件）退回追加一行，保持旧行为。
#[test]
fn test_async_hook_completed_without_start_appends_row() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let replaced = pipeline.finish_hook_result(
        "tc_nostart",
        HookExec::new("PostToolUse", 1, 6, Vec::new(), true),
    );

    assert!(!replaced, "没有占位时不应报告命中");
    let rows = tool_hook_rows(&mut pipeline, "tc_nostart");
    assert_eq!(rows.len(), 1, "完成事件独立追加一行");
    assert_eq!(rows[0].text, "Async hook PostToolUse completed");
}

/// 轮次结束时仍为 Running 的占位必须清掉，已完成的异步行保留。
#[test]
fn test_pipeline_done_drops_stale_running_hooks() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.start_hook_result("tc_stale", "PostToolUse");
    pipeline.push_hook_result(
        "tc_done",
        HookExec::new("PostToolUse", 1, 6, Vec::new(), true),
    );

    pipeline.done();

    assert!(
        !pipeline.hook_results.contains_key("tc_stale"),
        "done() 必须移除僵尸占位，避免永远显示 running…"
    );
    assert_eq!(
        pipeline.hook_results.get("tc_done").map(Vec::len),
        Some(1),
        "已完成行随工具块保留，不得被 done() 清掉"
    );
}
