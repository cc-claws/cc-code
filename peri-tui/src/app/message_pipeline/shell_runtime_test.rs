use super::*;

fn make_bash(id: &str) -> MessageViewModel {
    MessageViewModel::tool_block_with_id(
        id.into(),
        "Bash".into(),
        "Bash".into(),
        Some("same command".into()),
        false,
    )
}

#[test]
fn test_shell_runtime_routes_same_call_id_by_agent_not_command() {
    let mut pipeline = MessagePipeline::new(".".into());
    let parent_start = Instant::now() - std::time::Duration::from_secs(3);
    let child_start = Instant::now() - std::time::Duration::from_secs(5);
    // 登记早于工具事件：overlay 不能依赖当时已有对应 VM。
    pipeline.register_shell_runtime(None, "same-id", parent_start, 300_000);
    pipeline.register_shell_runtime(Some("child"), "same-id", child_start, 600_000);
    let mut child = MessageViewModel::subagent_group("general-purpose".into(), "task".into());
    if let MessageViewModel::SubAgentGroup {
        instance_id,
        recent_messages,
        ..
    } = &mut child
    {
        *instance_id = Some("child".into());
        recent_messages.push(make_bash("same-id"));
    }
    let mut messages = vec![make_bash("same-id"), make_bash("different-id"), child];
    assert!(pipeline.apply_shell_runtime(&mut messages));
    assert!(
        matches!(&messages[0], MessageViewModel::ToolBlock { started_at: Some(t), execution_timeout_ms: Some(300_000), shell_backgrounded: false, .. } if *t == parent_start)
    );
    assert!(
        matches!(
            &messages[1],
            MessageViewModel::ToolBlock {
                started_at: None,
                ..
            }
        ),
        "相同文本不能串位"
    );
    let MessageViewModel::SubAgentGroup {
        recent_messages, ..
    } = &messages[2]
    else {
        panic!("应为子 Agent")
    };
    assert!(
        matches!(&recent_messages[0], MessageViewModel::ToolBlock { started_at: Some(t), execution_timeout_ms: Some(600_000), shell_backgrounded: false, .. } if *t == child_start)
    );
    assert!(
        !pipeline.apply_shell_runtime(&mut messages),
        "重复应用不能造成无意义重绘"
    );
}

#[test]
fn test_shell_runtime_background_handoff_reaches_exact_tool_call() {
    let mut pipeline = MessagePipeline::new(".".into());
    let started_at = Instant::now() - std::time::Duration::from_secs(3);
    pipeline.register_shell_runtime(None, "running-call", started_at, 600_000);
    let mut messages = vec![make_bash("running-call"), make_bash("other-call")];
    assert!(pipeline.apply_shell_runtime(&mut messages));
    assert!(pipeline.set_shell_runtime_backgrounded(None, "running-call", true));
    assert!(pipeline.apply_shell_runtime(&mut messages));
    assert!(matches!(
        &messages[0],
        MessageViewModel::ToolBlock {
            shell_backgrounded: true,
            execution_timeout_ms: Some(600_000),
            ..
        }
    ));
    assert!(
        matches!(
            &messages[1],
            MessageViewModel::ToolBlock {
                shell_backgrounded: false,
                started_at: None,
                ..
            }
        ),
        "后台状态不能串到其他 Bash 调用"
    );
    let mut marker_message = make_bash("running-call");
    if let MessageViewModel::ToolBlock { content, .. } = &mut marker_message {
        *content = "<background-task-started><task-id>abc-123</task-id><command>sleep 30</command><output>C:/tmp/abc-123.log</output></background-task-started>".into();
    }
    messages[0] = marker_message;
    assert!(
        pipeline.apply_shell_runtime(&mut messages),
        "工具返回后台 marker 后应继续应用后台状态"
    );
    assert!(matches!(
        &messages[0],
        MessageViewModel::ToolBlock {
            shell_backgrounded: true,
            ..
        }
    ));
    assert!(pipeline.set_shell_runtime_backgrounded(None, "running-call", false));
    assert!(pipeline.apply_shell_runtime(&mut messages));
    assert!(
        matches!(
            &messages[0],
            MessageViewModel::ToolBlock {
                shell_backgrounded: false,
                ..
            }
        ),
        "命令结束后应撤销运行中后台标记"
    );
}

#[test]
fn test_shell_runtime_does_not_change_finished_tool() {
    let mut pipeline = MessagePipeline::new(".".into());
    pipeline.register_shell_runtime(None, "done", Instant::now(), 5000);
    let mut vm = make_bash("done");
    if let MessageViewModel::ToolBlock { content, .. } = &mut vm {
        *content = "done".into();
    }
    assert!(!pipeline.apply_shell_runtime(&mut [vm]));
}

#[test]
fn test_shell_runtime_restores_live_direct_background_after_thread_reopen() {
    let mut pipeline = MessagePipeline::new(".".into());
    let started_at = Instant::now() - std::time::Duration::from_secs(5);
    let mut vm = make_bash("direct-background");
    if let MessageViewModel::ToolBlock { content, .. } = &mut vm {
        *content = "<background-task-started><task-id>task-1</task-id><command>sleep 30</command><output>C:/tmp/task-1.output</output></background-task-started>".into();
    }
    assert!(pipeline.sync_shell_runtime(None, "direct-background", started_at, 600_000, true));
    assert!(pipeline.apply_shell_runtime(std::slice::from_mut(&mut vm)));
    assert!(matches!(
        &vm,
        MessageViewModel::ToolBlock {
            started_at: Some(actual_started),
            execution_timeout_ms: Some(600_000),
            shell_backgrounded: true,
            ..
        } if *actual_started == started_at
    ));

    // 切换线程会清空 pipeline 元数据；活跃 slot 下一轮 poll 应能重新提供完整状态。
    pipeline.clear();
    assert!(pipeline.sync_shell_runtime(None, "direct-background", started_at, 600_000, true));
    let mut restored = make_bash("direct-background");
    if let MessageViewModel::ToolBlock { content, .. } = &mut restored {
        *content = "<background-task-started><task-id>task-1</task-id><command>sleep 30</command><output>C:/tmp/task-1.output</output></background-task-started>".into();
    }
    assert!(pipeline.apply_shell_runtime(std::slice::from_mut(&mut restored)));
    assert!(matches!(
        restored,
        MessageViewModel::ToolBlock {
            started_at: Some(actual_started),
            execution_timeout_ms: Some(600_000),
            shell_backgrounded: true,
            ..
        } if actual_started == started_at
    ));
}
