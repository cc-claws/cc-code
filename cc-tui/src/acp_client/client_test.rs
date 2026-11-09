//! 回归：`peri/*` 自定义通知的路由判定。
//!
//! 背景：服务端 `event_sink` 发送的子 Agent 生命周期方法名是
//! `peri/subagent_lifecycle`（**不带** `notifications/` 前缀）。早期客户端只认
//! `notifications/peri/` 前缀，导致该通知被降级为 `Other` 并被 TUI 丢弃，
//! 底栏抽屉整条链路静默失效（`background_agents` 永远为空）。

use super::{AcpNotification, AcpTuiClient};
use cc_acp::transport::{
    mpsc::mpsc_transport_pair,
    types::{AcpError, IncomingMessage},
    AcpTransport,
};
use serde_json::json;
use std::time::Duration;

#[test]
fn test_peri_notification_routing_for_subagent_lifecycle() {
    // 服务端实际发送的方法名必须被识别
    assert!(AcpTuiClient::is_peri_notification(
        "peri/subagent_lifecycle"
    ));
    // 历史前缀形式保持兼容
    assert!(AcpTuiClient::is_peri_notification(
        "notifications/peri/anything"
    ));
    // 有专用通道的两个方法不走此分支
    assert!(!AcpTuiClient::is_peri_notification("peri/agent_event"));
    assert!(!AcpTuiClient::is_peri_notification("peri/agent_event_done"));
    assert!(!AcpTuiClient::is_peri_notification("session/update"));
}

#[tokio::test]
async fn new_and_load_forward_response_config_options() {
    for load in [false, true] {
        for include_options in [false, true] {
            let (transport, server) = mpsc_transport_pair();
            let (client, mut notifications) = AcpTuiClient::new(transport);
            let session_id = if load {
                "loaded-session"
            } else {
                "new-session"
            };
            let config_options = json!([
                {"id": "model", "currentValue": "test-model"},
                {"id": "agent_kernel", "currentValue": "pi"},
            ]);
            let mut response = if load {
                json!({})
            } else {
                json!({"sessionId": session_id})
            };
            if include_options {
                response["configOptions"] = config_options.clone();
            }
            let response_task = tokio::spawn(async move {
                let Some(IncomingMessage::Request { id, method, params }) = server.recv().await
                else {
                    panic!("expected a session request");
                };
                assert_eq!(method, if load { "session/load" } else { "session/new" });
                if load {
                    assert_eq!(params["sessionId"], session_id);
                }
                server.send_response(id, Ok(response)).await.unwrap();
            });
            let result = tokio::time::timeout(Duration::from_secs(5), async {
                if load {
                    client.load_session(session_id, "/test", None).await
                } else {
                    client.new_session("/test", None).await
                }
            })
            .await
            .unwrap()
            .unwrap();
            response_task.await.unwrap();
            assert_eq!(result, session_id);
            assert_eq!(client.session_id().as_deref(), Some(session_id));
            if include_options {
                let AcpNotification::SessionUpdate {
                    session_id: source_session,
                    params,
                } = notifications.try_recv().unwrap()
                else {
                    panic!("expected a config option update");
                };
                assert_eq!(source_session, session_id);
                assert_eq!(
                    params,
                    json!({
                        "sessionId": session_id,
                        "update": {
                            "sessionUpdate": "config_option_update",
                            "configOptions": config_options,
                        },
                    })
                );
            }
            assert!(matches!(
                notifications.try_recv(),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty)
            ));
        }
    }
}

#[tokio::test]
async fn failed_new_and_load_preserve_session_without_config_updates() {
    for load in [false, true] {
        let (transport, server) = mpsc_transport_pair();
        let (client, mut notifications) = AcpTuiClient::new(transport);
        *client.current_session_id.lock().unwrap() = Some("previous-session".to_string());
        let response_task = tokio::spawn(async move {
            let Some(IncomingMessage::Request { id, .. }) = server.recv().await else {
                panic!("expected a session request");
            };
            server
                .send_response(id, Err(AcpError::new(-32603, "session unavailable")))
                .await
                .unwrap();
        });
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            if load {
                client.load_session("loaded-session", "/test", None).await
            } else {
                client.new_session("/test", None).await
            }
        })
        .await
        .unwrap();
        response_task.await.unwrap();
        assert!(result.unwrap_err().contains("session unavailable"));
        assert_eq!(client.session_id().as_deref(), Some("previous-session"));
        assert!(matches!(
            notifications.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }
}
