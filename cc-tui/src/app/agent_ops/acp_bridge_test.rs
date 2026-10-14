//! 回归：`peri/subagent_lifecycle` 通知必须真正驱动底栏抽屉。
//!
//! 真机症状：状态栏没有 `↓ to view`、底栏无抽屉、消息区无 `(↓ to manage)` 摘要。
//! 根因在 ACP 客户端漏认方法名（已修），此用例守住「通知 → background_agents」
//! 这一段：只要 `AcpNotification::Peri { method: "peri/subagent_lifecycle" }` 到达，
//! 后台子 Agent 就必须登记进抽屉。

use crate::acp_client::AcpNotification;
use crate::app::App;

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
        tool_call_id: "m1".into(), name: "Grep".into(), display: "Grep".into(),
        args: "x".into(), input: serde_json::json!({}), source_agent_id: None,
    });
    app.push_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "m1".into(), name: "Grep".into(), output: "ok".into(),
        is_error: false, source_agent_id: None,
    });

    // 子 Agent 自己的工具调用
    for i in 0..3 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("c{i}"), name: "Read".into(), display: "Read".into(),
            args: "y".into(), input: serde_json::json!({}), source_agent_id: Some("child-1".into()),
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("c{i}"), name: "Read".into(), output: "ok".into(),
            is_error: false, source_agent_id: Some("child-1".into()),
        });
    }
    app.process_pending_events();

    let session = app.session_mgr.current();
    eprintln!("[diag] main stats = {:?}", session.agent.session_tool_stats);
    eprintln!("[diag] sub  stats = {:?}", session.subagent_sessions.get("child-1").map(|s| &s.agent.session_tool_stats));

    assert_eq!(
        session.agent.session_tool_stats.get("Grep"), Some(&1),
        "主视图应只统计主 Agent 自己的工具"
    );
    assert_eq!(
        session.agent.session_tool_stats.get("Read"), None,
        "子 Agent 的工具不得污染主视图统计"
    );
    assert_eq!(
        session.subagent_sessions.get("child-1").and_then(|s| s.agent.session_tool_stats.get("Read")),
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
            tool_call_id: format!("m{i}"), name: "Grep".into(), display: "Grep".into(),
            args: "x".into(), input: serde_json::json!({}), source_agent_id: None,
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("m{i}"), name: "Grep".into(), output: "ok".into(),
            is_error: false, source_agent_id: None,
        });
    }
    // 子 Agent 跑 2 次 Read
    for i in 0..2 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("c{i}"), name: "Read".into(), display: "Read".into(),
            args: "y".into(), input: serde_json::json!({}), source_agent_id: Some("child-1".into()),
        });
        app.push_agent_event(AgentEvent::ToolEnd {
            tool_call_id: format!("c{i}"), name: "Read".into(), output: "ok".into(),
            is_error: false, source_agent_id: Some("child-1".into()),
        });
    }
    app.process_pending_events();

    app.switch_subagent_viewport(Some("child-1".into()));
    app.flush_rebuild();
    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
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
    eprintln!("[diag] main={:?} sub={:?}", session.agent.session_tool_stats,
        session.subagent_sessions.get("child-1").map(|s| &s.agent.session_tool_stats));
    assert_eq!(
        session.subagent_sessions.get("child-1").and_then(|s| s.agent.session_tool_stats.get("Read")),
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
    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
    let snap = handle.snapshot().join("\n");
    eprintln!("[diag] running-tool snapshot:\n{snap}");

    assert!(
        snap.contains('◐') && snap.contains("query-weather"),
        "子 Agent 视口应显示其正在运行的工具，实际:\n{snap}"
    );
    assert_eq!(
        app.session_mgr.current().viewport_agent().running_tools.len(),
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
    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
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
