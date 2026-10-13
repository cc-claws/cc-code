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
