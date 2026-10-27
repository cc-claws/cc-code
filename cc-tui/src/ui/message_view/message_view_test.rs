use super::*;
use crate::ui::message_view::tools::is_readonly_shell_command;
use crate::ui::message_view::ContentBlockView;
use cc_agent::messages::{MessageContent, ToolCallRequest};
use serde_json::json;

#[test]
fn test_history_rebuild_sets_finished_phase() {
    let result = BaseMessage::tool_result("bash", "");
    let vm = MessageViewModel::from_base_message(
        &result,
        &[(
            "bash".to_string(),
            "Bash".to_string(),
            json!({"command":"true", "timeout":300_000}),
        )],
    );
    assert!(
        matches!(
            vm,
            MessageViewModel::ToolBlock {
                phase: ToolPhase::Finished,
                timeout_ms: Some(300_000),
                started_at: None,
                ..
            }
        ),
        "历史工具应已结束并保留显式 timeout"
    );
}

/// 测试：AI 消息只有 tool_calls（无 content）时，应正确渲染工具调用
#[test]
fn test_ai_message_with_only_tool_calls_renders_tool_use() {
    // 模拟：AI 消息只包含 tool_calls，content 为空
    let msg = BaseMessage::ai_with_tool_calls(
        MessageContent::text(""),
        vec![
            ToolCallRequest::new("toolu_001", "Bash", json!({"command": "ls"})),
            ToolCallRequest::new("toolu_002", "Read", json!({"path": "test.txt"})),
        ],
    );

    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::AssistantBubble { blocks, .. } => {
            // 应该有 2 个 ToolUse block
            let tool_uses: Vec<_> = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                .collect();
            assert_eq!(tool_uses.len(), 2, "应该有 2 个 ToolUse block");

            // 验证工具名称
            let names: Vec<&str> = blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlockView::ToolUse { name } = b {
                        Some(name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(names.contains(&"Bash"), "应包含 bash 工具");
            assert!(names.contains(&"Read"), "应包含 read_file 工具");
        }
        _ => panic!("应该是 AssistantBubble"),
    }
}

/// 测试：AI 消息同时有文本和 tool_calls 时，两者都应渲染
#[test]
fn test_ai_message_with_text_and_tool_calls_renders_both() {
    let msg = BaseMessage::ai_with_tool_calls(
        MessageContent::text("I'll run a command"),
        vec![ToolCallRequest::new(
            "toolu_001",
            "Bash",
            json!({"command": "ls"}),
        )],
    );

    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::AssistantBubble { blocks, .. } => {
            // 应该有 1 个 Text block 和 1 个 ToolUse block
            let text_count = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlockView::Text { .. }))
                .count();
            let tool_count = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                .count();

            assert_eq!(text_count, 1, "应该有 1 个 Text block");
            assert_eq!(tool_count, 1, "应该有 1 个 ToolUse block");
        }
        _ => panic!("应该是 AssistantBubble"),
    }
}

/// 测试：content 中已有 ToolUse block 时，不重复添加 tool_calls
#[test]
fn test_no_duplicate_tool_use_from_tool_calls() {
    use cc_agent::messages::ContentBlock;

    // content 中包含 ToolUse block，同时 tool_calls 也有相同的
    let blocks = vec![
        ContentBlock::text("I'll run bash"),
        ContentBlock::tool_use("toolu_001", "Bash", json!({"command": "ls"})),
    ];
    let msg = BaseMessage::ai_from_blocks(blocks);

    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::AssistantBubble { blocks, .. } => {
            // 应该只有 1 个 ToolUse block（不重复）
            let tool_count = blocks
                .iter()
                .filter(|b| matches!(b, ContentBlockView::ToolUse { .. }))
                .count();
            assert_eq!(tool_count, 1, "不应该重复添加 ToolUse block");
        }
        _ => panic!("应该是 AssistantBubble"),
    }
}

/// 测试：纯文本 AI 消息正常渲染
#[test]
fn test_ai_message_with_only_text_renders_text() {
    let msg = BaseMessage::ai("Hello, how can I help?");

    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::AssistantBubble { blocks, .. } => {
            assert_eq!(blocks.len(), 1, "应该有 1 个 block");
            assert!(
                matches!(blocks[0], ContentBlockView::Text { .. }),
                "应该是 Text block"
            );
        }
        _ => panic!("应该是 AssistantBubble"),
    }
}

#[test]
fn test_tool_category_new_names() {
    // 判定只看命令本身；历史残留的 file_inspect 字段一律忽略
    assert!(super::tools::is_file_inspect(
        "Bash",
        &json!({"command": "ls -la"})
    ));
    assert!(!super::tools::is_file_inspect(
        "Bash",
        &json!({"command": "cargo build", "file_inspect": true})
    ));
    assert!(!super::tools::is_file_inspect(
        "Read",
        &json!({"file_inspect": true})
    ));
    assert_eq!(
        ToolCategory::from_tool_name("Read"),
        Some(ToolCategory::Read)
    );
    assert_eq!(
        ToolCategory::from_tool_name("Grep"),
        Some(ToolCategory::Search)
    );
    assert_eq!(
        ToolCategory::from_tool_name("Glob"),
        Some(ToolCategory::Glob)
    );
    assert_eq!(ToolCategory::from_tool_name("Write"), None);
    assert_eq!(ToolCategory::from_tool_name("Bash"), None);
    assert_eq!(ToolCategory::from_tool_name("Agent"), None);
    assert_eq!(
        ToolCategory::from_tool_name("AskUserQuestion"),
        Some(ToolCategory::AskUser)
    );
    assert_eq!(
        ToolCategory::from_tool_block("Bash", true),
        Some(ToolCategory::Inspect)
    );
    assert_eq!(ToolCategory::from_tool_block("Bash", false), None);
}

#[test]
fn test_file_inspect_requires_explicit_boolean_flag() {
    for input in [
        json!({}),
        json!({"file_inspect": false}),
        json!({"file_inspect": "true"}),
        json!({"file_inspect": 1}),
        json!({"file_inspect": null}),
    ] {
        assert!(
            !super::is_file_inspect("Bash", &input),
            "只有显式布尔 true 才能折叠：{input}"
        );
    }
}

fn make_test_tool_block(
    name: &str,
    file_inspect: bool,
    phase: ToolPhase,
    timeout_ms: Option<u64>,
    shell_backgrounded: bool,
    content: &str,
) -> MessageViewModel {
    let mut vm = MessageViewModel::tool_block_with_id(
        format!("{name}-id"),
        name.to_string(),
        name.to_string(),
        Some("test-command".to_string()),
        false,
    );
    if let MessageViewModel::ToolBlock {
        file_inspect: flag,
        phase: actual_phase,
        timeout_ms: actual_timeout,
        shell_backgrounded: actual_backgrounded,
        content: actual_content,
        ..
    } = &mut vm
    {
        *flag = file_inspect;
        *actual_phase = phase;
        *actual_timeout = timeout_ms;
        *actual_backgrounded = shell_backgrounded;
        *actual_content = content.to_string();
    }
    vm.recompute_hash();
    vm
}

#[test]
fn test_aggregate_bash_file_inspect_joins_read_group() {
    let mut messages = vec![
        make_test_tool_block("Bash", true, ToolPhase::Finished, None, false, "files"),
        make_test_tool_block(
            "Read",
            false,
            ToolPhase::Finished,
            None,
            false,
            "Read 1 lines",
        ),
    ];

    aggregate_tool_groups(&mut messages);

    assert!(matches!(
        messages.as_slice(),
        [MessageViewModel::ToolCallGroup { tools, .. }]
            if tools.len() == 2 && tools[0].tool_name == "Bash" && tools[0].file_inspect
    ));
}

#[test]
fn test_aggregate_bash_file_inspect_joins_group_unless_backgrounded() {
    // §13.1：只要 file_inspect=true 就入组（任意 phase、带 timeout 也入组）
    let grouped = [
        (ToolPhase::Queued, None, false, "files"),
        (ToolPhase::AwaitingApproval, None, false, "files"),
        (ToolPhase::Running, None, false, "files"),
        (ToolPhase::Finished, None, false, "files"),
        (ToolPhase::Finished, Some(1_000), false, "files"),
    ];
    for (phase, timeout_ms, shell_backgrounded, content) in grouped {
        let mut messages = vec![make_test_tool_block(
            "Bash",
            true,
            phase,
            timeout_ms,
            shell_backgrounded,
            content,
        )];
        aggregate_tool_groups(&mut messages);
        assert!(
            matches!(messages.as_slice(), [MessageViewModel::ToolCallGroup { .. }]),
            "应入组: phase={phase:?} timeout={timeout_ms:?}"
        );
    }

    // 仍排除：已转后台、内容为后台启动标记
    for (shell_backgrounded, content) in [
        (true, "files"),
        (
            false,
            "<background-task-started><task-id>shell-1</task-id><command>ls</command><output>out.log</output></background-task-started>",
        ),
    ] {
        let mut messages = vec![make_test_tool_block(
            "Bash",
            true,
            ToolPhase::Finished,
            None,
            shell_backgrounded,
            content,
        )];
        aggregate_tool_groups(&mut messages);
        assert!(
            matches!(messages.as_slice(), [MessageViewModel::ToolBlock { .. }]),
            "后台态应保持单条"
        );
    }
}

#[test]
fn test_aggregate_single_bash_file_inspect_still_groups() {
    let mut messages = vec![make_test_tool_block(
        "Bash",
        true,
        ToolPhase::Finished,
        None,
        false,
        "files",
    )];

    aggregate_tool_groups(&mut messages);

    assert!(matches!(
        messages.as_slice(),
        [MessageViewModel::ToolCallGroup { tools, .. }]
            if tools.len() == 1 && tools[0].file_inspect
    ));
}

#[test]
fn test_aggregate_bash_without_flag_splits_groups() {
    let mut messages = vec![
        make_test_tool_block(
            "Read",
            false,
            ToolPhase::Finished,
            None,
            false,
            "Read 1 lines",
        ),
        make_test_tool_block("Bash", false, ToolPhase::Finished, None, false, "files"),
        make_test_tool_block("Bash", true, ToolPhase::Finished, None, false, "files"),
    ];
    aggregate_tool_groups(&mut messages);
    assert!(
        matches!(
            messages.as_slice(),
            [
                MessageViewModel::ToolCallGroup { .. },
                MessageViewModel::ToolBlock {
                    file_inspect: false,
                    ..
                },
                MessageViewModel::ToolCallGroup { .. }
            ]
        ),
        "未声明的 Bash 必须保留单条并打断分组"
    );
}

#[test]
fn test_aggregate_file_inspect_skips_only_empty_thinking() {
    let mut messages = vec![
        make_test_tool_block("Bash", true, ToolPhase::Finished, None, false, "files"),
        MessageViewModel::AssistantBubble {
            blocks: vec![ContentBlockView::Reasoning {
                text: String::new(),
                char_count: 0,
                duration_ms: None,
                action_summary: None,
                active: false,
                active_since: None,
                finished_at: None,
                hint: None,
            }],
            is_streaming: false,
            collapsed: false,
            content_hash: 0,
        },
        make_test_tool_block("Bash", true, ToolPhase::Finished, None, false, "files"),
    ];
    aggregate_tool_groups(&mut messages);
    assert!(
        matches!(messages.as_slice(), [MessageViewModel::ToolCallGroup { tools, .. }] if tools.len() == 2),
        "空思考仍可跨过，不能打断相邻文件查询分组"
    );
}

#[test]
fn test_tool_block_hash_includes_file_inspect() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let a = make_test_tool_block("Bash", false, ToolPhase::Finished, None, false, "files");
    let b = make_test_tool_block("Bash", true, ToolPhase::Finished, None, false, "files");
    let mut a_hasher = DefaultHasher::new();
    let mut b_hasher = DefaultHasher::new();
    a.hash(&mut a_hasher);
    b.hash(&mut b_hasher);
    assert_ne!(a, b, "仅 file_inspect 不同的工具块不能判为相等");
    assert_ne!(a_hasher.finish(), b_hasher.finish());
}

#[test]
fn test_history_rebuild_preserves_bash_file_inspect() {
    let result = BaseMessage::tool_result("bash", "files");
    let vm = MessageViewModel::from_base_message(
        &result,
        &[(
            "bash".to_string(),
            "Bash".to_string(),
            json!({"command":"ls", "file_inspect": true}),
        )],
    );
    assert!(matches!(
        vm,
        MessageViewModel::ToolBlock {
            file_inspect: true,
            ..
        }
    ));
}

#[test]
fn test_tool_color_new_names() {
    // 读取/搜索 — SAGE
    assert_eq!(tool_color("Read"), theme::cur().success);
    assert_eq!(tool_color("Glob"), theme::cur().success);
    assert_eq!(tool_color("Grep"), theme::cur().success);
    // 写入/编辑 — WARNING
    assert_eq!(tool_color("Write"), theme::cur().warning);
    assert_eq!(tool_color("Edit"), theme::cur().warning);
    // 执行 — BASH_BORDER
    assert_eq!(tool_color("Bash"), theme::cur().bash_border);
    // 代理/交互 — THINKING
    assert_eq!(tool_color("Agent"), theme::cur().thinking);
    assert_eq!(tool_color("AskUserQuestion"), theme::cur().thinking);
    assert_eq!(tool_color("TodoWrite"), theme::cur().thinking);
}

// ── aggregate_batch_groups 测试 ──

/// 创建一个已完成的单 agent SubAgentGroup VM
fn make_done_subagent(agent_id: &str, task: &str) -> MessageViewModel {
    let mut vm = MessageViewModel::SubAgentGroup {
        agent_id: agent_id.to_string(),
        task_preview: task.to_string(),
        total_steps: 3,
        recent_messages: Vec::new(),
        is_running: false,
        collapsed: false,
        final_result: Some("done".to_string()),
        is_error: false,
        is_background: false,
        bg_hash: Some("test01".to_string()),
        batch_agents: Vec::new(),
        instance_id: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}

/// 创建一个运行中的 SubAgentGroup VM
fn make_running_subagent(agent_id: &str, task: &str) -> MessageViewModel {
    let mut vm = MessageViewModel::SubAgentGroup {
        agent_id: agent_id.to_string(),
        task_preview: task.to_string(),
        total_steps: 0,
        recent_messages: Vec::new(),
        is_running: true,
        collapsed: false,
        final_result: None,
        is_error: false,
        is_background: false,
        bg_hash: Some("test02".to_string()),
        batch_agents: Vec::new(),
        instance_id: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}

#[test]
fn test_aggregate_batch_groups_single_agent_noop() {
    let mut vms = vec![make_done_subagent("explorer", "explore code")];
    aggregate_batch_groups(&mut vms);
    assert_eq!(vms.len(), 1, "单个 SubAgentGroup 不应聚合");
    // batch_agents 应保持为空
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
        assert!(batch_agents.is_empty());
    } else {
        panic!("应为 SubAgentGroup");
    }
}

#[test]
fn test_aggregate_batch_groups_consecutive_agents() {
    let mut vms = vec![
        make_done_subagent("agent-1", "task one"),
        make_done_subagent("agent-2", "task two"),
        make_done_subagent("agent-3", "task three"),
    ];
    aggregate_batch_groups(&mut vms);
    assert_eq!(vms.len(), 1, "3 个连续已完成 SubAgentGroup 应合并为 1 个");
    if let MessageViewModel::SubAgentGroup {
        batch_agents,
        collapsed,
        ..
    } = &vms[0]
    {
        assert_eq!(batch_agents.len(), 3);
        assert!(*collapsed, "合并后应默认折叠");
        assert_eq!(batch_agents[0].agent_id, "agent-1");
        assert_eq!(batch_agents[1].agent_id, "agent-2");
        assert_eq!(batch_agents[2].agent_id, "agent-3");
    } else {
        panic!("应为 SubAgentGroup");
    }
}

#[test]
fn test_aggregate_batch_groups_running_agent_skip() {
    let mut vms = vec![
        make_done_subagent("agent-1", "task one"),
        make_running_subagent("agent-2", "task two"),
        make_done_subagent("agent-3", "task three"),
    ];
    aggregate_batch_groups(&mut vms);
    // running agent 打断连续区间，不应聚合
    assert_eq!(vms.len(), 3, "中间有 is_running=true 时不合并");
}

#[test]
fn test_aggregate_batch_groups_mixed_batch() {
    // 3 完成 + 1 running + 2 完成 = 两个独立的聚合区间
    // 但由于 running 打断，每个区间只有 1 个或 2 个
    let mut vms = vec![
        make_done_subagent("agent-1", "task one"),
        make_done_subagent("agent-2", "task two"),
        make_done_subagent("agent-3", "task three"),
        make_running_subagent("agent-4", "task four"),
        make_done_subagent("agent-5", "task five"),
        make_done_subagent("agent-6", "task six"),
    ];
    aggregate_batch_groups(&mut vms);
    // 前 3 个合并为 1，running 保持独立，后 2 个合并为 1
    assert_eq!(vms.len(), 3, "3+1+2 = 两个聚合区间 + 1 个 running");
    // 第一个是合并后的
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
        assert_eq!(batch_agents.len(), 3);
    } else {
        panic!("第一个应为聚合的 SubAgentGroup");
    }
    // 第二个是 running 的
    if let MessageViewModel::SubAgentGroup { is_running, .. } = &vms[1] {
        assert!(*is_running);
    } else {
        panic!("第二个应为 running 的 SubAgentGroup");
    }
    // 第三个是合并后的
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[2] {
        assert_eq!(batch_agents.len(), 2);
    } else {
        panic!("第三个应为聚合的 SubAgentGroup");
    }
}

#[test]
fn test_aggregate_batch_groups_already_aggregated_skip() {
    // 已聚合的 SubAgentGroup（batch_agents 非空）不参与二次聚合
    let mut aggregated = MessageViewModel::SubAgentGroup {
        agent_id: "agent-1".to_string(),
        task_preview: "task one".to_string(),
        total_steps: 3,
        recent_messages: Vec::new(),
        is_running: false,
        collapsed: true,
        final_result: Some("done".to_string()),
        is_error: false,
        is_background: false,
        bg_hash: Some("batch01".to_string()),
        batch_agents: vec![
            AgentSummary {
                agent_id: "agent-1".to_string(),
                task_preview: "task one".to_string(),
                tool_count: 3,
                is_error: false,
                final_result: Some("done".to_string()),
            },
            AgentSummary {
                agent_id: "agent-2".to_string(),
                task_preview: "task two".to_string(),
                tool_count: 5,
                is_error: false,
                final_result: Some("done".to_string()),
            },
        ],
        instance_id: None,
        content_hash: 0,
    };
    aggregated.recompute_hash();
    let mut vms = vec![aggregated.clone()];
    aggregate_batch_groups(&mut vms);
    assert_eq!(vms.len(), 1, "已聚合的不应二次聚合");
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
        assert_eq!(batch_agents.len(), 2, "batch_agents 应保持不变");
    } else {
        panic!("应为 SubAgentGroup");
    }
}

#[test]
fn test_agent_summary_truncation() {
    let long_task = "这是一个非常非常非常非常非常非常非常非常非常非常非常非常长的任务描述需要超过五十个字符的长度才能触发截断逻辑验证";
    assert!(long_task.chars().count() > 50, "测试数据应超过 50 字符");
    let mut vms = vec![
        make_done_subagent("agent-1", long_task),
        make_done_subagent("agent-2", "short task"),
    ];
    aggregate_batch_groups(&mut vms);
    if let MessageViewModel::SubAgentGroup { batch_agents, .. } = &vms[0] {
        assert_eq!(
            batch_agents[0].task_preview.chars().count(),
            50,
            "task_preview 应截断到 50 字符"
        );
        assert_eq!(batch_agents[1].task_preview, "short task", "短文本不应截断");
    } else {
        panic!("应为 SubAgentGroup");
    }
}

#[test]
fn test_batch_group_default_collapsed() {
    let mut vms = vec![
        make_done_subagent("agent-1", "task one"),
        make_done_subagent("agent-2", "task two"),
    ];
    aggregate_batch_groups(&mut vms);
    if let MessageViewModel::SubAgentGroup { collapsed, .. } = &vms[0] {
        assert!(*collapsed, "合并后应默认折叠");
    } else {
        panic!("应为 SubAgentGroup");
    }
}

// ── content_hash 测试 ──

#[test]
fn test_content_hash_user_bubble_consistent() {
    let vm1 = MessageViewModel::user("Hello".to_string());
    let vm2 = MessageViewModel::user("Hello".to_string());
    assert_eq!(
        vm1.content_hash(),
        vm2.content_hash(),
        "相同内容的 hash 应一致"
    );
}

#[test]
fn test_content_hash_user_bubble_different() {
    let vm1 = MessageViewModel::user("Hello".to_string());
    let vm2 = MessageViewModel::user("World".to_string());
    assert_ne!(
        vm1.content_hash(),
        vm2.content_hash(),
        "不同内容的 hash 应不同"
    );
}

#[test]
fn test_content_hash_system_note() {
    let vm = MessageViewModel::system("test note".to_string());
    assert_ne!(vm.content_hash(), 0, "hash 不应为 0（几乎不可能碰撞）");
}

#[test]
fn test_content_hash_matches_compute_hash() {
    // 验证 content_hash 与 RenderTask::compute_hash 结果一致
    let vm = MessageViewModel::user("test content".to_string());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    vm.hash(&mut hasher);
    let expected = hasher.finish();
    assert_eq!(
        vm.content_hash(),
        expected,
        "content_hash 应与手动 compute_hash 一致"
    );
}

#[test]
fn test_content_hash_assistant_bubble_changes_on_append() {
    let mut vm = MessageViewModel::assistant();
    let hash1 = vm.content_hash();
    vm.append_chunk("new text");
    // append_chunk 内部会 recompute_hash
    assert_ne!(vm.content_hash(), hash1, "append_chunk 后 hash 应变化");
}

#[test]
fn test_content_hash_assistant_bubble_changes_on_streaming() {
    let mut vm = MessageViewModel::assistant();
    let hash_streaming = vm.content_hash();
    // 模拟 is_streaming 变化
    if let MessageViewModel::AssistantBubble { is_streaming, .. } = &mut vm {
        *is_streaming = false;
    }
    vm.recompute_hash();
    assert_ne!(
        vm.content_hash(),
        hash_streaming,
        "is_streaming 变化后 hash 应不同"
    );
}

#[test]
fn test_content_hash_subagent_group() {
    let vm = MessageViewModel::subagent_group("explorer".to_string(), "explore".to_string());
    assert_ne!(vm.content_hash(), 0);
}

#[test]
fn test_content_hash_cache_warning() {
    let vm = MessageViewModel::cache_warning("low cache".to_string());
    assert_ne!(vm.content_hash(), 0);
}

#[test]
fn test_recompute_hash_idempotent() {
    let mut vm = MessageViewModel::user("stable".to_string());
    let h1 = vm.content_hash();
    vm.recompute_hash();
    let h2 = vm.content_hash();
    assert_eq!(h1, h2, "未修改内容时 recompute_hash 应幂等");
}

#[test]
fn test_human_message_with_system_reminder_detection() {
    let text = "<system-reminder>\n此会话从之前的对话延续。\n## Summary\nDone.\n[Context compacted; continue based on the summary]\n</system-reminder>";
    let msg = BaseMessage::human(text);
    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::UserBubble {
            system_reminder,
            content,
            ..
        } => {
            assert!(system_reminder, "应识别为系统提醒");
            assert!(
                !content.contains("<system-reminder>"),
                "标签应被剥离: {}",
                content
            );
            assert!(!content.contains("</system-reminder>"), "结束标签应被剥离");
            assert!(content.contains("## Summary"), "正文应保留");
            assert!(content.contains("[Context compacted"), "续接指令应保留");
        }
        _ => panic!("应为 UserBubble"),
    }
}

#[test]
fn test_human_background_shell_notification_renders_as_system_note() {
    let text = "<system-reminder>\n<background-task-completed>\n<task-id>abc</task-id>\n<command>npm test</command>\n<status>completed (exit 0)</status>\n<output>/tmp/abc.output</output>\n</background-task-completed>\n</system-reminder>";
    let msg = BaseMessage::human(text);
    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::SystemNote { content, .. } => {
            // 语言无关：具体前缀文案随进程语言（全局注册表）而定，此处只验证
            // 已转换为可读系统提示（保留命令/状态、不泄露 XML）。
            assert!(content.contains("npm test"), "应保留命令摘要: {}", content);
            assert!(
                content.contains("exit code 0") || content.contains("退出码 0"),
                "应保留退出码: {}",
                content
            );
            assert!(
                !content.contains("(completed ("),
                "状态不应再被包成双层括号: {}",
                content
            );
            assert!(
                !content.contains("<background-task-completed>"),
                "不应泄露 XML 标签: {}",
                content
            );
        }
        _ => panic!("后台 shell 通知应渲染为 SystemNote"),
    }
}

#[test]
fn test_message_view_constructors_convert_background_shell_xml_to_system_note() {
    let text = "<background-task-completed><task-id>abc</task-id><command>git fetch</command><status>completed (exit 0)</status><output>/tmp/abc.output</output></background-task-completed>";
    let cases = [
        MessageViewModel::user(text.to_string()),
        MessageViewModel::system(format!("<system-reminder>{}</system-reminder>", text)),
    ];
    for vm in cases {
        match vm {
            MessageViewModel::SystemNote { content, .. } => {
                // 语言无关（见上）：验证已转换为可读提示并保留命令、不泄露 XML。
                assert!(content.contains("git fetch"), "应保留命令摘要: {}", content);
                assert!(
                    content.contains("exit code 0") || content.contains("退出码 0"),
                    "应保留退出码: {}",
                    content
                );
                assert!(
                    !content.contains("(completed ("),
                    "状态不应再被包成双层括号: {}",
                    content
                );
                assert!(
                    !content.contains("<background-task-completed>"),
                    "不应泄露 XML 标签: {}",
                    content
                );
            }
            _ => panic!("后台 shell XML 构造时应转换为 SystemNote"),
        }
    }
}

#[test]
fn test_human_message_without_system_reminder() {
    let text = "普通用户消息";
    let msg = BaseMessage::human(text);
    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::UserBubble {
            system_reminder, ..
        } => {
            assert!(!system_reminder, "普通消息不应标记为系统提醒");
        }
        _ => panic!("应为 UserBubble"),
    }
}

#[test]
fn test_system_reminder_partial_eq_different() {
    let msg1 = BaseMessage::human("普通消息");
    let msg2 = BaseMessage::human("<system-reminder>\n压缩摘要\n</system-reminder>");
    let vm1 = MessageViewModel::from_base_message(&msg1, &[]);
    let vm2 = MessageViewModel::from_base_message(&msg2, &[]);
    assert_ne!(vm1, vm2, "系统提醒消息和普通消息不应相等");
}

#[test]
fn test_system_reminder_content_hash_different() {
    let msg1 = BaseMessage::human("普通消息");
    let msg2 = BaseMessage::human("<system-reminder>\n压缩摘要\n</system-reminder>");
    let vm1 = MessageViewModel::from_base_message(&msg1, &[]);
    let vm2 = MessageViewModel::from_base_message(&msg2, &[]);
    assert_ne!(
        vm1.content_hash(),
        vm2.content_hash(),
        "content_hash 应不同"
    );
}

/// 回归：`Agent` 工具调用的类型标签必须反映**真实**模式。
///
/// 真机症状：模型漏传 `subagent_type` 导致调用失败，标题却硬编码成 `Agent(fork)`，
/// 下面紧跟 `Error: please provide subagent_type parameter`——自相矛盾。
/// fork 本来就是一种子 Agent，只有真的走 fork 才配得上 "fork" 标签。
#[test]
fn test_agent_group_label_reflects_real_mode() {
    let cases = [
        (
            json!({"prompt": "查需求"}),
            "unknown",
            "漏参 → 不得冒充 fork",
        ),
        (
            json!({"prompt": "查需求", "fork": true}),
            "fork",
            "fork: true 才是 fork",
        ),
        (
            json!({"prompt": "查需求", "subagent_type": "fork"}),
            "fork",
            "subagent_type=fork 等价",
        ),
        (
            json!({"prompt": "查需求", "subagent_type": "general-purpose"}),
            "general-purpose",
            "显式类型原样展示",
        ),
    ];
    for (index, (input, expected, message)) in cases.into_iter().enumerate() {
        let tool_call_id = format!("toolu_label_{index}");
        let prev = vec![(tool_call_id.clone(), "Agent".to_string(), input)];
        let vm = MessageViewModel::from_base_message(
            &BaseMessage::tool_error(&tool_call_id, "boom"),
            &prev,
        );
        match vm {
            MessageViewModel::SubAgentGroup {
                agent_id, is_error, ..
            } => {
                assert_eq!(agent_id, expected, "{message}");
                assert!(is_error, "工具报错时分组必须标记为失败");
            }
            other => panic!("应为 SubAgentGroup: {other:?}"),
        }
    }
}
#[test]
fn test_user_image_paths_affect_render_hash_and_equality() {
    let mut message = MessageViewModel::user("[Image #1]".into());
    message.set_image_ids(vec![1]);
    let original = message.clone();
    let original_hash = message.content_hash();
    let paths = [(1, std::env::temp_dir().join("image.png"))]
        .into_iter()
        .collect();
    message.set_image_paths(&paths);
    assert_ne!(
        message.content_hash(),
        original_hash,
        "路径改变后必须使渲染缓存失效"
    );
    assert_ne!(message, original, "链接有无属于可见内容差异");
    let linked_hash = message.content_hash();
    message.set_image_paths(&paths);
    assert_eq!(
        message.content_hash(),
        linked_hash,
        "同一路径快照应保持稳定"
    );
    message.set_image_paths(&Default::default());
    assert_eq!(message, original, "清索引后必须清掉旧链接");
    assert_eq!(message.content_hash(), original_hash);
}
#[test]
fn test_handwritten_image_placeholder_cannot_link_to_an_earlier_attachment() {
    let paths = [(1, std::env::temp_dir().join("earlier.png"))]
        .into_iter()
        .collect();
    let mut message = MessageViewModel::user("手写 [Image #1]".into());
    message.set_image_paths(&paths);
    assert!(
        matches!(message, MessageViewModel::UserBubble { ref image_paths, .. } if image_paths.is_empty()),
        "已有同编号图片也不能让手写文本误链到旧文件"
    );
}

#[test]
fn test_rebuilt_user_message_only_links_identifiers_with_real_image_blocks() {
    let paths = [
        (1, std::env::temp_dir().join("one.png")),
        (2, std::env::temp_dir().join("two.png")),
    ]
    .into_iter()
    .collect();
    for text in ["真实截图 [Image #2]", "重复引用 [Image #2] [Image #2]"] {
        let source = BaseMessage::human(cc_agent::messages::MessageContent::Blocks(vec![
            ContentBlock::text(text),
            ContentBlock::image_base64("image/png", "aW1hZ2U="),
        ]));
        let mut message = MessageViewModel::from_base_message(&source, &[]);
        message.set_image_paths(&paths);
        assert!(
            matches!(message, MessageViewModel::UserBubble { ref image_paths, .. } if image_paths.len() == 1 && image_paths[0].0 == 2),
            "从状态快照重建时应保留真实图片编号并合并重复引用"
        );
    }
    let source = BaseMessage::human("手写 [Image #1]");
    let mut message = MessageViewModel::from_base_message(&source, &[]);
    message.set_image_paths(&paths);
    assert!(
        matches!(message, MessageViewModel::UserBubble { ref image_paths, .. } if image_paths.is_empty())
    );
}

#[test]
fn test_is_readonly_shell_command_detects_file_inspection() {
    let yes = [
        "grep -rn \"x\" app/ | head -5",
        "cd /work/project && sed -n '159,260p' app/a.php | grep -n needle",
        "grep -rn A app/ ; echo '---' ; grep -rn B app/",
        "ls -la",
        "find . -name '*.rs'",
        "wc -l file",
        "VAR=1 grep x file",
        "cd /tmp && cat a.txt",
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
    let no = [
        "cargo build -p cc-tui",
        "git show origin/test:app/x.php",
        "cd /work/project && git status",
        "cat a.txt > b.txt",
        "echo hello",
        "cd /tmp",
        "./target/debug/cc-code --version",
        "grep x f | xargs rm",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
}

#[test]
fn test_is_readonly_shell_command_rejects_write_variants() {
    let no = [
        "sed -n -i 's/a/b/' f",
        "find . -name x -delete",
        r"find . -name x -exec rm {} \;",
        "sort -o out.txt in.txt",
        "cat \"$(rm -f important)\"",
        "grep x f | tee out.txt",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
    let yes = ["sed -n '1,10p' f", "find . -name '*.rs'", "sort in.txt | uniq"];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
}

/// 回归：`2>/dev/null`、`2>&1`、`>&2` 只丢弃/合并 stderr，不是写盘重定向。
#[test]
fn test_is_readonly_shell_command_allows_stderr_redirect() {
    let yes = [
        "grep -rn foo app/ 2>/dev/null | head -5",
        "find . -type f -name '*.sql' 2>/dev/null | head",
        "ls -la 2> /dev/null",
        "cat missing.txt 2>&1 | head -3",
        "grep -n needle f >&2",
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
    // 同一段里再带上真正的写盘重定向，仍须拒绝
    let no = [
        "grep -rn foo app/ 2>/dev/null > out.txt",
        "ls 2>&1 >> log.txt",
        "find . -name '*.rs' 2>/dev/null | xargs rm",
    ];
    for cmd in no {
        assert!(!is_readonly_shell_command(cmd), "不应判定为只读查看: {cmd}");
    }
}

/// 回归：引号内的 `;`、`|`、`&&` 是参数内容，不能当命令分隔符拆开。
#[test]
fn test_is_readonly_shell_command_ignores_quoted_separators() {
    let yes = [
        r#"grep -n "expand\|shell command\|Ran " cc-tui/src/ui/message_render/mod.rs | head -30"#,
        r#"awk 'NR>=2900 && NR<=2960' f | grep -n "fn \|detail_mode""#,
        r#"grep -rn 'a;b' app/ | head -5"#,
        r#"grep -F 'foo && bar' README.md"#,
        r#"grep -n 'a > b' notes.md"#,
    ];
    for cmd in yes {
        assert!(is_readonly_shell_command(cmd), "应判定为只读查看: {cmd}");
    }
}
