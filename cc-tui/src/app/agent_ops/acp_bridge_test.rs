//! 回归：ACP Write 快照与 `peri/subagent_lifecycle` 通知必须真正驱动界面。
//!
//! Write 测试在实际写盘完成后才消费开始事件，验证预览不依赖界面读盘时序。
//! 真机症状：状态栏没有 `↓ to view`、底栏无抽屉、消息区无 `(↓ to manage)` 摘要。
//! 根因在 ACP 客户端漏认方法名（已修），此用例守住「通知 → background_agents」
//! 这一段：只要 `AcpNotification::Peri { method: "peri/subagent_lifecycle" }` 到达，
//! 后台子 Agent 就必须登记进抽屉。

use crate::acp_client::AcpNotification;
use crate::app::App;

fn make_tool_phase(app: &App, id: &str) -> crate::ui::message_view::ToolPhase {
    app.session_mgr
        .current()
        .messages
        .pipeline
        .build_tail_vms()
        .into_iter()
        .find_map(|vm| match vm {
            crate::ui::message_view::MessageViewModel::ToolBlock {
                tool_call_id,
                phase,
                ..
            } if tool_call_id == id => Some(phase),
            _ => None,
        })
        .expect("应保留对应工具块")
}

fn make_pending_tool(app: &mut App, id: &str) {
    let event = cc_agent::agent::AgentEvent::ToolPending {
        message_id: cc_agent::messages::MessageId::new(),
        tool_call_id: id.into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "git status", "timeout": 300000}),
        source_agent_id: None,
    };
    for mapped in cc_acp::event::map_event(&event, 200_000) {
        for update in mapped.updates {
            app.handle_session_update_peri(&serde_json::json!({"update": update}));
        }
    }
}

fn make_routed_tool_event(app: &mut App, event: &cc_agent::agent::AgentEvent) {
    for mapped in cc_acp::event::map_event(event, 200_000) {
        for update in mapped.updates {
            let params = cc_acp::session::subagent_protocol::session_update_payload(
                "streaming-session",
                &update,
                mapped.source_agent_id.as_deref(),
            )
            .expect("真实 ACP 映射应可序列化");
            app.handle_session_update_peri(&params);
        }
    }
}

fn make_live_tool_output(
    app: &App,
    id: &str,
) -> Option<crate::ui::message_view::ToolOutputPreview> {
    app.session_mgr
        .current()
        .messages
        .pipeline
        .build_tail_vms()
        .into_iter()
        .find_map(|vm| match vm {
            crate::ui::message_view::MessageViewModel::ToolBlock {
                tool_call_id,
                live_output,
                ..
            } if tool_call_id == id => Some(live_output),
            _ => None,
        })
        .expect("应存在对应工具块")
}

#[tokio::test]
async fn test_acp_tool_output_updates_running_preview_and_final_result_clears_it() {
    use crate::ui::message_view::{MessageViewModel, ToolPhase};
    use cc_agent::agent::AgentEvent;
    use cc_agent::messages::MessageId;
    let (mut app, _handle) = App::new_headless(120, 30).await;
    let message_id = MessageId::new();
    make_routed_tool_event(
        &mut app,
        &AgentEvent::ToolStart {
            message_id,
            tool_call_id: "live-command".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": "echo progress", "timeout": 300000}),
            file_snapshot: None,
            source_agent_id: None,
        },
    );
    let output = AgentEvent::ToolOutput {
        message_id,
        tool_call_id: "live-command".into(),
        tail: "progress\n".into(),
        total_lines: 1,
        total_bytes: 9,
        elapsed_ms: 2100,
        source_agent_id: None,
    };
    make_routed_tool_event(&mut app, &output);
    let preview = make_live_tool_output(&app, "live-command").expect("进度应写入展示缓存");
    assert_eq!(preview.tail, "progress\n");
    assert_eq!(preview.total_lines, 1);
    assert_eq!(preview.total_bytes, 9);
    assert_eq!(preview.elapsed_ms, 2100);
    assert_eq!(make_tool_phase(&app, "live-command"), ToolPhase::Running);
    make_routed_tool_event(
        &mut app,
        &AgentEvent::ToolEnd {
            message_id,
            tool_call_id: "live-command".into(),
            name: "Bash".into(),
            output: "complete result\n".into(),
            is_error: false,
            source_agent_id: None,
        },
    );
    assert_eq!(make_tool_phase(&app, "live-command"), ToolPhase::Finished);
    assert!(
        make_live_tool_output(&app, "live-command").is_none(),
        "完成结果应清除临时尾缓存"
    );
    make_routed_tool_event(&mut app, &output);
    assert!(
        make_live_tool_output(&app, "live-command").is_none(),
        "迟到进度不能恢复已完成缓存"
    );
    assert!(app.session_mgr.current().messages.pipeline.build_tail_vms().iter().any(|vm|
        matches!(vm, MessageViewModel::ToolBlock { content, phase: ToolPhase::Finished, .. }
            if content == "complete result\n")), "最终工具结果必须独立保留");
}

#[tokio::test]
async fn test_acp_tool_output_preserves_child_source_with_same_call_id() {
    use crate::ui::message_view::MessageViewModel;
    use cc_agent::agent::AgentEvent;
    use cc_agent::messages::MessageId;
    let (mut app, _handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "streaming-session".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "event": "started", "instanceId": "streaming-child",
            "agentType": "general-purpose", "taskPreview": "输出隔离", "isBackground": true,
        }),
    });
    for (source, tail) in [
        (None, "main progress\n"),
        (Some("streaming-child"), "child progress\n"),
    ] {
        let message_id = MessageId::new();
        make_routed_tool_event(
            &mut app,
            &AgentEvent::ToolStart {
                message_id,
                tool_call_id: "same-command".into(),
                name: "Bash".into(),
                input: serde_json::json!({"command": "echo progress"}),
                file_snapshot: None,
                source_agent_id: source.map(String::from),
            },
        );
        make_routed_tool_event(
            &mut app,
            &AgentEvent::ToolOutput {
                message_id,
                tool_call_id: "same-command".into(),
                tail: tail.into(),
                total_lines: 1,
                total_bytes: tail.len(),
                elapsed_ms: 2100,
                source_agent_id: source.map(String::from),
            },
        );
    }
    assert_eq!(
        make_live_tool_output(&app, "same-command")
            .expect("主缓存应存在")
            .tail,
        "main progress\n"
    );
    let child = app
        .session_mgr
        .current()
        .subagent_sessions
        .get("streaming-child")
        .expect("子会话应存在");
    assert!(
        child.pipeline.build_tail_vms().iter().any(
            |vm| matches!(vm, MessageViewModel::ToolBlock { live_output: Some(output), .. }
            if output.tail == "child progress\n")
        ),
        "真实 ACP 来源扩展应精确路由到子会话"
    );
}

#[tokio::test]
async fn test_acp_pending_and_start_update_one_tool_block() {
    use crate::ui::message_view::{MessageViewModel, ToolPhase};
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_pending_tool(&mut app, "command");
    assert_eq!(make_tool_phase(&app, "command"), ToolPhase::Queued);
    assert!(
        app.session_mgr.current().agent.interaction_prompt.is_none(),
        "自动判定中不创建审批弹窗"
    );
    app.handle_session_update_peri(&serde_json::json!({
        "update": {"sessionUpdate": "tool_call", "toolCallId": "command", "title": "Bash",
            "status": "in_progress", "rawInput": {"command": "git status", "timeout": 300000}}
    }));
    assert_eq!(make_tool_phase(&app, "command"), ToolPhase::Running);
    let tools: Vec<_> = app
        .session_mgr
        .current()
        .messages
        .pipeline
        .build_tail_vms()
        .into_iter()
        .filter(|vm| matches!(vm, MessageViewModel::ToolBlock { .. }))
        .collect();
    assert_eq!(tools.len(), 1, "同批排队与开始不能产生重复工具行");
    assert!(matches!(
        &tools[0],
        MessageViewModel::ToolBlock {
            started_at: Some(_),
            timeout_ms: Some(300000),
            ..
        }
    ));
}

#[tokio::test]
async fn test_acp_permission_only_marks_requested_tool() {
    use crate::ui::message_view::ToolPhase;
    let (mut app, _handle) = App::new_headless(120, 30).await;
    app.session_mgr.current_mut().ui.loading = true;
    for id in ["first", "second", "third"] {
        make_pending_tool(&mut app, id);
    }
    app.handle_acp_request_permission(
        cc_acp::transport::types::RequestId::Number(1),
        serde_json::json!({
            "sessionId": "main",
            "toolCall": {"toolCallId": "second", "title": "Bash", "rawInput": {"command": "git status"}},
            "options": [{"optionId": "allow_once", "name": "Allow", "kind": "allow_once"}]
        }),
    );
    assert!(
        app.session_mgr.current().agent.interaction_prompt.is_some(),
        "Ask 时才创建人工审批弹窗"
    );
    assert_eq!(make_tool_phase(&app, "first"), ToolPhase::Queued);
    assert_eq!(make_tool_phase(&app, "second"), ToolPhase::AwaitingApproval);
    assert_eq!(make_tool_phase(&app, "third"), ToolPhase::Queued);
}

#[tokio::test]
async fn test_acp_pending_rejection_finishes_without_start() {
    use crate::ui::message_view::{MessageViewModel, ToolPhase};
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_pending_tool(&mut app, "rejected");
    app.handle_session_update_peri(&serde_json::json!({
        "update": {"sessionUpdate": "tool_call_update", "toolCallId": "rejected", "title": "Bash",
            "status": "failed", "rawOutput": "interrupted by user"}
    }));
    assert_eq!(make_tool_phase(&app, "rejected"), ToolPhase::Finished);
    assert!(app.session_mgr.current().messages.pipeline.build_tail_vms().iter().any(|vm|
        matches!(vm, MessageViewModel::ToolBlock { is_error: true, content, .. } if content == "interrupted by user")));
    assert!(
        app.session_mgr.current().agent.running_tools.is_empty(),
        "拒绝后不能残留运行态"
    );
}

#[tokio::test]
async fn test_acp_bash_result_preserves_full_output_for_decorator() {
    use crate::ui::message_view::MessageViewModel;
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_pending_tool(&mut app, "long-output");
    let output = "详细输出 https://example.com\n".repeat(20);
    app.handle_session_update_peri(&serde_json::json!({
        "update": {"sessionUpdate": "tool_call_update", "toolCallId": "long-output", "title": "Bash",
            "status": "completed", "rawOutput": output}
    }));
    assert!(
        app.session_mgr
            .current()
            .messages
            .pipeline
            .build_tail_vms()
            .iter()
            .any(
                |vm| matches!(vm, MessageViewModel::ToolBlock { content, .. } if content == &output)
            ),
        "结果折叠应由 OutputLine 负责，不能提前丢失原始行"
    );
}

async fn make_delayed_write_preview(
    old_content: Option<&str>,
    content: &str,
    append: bool,
) -> Option<cc_widgets::DiffInput> {
    use crate::app::message_pipeline::MessagePipeline;
    use crate::ui::message_view::MessageViewModel;
    use cc_agent::agent::{
        events::FnEventHandler,
        react::{AgentInput, Reasoning, ToolCall},
        state::{AgentState, State},
        ReActAgent,
    };
    use cc_agent::llm::MockLLM;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preview.txt");
    if let Some(old) = old_content {
        std::fs::write(&path, old).unwrap();
    }
    let call_id = uuid::Uuid::now_v7().to_string();
    let input = serde_json::json!({"file_path": path, "content": content, "append": append});
    let events = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured = events.clone();
    let agent = ReActAgent::new(MockLLM::new(vec![
        Reasoning::with_tools(
            "write",
            vec![ToolCall::new(&call_id, "Write", input.clone())],
        ),
        Reasoning::with_answer("done", "done"),
    ]))
    .add_middleware(Box::new(
        cc_middlewares::middleware::filesystem::FilesystemMiddleware::new(),
    ))
    .with_event_handler(std::sync::Arc::new(FnEventHandler(move |event| {
        captured.lock().push(event)
    })));
    let mut state = AgentState::new(dir.path().to_string_lossy());
    agent
        .execute(AgentInput::text("write"), &mut state, None)
        .await
        .expect("真实 Write 应执行成功");
    let expected = if append {
        format!("{}{content}", old_content.unwrap_or(""))
    } else {
        content.to_string()
    };
    assert_eq!(
        std::fs::read_to_string(&path).expect("应已完成实际写盘"),
        expected
    );
    let (mut app, _handle) = App::new_headless(120, 30).await;
    // 故意等写盘完成后才消费开始事件，确定性复现实际队列时序。
    for event in events.lock().iter() {
        for mapped in cc_acp::event::map_event(event, 200_000) {
            for update in mapped.updates {
                let notification = serde_json::json!({"update": update});
                if notification["update"]["sessionUpdate"] == "tool_call" {
                    assert_eq!(
                        notification["update"]["rawInput"], input,
                        "快照不能混入工具入参"
                    );
                }
                app.handle_session_update_peri(&notification);
            }
        }
    }
    let find_preview = |vms: Vec<MessageViewModel>| {
        vms.into_iter()
            .find_map(|vm| match vm {
                MessageViewModel::ToolBlock {
                    tool_call_id,
                    diff_input,
                    ..
                } if tool_call_id == call_id => Some(diff_input),
                _ => None,
            })
            .expect("应保留 Write 结果块")
    };
    let live = find_preview(app.session_mgr.current().messages.pipeline.build_tail_vms());
    let saved_call = state
        .messages()
        .iter()
        .flat_map(|message| message.tool_calls())
        .find(|call| call.id == call_id)
        .expect("历史应保留工具调用");
    assert_eq!(saved_call.arguments, input, "快照不能混入模型历史");
    let restored = find_preview(MessagePipeline::messages_to_view_models(
        state.messages(),
        dir.path().to_str().unwrap(),
    ));
    assert_eq!(live, restored, "StateSnapshot 重建必须保留相同预览");
    live
}

#[tokio::test]
async fn test_write_preview_delayed_start_preserves_created_content() {
    let preview = make_delayed_write_preview(None, "你好\nworld\n", false)
        .await
        .expect("新建文件应展示内容，即使开始事件消费晚于写盘");
    assert!(preview.is_new_file, "文件原本不存在，应显示新建预览");
    assert_eq!(preview.new_content, "你好\nworld\n");
}

#[tokio::test]
async fn test_write_preview_delayed_start_preserves_overwritten_content() {
    let preview = make_delayed_write_preview(Some("old\n"), "new\n", false)
        .await
        .expect("覆盖文件应保留真实 diff");
    assert!(!preview.is_new_file, "覆盖文件不能误判为新建");
    assert_eq!(preview.old_content, "old\n");
    assert_eq!(preview.new_content, "new\n");
}

#[tokio::test]
async fn test_write_preview_delayed_start_does_not_duplicate_appended_content() {
    let preview = make_delayed_write_preview(Some("a\n"), "b\n", true)
        .await
        .expect("追加文件应保留真实 diff");
    assert_eq!(preview.old_content, "a\n");
    assert_eq!(preview.new_content, "a\nb\n", "追加内容只能出现一次");
}

#[tokio::test]
async fn test_write_preview_delayed_start_hides_unchanged_content() {
    assert!(
        make_delayed_write_preview(Some("same\n"), "same\n", false)
            .await
            .is_none(),
        "未改变的文件仍只展示结果行"
    );
}

#[tokio::test]
async fn test_subagent_lifecycle_started_populates_drawer() {
    let (mut app, _handle) = App::new_headless(120, 30).await;

    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1",
            "event": "started",
            "instanceId": "child-1",
            "agentType": "general-purpose",
            "taskPreview": "查询武汉天气",
            "isBackground": true,
        }),
    });

    let session = app.session_mgr.current();
    assert_eq!(
        session.background_agents.len(),
        1,
        "后台子 Agent 应登记进底栏抽屉（否则状态栏不会出现 ↓ to view）"
    );
    assert!(
        session.subagent_sessions.contains_key("child-1"),
        "抽屉会话应同时建立，供视口切换使用"
    );
    assert_eq!(session.background_agents[0].agent_name, "general-purpose");
}

#[tokio::test]
async fn test_subagent_lifecycle_stopped_clears_drawer_entry() {
    let (mut app, _handle) = App::new_headless(120, 30).await;

    for (event, extra) in [
        ("started", serde_json::json!({ "isBackground": true })),
        ("stopped", serde_json::json!({ "result": "LGTM" })),
    ] {
        let mut params = serde_json::json!({
            "sessionId": "s1",
            "event": event,
            "instanceId": "child-1",
            "agentType": "general-purpose",
            "taskPreview": "查询武汉天气",
        });
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        app.handle_acp_notification(AcpNotification::Peri {
            session_id: "s1".into(),
            method: "peri/subagent_lifecycle".into(),
            params,
        });
    }

    let session = app.session_mgr.current();
    assert_eq!(
        session
            .subagent_sessions
            .get("child-1")
            .map(|sub| sub.status),
        Some(crate::app::subagent_session::SubagentRunStatus::Completed),
        "停止通知应把抽屉会话标记为 Completed"
    );
}

/// 诊断：子 Agent 视口的工具统计是否独立（不应为空、也不应污染主视图）。
#[tokio::test]
async fn test_subagent_viewport_tool_stats_isolation() {
    use crate::app::AgentEvent;

    let (mut app, _handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });

    // 主 Agent 自己的一次工具调用
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "m1".into(),
        name: "Grep".into(),
        display: "Grep".into(),
        args: "x".into(),
        input: serde_json::json!({}),
        source_agent_id: None,
    });
    app.push_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "m1".into(),
        name: "Grep".into(),
        output: "ok".into(),
        is_error: false,
        source_agent_id: None,
    });

    // 子 Agent 自己的工具调用
    for i in 0..3 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("c{i}"),
            name: "Read".into(),
            display: "Read".into(),
            args: "y".into(),
            input: serde_json::json!({}),
            source_agent_id: Some("child-1".into()),
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("c{i}"),
            name: "Read".into(),
            output: "ok".into(),
            is_error: false,
            source_agent_id: Some("child-1".into()),
        });
    }
    app.process_pending_events();

    let session = app.session_mgr.current();
    eprintln!("[diag] main stats = {:?}", session.agent.session_tool_stats);
    eprintln!(
        "[diag] sub  stats = {:?}",
        session
            .subagent_sessions
            .get("child-1")
            .map(|s| &s.agent.session_tool_stats)
    );

    assert_eq!(
        session.agent.session_tool_stats.get("Grep"),
        Some(&1),
        "主视图应只统计主 Agent 自己的工具"
    );
    assert_eq!(
        session.agent.session_tool_stats.get("Read"),
        None,
        "子 Agent 的工具不得污染主视图统计"
    );
    assert_eq!(
        session
            .subagent_sessions
            .get("child-1")
            .and_then(|s| s.agent.session_tool_stats.get("Read")),
        Some(&3),
        "子 Agent 视口应统计自己的工具"
    );
}

/// 诊断：进入子 Agent 视口后，状态栏第二行必须渲染「子 Agent 自己的」工具统计。
#[tokio::test]
async fn test_subagent_viewport_status_bar_shows_own_stats() {
    use crate::app::AgentEvent;
    use crate::ui::main_ui;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });

    // 主 Agent 跑很多 Grep
    for i in 0..5 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("m{i}"),
            name: "Grep".into(),
            display: "Grep".into(),
            args: "x".into(),
            input: serde_json::json!({}),
            source_agent_id: None,
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("m{i}"),
            name: "Grep".into(),
            output: "ok".into(),
            is_error: false,
            source_agent_id: None,
        });
    }
    // 子 Agent 跑 2 次 Read
    for i in 0..2 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("c{i}"),
            name: "Read".into(),
            display: "Read".into(),
            args: "y".into(),
            input: serde_json::json!({}),
            source_agent_id: Some("child-1".into()),
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("c{i}"),
            name: "Read".into(),
            output: "ok".into(),
            is_error: false,
            source_agent_id: Some("child-1".into()),
        });
    }
    app.process_pending_events();

    app.switch_subagent_viewport(Some("child-1".into()));
    app.flush_rebuild();
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    eprintln!("[diag] sub viewport snapshot:\n{snap}");

    assert!(
        snap.contains("Read ×2"),
        "子 Agent 视口应显示自己的 Read ×2，实际:\n{snap}"
    );
    assert!(
        !snap.contains("Grep ×5"),
        "子 Agent 视口不应显示主 Agent 的 Grep ×5，实际:\n{snap}"
    );
}

/// 端到端：走真实 `session/update` 通道（带 `_peri.sourceAgentId`），
/// 后台子 Agent 的工具必须落进子会话，而不是主 Agent。
#[tokio::test]
async fn test_session_update_with_peri_source_routes_to_subagent() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });

    let tool_call = |id: &str| {
        serde_json::json!({
            "sessionId": "s1",
            "update": {"sessionUpdate": "tool_call", "toolCallId": id, "title": "Read", "rawInput": {}},
            "_peri": {"sourceAgentId": "child-1"},
        })
    };
    let tool_done = |id: &str| {
        serde_json::json!({
            "sessionId": "s1",
            "update": {"sessionUpdate": "tool_call_update", "toolCallId": id, "title": "Read", "rawOutput": "ok", "status": "completed"},
            "_peri": {"sourceAgentId": "child-1"},
        })
    };

    for i in 0..2 {
        app.handle_acp_notification(AcpNotification::SessionUpdate {
            session_id: "s1".into(),
            params: tool_call(&format!("c{i}")),
        });
        app.handle_acp_notification(AcpNotification::SessionUpdate {
            session_id: "s1".into(),
            params: tool_done(&format!("c{i}")),
        });
    }

    let session = app.session_mgr.current();
    eprintln!(
        "[diag] main={:?} sub={:?}",
        session.agent.session_tool_stats,
        session
            .subagent_sessions
            .get("child-1")
            .map(|s| &s.agent.session_tool_stats)
    );
    assert_eq!(
        session
            .subagent_sessions
            .get("child-1")
            .and_then(|s| s.agent.session_tool_stats.get("Read")),
        Some(&2),
        "带 _peri.sourceAgentId 的工具必须计入子会话"
    );
    assert!(
        session.agent.session_tool_stats.is_empty(),
        "带来源标识的事件不得污染主 Agent 统计"
    );
}

/// 回归：子 Agent 视口第二行必须显示子 Agent **正在运行**的工具。
///
/// 症状：Enter 进入子 Agent 后状态栏第二行为空（只有主视图能看到 `◐ Bash : …`）。
/// 根因：`BackgroundAgentSession::handle_event` 只写 active_tool，不登记
/// `running_tools` / `recent_tools`，而第二行读的正是 `visible_recent_tools()`。
#[tokio::test]
async fn test_subagent_viewport_shows_running_tool() {
    use crate::app::AgentEvent;
    use crate::ui::main_ui;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });

    // 子 Agent 起了一个仍在运行的工具（没有 ToolEnd）
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "c-sleep".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "sleep 60 && query-weather".into(),
        input: serde_json::json!({"command": "sleep 60 && query-weather"}),
        source_agent_id: Some("child-1".into()),
    });
    app.process_pending_events();

    app.switch_subagent_viewport(Some("child-1".into()));
    app.flush_rebuild();
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    eprintln!("[diag] running-tool snapshot:\n{snap}");

    assert!(
        snap.contains('◐') && snap.contains("query-weather"),
        "子 Agent 视口应显示其正在运行的工具，实际:\n{snap}"
    );
    assert_eq!(
        app.session_mgr
            .current()
            .viewport_agent()
            .running_tools
            .len(),
        1,
        "子会话必须登记 running_tools"
    );
}

/// 回归 #485：子 Agent 正常完成后，其专属 spinner 必须结算耗时，
/// 使切入子视口时底部渲染出 `✻ <verb> for Xs · done HH:MM` 完成总结行。
///
/// 此前 `peri/subagent_lifecycle` 的 "stopped" 分支漏调 `set_mode(Idle)`，
/// 子 spinner 的 `last_summary_elapsed_ms` 恒为 0，渲染层退化为不画任何行。
#[tokio::test]
async fn test_subagent_viewport_shows_completion_summary_after_stop() {
    use crate::ui::main_ui;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });

    // 让 spinner 至少经历 1ms，避免结算耗时亚毫秒归零造成断言不稳。
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "stopped", "instanceId": "child-1",
            "result": "LGTM", "status": "completed",
        }),
    });

    let sub_summary = app
        .session_mgr
        .current()
        .subagent_sessions
        .get("child-1")
        .expect("抽屉会话应存在")
        .spinner
        .last_summary_elapsed_ms();
    assert!(
        sub_summary > 0,
        "子 Agent 完成后 spinner 应结算耗时，实际 {sub_summary}ms"
    );

    app.switch_subagent_viewport(Some("child-1".into()));
    app.flush_rebuild();
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    eprintln!("[diag] completion summary snapshot:\n{snap}");

    assert!(
        !app.session_mgr.current().viewport_loading(),
        "子 Agent 已完成，视口不应仍处于加载态"
    );
    assert!(
        snap.contains('✻') && snap.contains(" for "),
        "子 Agent 视口底部应渲染完成总结行（✻ ... for ...），实际:\n{snap}"
    );
}

/// 回归 #485（异常路径对照）：被停止的子 Agent 已结算 spinner，
/// 但应清空总结行，不能显示成功文案。
#[tokio::test]
async fn test_subagent_viewport_stopped_has_no_success_summary() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "started", "instanceId": "child-1",
            "agentType": "general-purpose", "taskPreview": "查天气", "isBackground": true,
        }),
    });
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    app.handle_acp_notification(AcpNotification::Peri {
        session_id: "s1".into(),
        method: "peri/subagent_lifecycle".into(),
        params: serde_json::json!({
            "sessionId": "s1", "event": "stopped", "instanceId": "child-1",
            "result": "cancelled", "status": "stopped",
        }),
    });

    let sub = app
        .session_mgr
        .current()
        .subagent_sessions
        .get("child-1")
        .expect("抽屉会话应存在");
    assert_eq!(
        sub.status,
        crate::app::subagent_session::SubagentRunStatus::Stopped
    );
    assert_eq!(
        sub.spinner.last_summary_elapsed_ms(),
        0,
        "被停止的子 Agent 不应残留成功总结行"
    );
}
