use super::*;
use crate::ui::message_view::ToolPhase;
use serde_json::json;

fn make_pending(id: &str, source: Option<&str>) -> AgentEvent {
    AgentEvent::ToolPending {
        tool_call_id: id.into(),
        name: "Bash".into(),
        input: json!({"command":"echo same"}),
        source_agent_id: source.map(str::to_owned),
    }
}

fn make_permission(id: &str, source: Option<&str>) -> serde_json::Value {
    json!({"sessionId":"main", "_peri":{"sourceAgentId":source}, "toolCall":{"toolCallId":id,"title":"Bash","rawInput":{"command":"echo same"}}, "options":[{"optionId":"allow_once","name":"Allow","kind":"allow_once"}]})
}

fn make_phase(messages: &[MessageViewModel], id: &str) -> Option<ToolPhase> {
    messages.iter().find_map(|vm| match vm {
        MessageViewModel::ToolBlock {
            tool_call_id,
            phase,
            ..
        } if tool_call_id == id => Some(*phase),
        _ => None,
    })
}

#[tokio::test]
async fn test_approval_targets_call_id_with_identical_commands() {
    let (mut app, _) = App::new_headless(100, 30).await;
    app.session_mgr.current_mut().ui.loading = true;
    for id in ["first", "second", "third"] {
        app.handle_agent_event(make_pending(id, None));
    }
    app.handle_acp_request_permission(
        cc_acp::transport::types::RequestId::Number(1),
        make_permission("second", None),
    );
    let messages = &app.session_mgr.current().messages.view_messages;
    assert_eq!(make_phase(messages, "first"), Some(ToolPhase::Queued));
    assert_eq!(
        make_phase(messages, "second"),
        Some(ToolPhase::AwaitingApproval)
    );
    assert_eq!(make_phase(messages, "third"), Some(ToolPhase::Queued));
    let Some(InteractionPrompt::Approval(prompt)) =
        &app.session_mgr.current().agent.interaction_prompt
    else {
        panic!("应创建审批弹窗")
    };
    assert_eq!(
        prompt.items[0].tool_call_id, "second",
        "审批项必须透传调用身份"
    );
}

#[tokio::test]
async fn test_subagent_approval_does_not_change_parent_call_with_same_id() {
    let (mut app, _) = App::new_headless(100, 30).await;
    app.session_mgr.current_mut().ui.loading = true;
    app.handle_agent_event(make_pending("same", None));
    app.handle_agent_event(make_pending("same", Some("child")));
    app.handle_acp_request_permission(
        cc_acp::transport::types::RequestId::Number(1),
        make_permission("same", Some("child")),
    );
    assert_eq!(
        make_phase(&app.session_mgr.current().messages.view_messages, "same"),
        Some(ToolPhase::Queued)
    );
    let child = &app.session_mgr.current().subagent_sessions["child"];
    assert_eq!(
        make_phase(&child.view_messages, "same"),
        Some(ToolPhase::AwaitingApproval)
    );
    assert!(
        matches!(&child.view_messages[0], MessageViewModel::ToolBlock { source_agent_id:Some(source), .. } if source == "child")
    );
}

#[tokio::test]
async fn test_pending_then_start_same_batch_publishes_running_only() {
    let (mut app, _) = App::new_headless(100, 30).await;
    for id in ["one", "two"] {
        app.handle_agent_event(make_pending(id, None));
    }
    assert!(
        app.session_mgr.current().messages.view_messages.is_empty(),
        "Pending应留到批次末统一发布"
    );
    assert!(
        app.session_mgr.current().agent.interaction_prompt.is_none(),
        "自动判定期间不应创建审批弹窗"
    );
    for id in ["one", "two"] {
        app.handle_agent_event(AgentEvent::ToolStart {
            tool_call_id: id.into(),
            name: "Bash".into(),
            display: "Bash".into(),
            args: "echo same".into(),
            input: json!({"command":"echo same"}),
            source_agent_id: None,
        });
        assert!(
            app.session_mgr.current().messages.view_messages.is_empty(),
            "同批第一条Start不能先发布其余工具的Waiting帧"
        );
    }
    assert!(app.poll_agent(), "批次末应统一刷新阶段");
    for id in ["one", "two"] {
        assert_eq!(
            make_phase(&app.session_mgr.current().messages.view_messages, id),
            Some(ToolPhase::Running)
        );
    }
    assert_eq!(
        app.session_mgr.current().messages.view_messages.len(),
        2,
        "同批Pending/Start只生成一个工具块"
    );
}

#[tokio::test]
async fn test_pending_across_batches_remains_naturally_visible() {
    let (mut app, _) = App::new_headless(100, 30).await;
    app.handle_agent_event(make_pending("one", None));
    assert!(app.poll_agent());
    assert_eq!(
        make_phase(&app.session_mgr.current().messages.view_messages, "one"),
        Some(ToolPhase::Queued),
        "跨批Pending应自然显示Waiting"
    );
}
