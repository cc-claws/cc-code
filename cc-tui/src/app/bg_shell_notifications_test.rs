use crate::app::{App, PendingBgShellNotification};
use cc_acp::{
    session::steering::{STEERING_NOT_CONSUMED, STEERING_NOT_ENQUEUED},
    transport::{
        mpsc::{mpsc_transport_pair, MpscServerTransport},
        types::{AcpError, IncomingMessage, RequestId},
        AcpTransport,
    },
};
use serde_json::{json, Value};
use std::time::Duration;

const TEST_TIMEOUT: Duration = Duration::from_secs(2);

fn make_submission_ready(app: &mut App) {
    let mut config = crate::config::PeriConfig::default();
    config.config.active_provider_id = "test-provider".into();
    config.config.active_alias = "sonnet".into();
    config.config.providers = vec![crate::config::ProviderConfig {
        id: "test-provider".into(),
        provider_type: "openai".into(),
        api_key: "test-key".into(),
        models: crate::config::ProviderModels {
            sonnet: "mock-model".into(),
            ..Default::default()
        },
        ..Default::default()
    }];
    app.services.peri_config = Some(config);
    // ACP 请求由内存服务端接管；禁用首次标题生成，防止另起真实模型请求。
    app.session_mgr
        .current_mut()
        .metadata
        .title_generation_attempted = true;
}

fn make_notification(owner: Option<&str>, task_id: &str) -> PendingBgShellNotification {
    PendingBgShellNotification {
        owner_session_id: owner.map(ToString::to_string),
        content: format!(
            "<background-task-completed>\n<task-id>{task_id}</task-id>\n<command>echo done</command>\n<status>completed (exit 0)</status>\n<output>/tmp/{task_id}.output</output>\n</background-task-completed>"
        ),
    }
}

async fn make_client(session_id: &str) -> (crate::acp_client::AcpTuiClient, MpscServerTransport) {
    let (transport, server) = mpsc_transport_pair();
    let (client, _notifications) = crate::acp_client::AcpTuiClient::new(transport);
    let handshake = async {
        let (id, method, _) = receive_request(&server).await;
        assert_eq!(method, "session/new", "应先创建目标会话");
        assert!(server
            .send_response(id, Ok(json!({ "sessionId": session_id })))
            .await
            .is_ok());
    };
    let (created, ()) = tokio::join!(client.new_session(".", None), handshake);
    assert_eq!(created.expect("创建测试会话"), session_id);
    (client, server)
}

async fn receive_request(server: &MpscServerTransport) -> (RequestId, String, Value) {
    match tokio::time::timeout(TEST_TIMEOUT, server.recv())
        .await
        .expect("应及时收到 ACP 请求")
    {
        Some(IncomingMessage::Request { id, method, params }) => (id, method, params),
        message => panic!("应收到 ACP 请求，实际为 {message:?}"),
    }
}

async fn wait_for_receipt(app: &mut App) {
    tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            if app.poll_bg_shell_notifications() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("应及时回收后台通知回执");
}

async fn assert_no_request(server: &MpscServerTransport) {
    match tokio::time::timeout(Duration::from_millis(30), server.recv()).await {
        // 传输已关闭（会话切换后旧 client 被 drop）时不可能再送达任何请求，与超时同义。
        // 判据必须落在「有没有真收到消息」上，不能只看 timeout 是否 Err。
        Ok(None) | Err(_) => {}
        Ok(Some(message)) => {
            panic!("不能额外发起推理轮次或重复投递通知，实际收到 {message:?}")
        }
    }
}

#[tokio::test]
async fn test_background_notification_steers_while_loading() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "running-task")]);
    let (id, method, params) = receive_request(&server).await;
    assert_eq!(method, "peri/session/steer", "运行中必须插入当前轮次");
    assert_eq!(params["sessionId"], "owner-a");
    assert!(params["message"]["content"]
        .to_string()
        .contains("running-task"));
    assert!(app
        .session_mgr
        .current()
        .pending_bg_shell_notifications
        .is_empty());
    assert!(
        app.session_mgr.current().ui.loading,
        "等待消费回执不能结束当前任务"
    );
    assert!(server
        .send_response(id, Ok(json!({ "consumed": true })))
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    assert!(app
        .session_mgr
        .current()
        .pending_bg_shell_notifications
        .is_empty());
    assert_no_request(&server).await;
}

#[tokio::test]
async fn test_background_notifications_share_current_turn_without_extra_prompts() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(
        ["first-task", "second-task", "third-task"]
            .into_iter()
            .map(|task_id| make_notification(Some("owner-a"), task_id))
            .collect(),
    );
    let mut requests = Vec::new();
    for _ in 0..3 {
        let (id, method, params) = receive_request(&server).await;
        assert_eq!(method, "peri/session/steer", "三条通知应共享正在执行的轮次");
        assert_eq!(params["sessionId"], "owner-a");
        requests.push((id, params["message"]["content"].to_string()));
    }
    for task_id in ["first-task", "second-task", "third-task"] {
        assert_eq!(
            requests
                .iter()
                .filter(|(_, content)| content.contains(task_id))
                .count(),
            1
        );
    }
    for (id, _) in requests {
        assert!(server
            .send_response(id, Ok(json!({ "consumed": true })))
            .await
            .is_ok());
    }
    wait_for_receipt(&mut app).await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
        app.poll_bg_shell_notifications();
    }
    assert!(app
        .session_mgr
        .current()
        .pending_bg_shell_notifications
        .is_empty());
    assert!(app.session_mgr.current().ui.loading);
    assert_no_request(&server).await;
}

async fn assert_rejected_notification_is_retained(code: i64) {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "rejected-task")]);
    let (id, method, _) = receive_request(&server).await;
    assert_eq!(method, "peri/session/steer");
    assert!(server
        .send_response(id, Err(AcpError::new(code, "本轮未消费")))
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    let pending = &app.session_mgr.current().pending_bg_shell_notifications;
    assert_eq!(pending.len(), 1, "明确未消费的通知必须保留");
    assert!(pending[0].content.contains("rejected-task"));
    assert_eq!(pending[0].owner_session_id.as_deref(), Some("owner-a"));
    assert_no_request(&server).await;
}

#[tokio::test]
async fn test_background_notification_is_retained_when_not_enqueued() {
    assert_rejected_notification_is_retained(STEERING_NOT_ENQUEUED).await;
}

#[tokio::test]
async fn test_background_notification_is_retained_when_not_consumed() {
    assert_rejected_notification_is_retained(STEERING_NOT_CONSUMED).await;
}

#[tokio::test]
async fn test_done_before_rejected_receipt_submits_idle_notification_once() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    make_submission_ready(&mut app);
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "late-task")]);
    let (id, method, _) = receive_request(&server).await;
    assert_eq!(method, "peri/session/steer");
    app.handle_agent_event(crate::app::AgentEvent::Done);
    assert!(!app.session_mgr.current().ui.loading);
    assert!(server
        .send_response(id, Err(AcpError::new(STEERING_NOT_ENQUEUED, "执行已结束")))
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    let (prompt_id, method, params) = receive_request(&server).await;
    assert_eq!(
        method, "session/prompt",
        "Done 后迟到的回执也必须驱动兜底出口"
    );
    assert!(params["message"]["content"]
        .to_string()
        .contains("late-task"));
    assert!(app
        .session_mgr
        .current()
        .pending_bg_shell_notifications
        .is_empty());
    assert!(server.send_response(prompt_id, Ok(json!({}))).await.is_ok());
    for _ in 0..8 {
        tokio::task::yield_now().await;
        app.poll_bg_shell_notifications();
    }
    assert_no_request(&server).await;
}

#[tokio::test]
async fn test_late_rejected_receipt_keeps_original_session_owner() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (first_client, first_server) = make_client("owner-a").await;
    // 保留旧 client 的传输：client 被替换后通道会关闭，`recv` 立即返回 None，
    // 「旧会话不再收到请求」就断言不到了。留一份引用让该断言真正生效。
    let _first_client_keepalive = first_client.clone();
    app.acp_client = Some(first_client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(None, "original-task")]);
    let (id, method, params) = receive_request(&first_server).await;
    assert_eq!(method, "peri/session/steer");
    assert_eq!(params["sessionId"], "owner-a");
    let (second_client, second_server) = make_client("owner-b").await;
    app.acp_client = Some(second_client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-b".into());
    app.set_loading(false);
    assert!(first_server
        .send_response(
            id,
            Err(AcpError::new(STEERING_NOT_ENQUEUED, "原执行已结束"))
        )
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    let pending = &app.session_mgr.current().pending_bg_shell_notifications;
    assert_eq!(pending.len(), 1, "原会话通知不能被新会话消费");
    assert_eq!(pending[0].owner_session_id.as_deref(), Some("owner-a"));
    assert!(pending[0].content.contains("original-task"));
    assert!(!app.session_mgr.current().ui.loading);
    assert_no_request(&second_server).await;
    assert_no_request(&first_server).await;
}

#[tokio::test]
async fn test_unknown_receipt_error_is_not_automatically_retried() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    make_submission_ready(&mut app);
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "uncertain-task")]);
    let (id, method, _) = receive_request(&server).await;
    assert_eq!(method, "peri/session/steer");
    assert!(server
        .send_response(id, Err(AcpError::new(-32603, "连接状态未知")))
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    app.handle_agent_event(crate::app::AgentEvent::Done);
    for _ in 0..8 {
        tokio::task::yield_now().await;
        app.poll_bg_shell_notifications();
    }
    assert!(
        !app.session_mgr.current().ui.loading,
        "无法判定是否消费时不能自动开启新轮"
    );
    assert_no_request(&server).await;
}

#[tokio::test]
async fn test_interrupted_and_error_receipts_do_not_restart_finished_tasks() {
    for interrupted in [true, false] {
        let (mut app, _handle) = App::new_headless(80, 24).await;
        make_submission_ready(&mut app);
        let (client, server) = make_client("owner-a").await;
        app.acp_client = Some(client);
        app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
        app.set_loading(true);
        app.inject_bg_shell_notifications(vec![make_notification(
            Some("owner-a"),
            "deferred-task",
        )]);
        let (id, method, _) = receive_request(&server).await;
        assert_eq!(method, "peri/session/steer");
        if interrupted {
            app.handle_agent_event(crate::app::AgentEvent::Interrupted);
        } else {
            app.handle_agent_event(crate::app::AgentEvent::Error("测试失败".into()));
        }
        app.handle_agent_event(crate::app::AgentEvent::Done);
        assert!(server
            .send_response(
                id,
                Err(AcpError::new(STEERING_NOT_CONSUMED, "结束前未消费"))
            )
            .await
            .is_ok());
        wait_for_receipt(&mut app).await;
        assert!(
            !app.session_mgr.current().ui.loading,
            "异常收尾后的通知不能自动恢复执行"
        );
        let pending = &app.session_mgr.current().pending_bg_shell_notifications;
        assert_eq!(pending.len(), 1, "未消费通知应保留到用户继续任务");
        assert!(pending[0].content.contains("deferred-task"));
        for _ in 0..8 {
            tokio::task::yield_now().await;
            app.poll_bg_shell_notifications();
        }
        assert_no_request(&server).await;
    }
}

#[tokio::test]
async fn test_rejected_notification_before_interruption_and_error_is_not_automatically_submitted() {
    for interrupted in [true, false] {
        let (mut app, _handle) = App::new_headless(80, 24).await;
        make_submission_ready(&mut app);
        let (client, server) = make_client("owner-a").await;
        app.acp_client = Some(client);
        app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
        app.set_loading(true);
        app.inject_bg_shell_notifications(vec![make_notification(
            Some("owner-a"),
            "early-rejected-task",
        )]);
        let (id, method, _) = receive_request(&server).await;
        assert_eq!(method, "peri/session/steer");
        assert!(server
            .send_response(id, Err(AcpError::new(STEERING_NOT_CONSUMED, "本轮未消费")))
            .await
            .is_ok());
        wait_for_receipt(&mut app).await;
        assert_eq!(
            app.session_mgr
                .current()
                .pending_bg_shell_notifications
                .len(),
            1
        );
        if interrupted {
            app.handle_agent_event(crate::app::AgentEvent::Interrupted);
        } else {
            app.handle_agent_event(crate::app::AgentEvent::Error("测试失败".into()));
        }
        app.handle_agent_event(crate::app::AgentEvent::Done);
        for _ in 0..8 {
            tokio::task::yield_now().await;
            app.poll_bg_shell_notifications();
        }
        assert!(
            !app.session_mgr.current().ui.loading,
            "先回队的通知也不能在异常 Done 后恢复执行"
        );
        let pending = &app.session_mgr.current().pending_bg_shell_notifications;
        assert_eq!(pending.len(), 1, "异常收尾不能丢弃已经回队的通知");
        assert!(pending[0].content.contains("early-rejected-task"));
        assert_no_request(&server).await;
    }
}

#[tokio::test]
async fn test_next_user_submission_includes_notification_retained_after_interruption_and_error() {
    for interrupted in [true, false] {
        let (mut app, _handle) = App::new_headless(80, 24).await;
        make_submission_ready(&mut app);
        let (client, server) = make_client("owner-a").await;
        app.acp_client = Some(client);
        app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
        app.set_loading(true);
        app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "resume-task")]);
        let (id, method, _) = receive_request(&server).await;
        assert_eq!(method, "peri/session/steer");
        if interrupted {
            app.handle_agent_event(crate::app::AgentEvent::Interrupted);
        } else {
            app.handle_agent_event(crate::app::AgentEvent::Error("测试失败".into()));
        }
        app.handle_agent_event(crate::app::AgentEvent::Done);
        assert!(server
            .send_response(
                id,
                Err(AcpError::new(STEERING_NOT_CONSUMED, "结束前未消费"))
            )
            .await
            .is_ok());
        wait_for_receipt(&mut app).await;
        assert!(!app.session_mgr.current().ui.loading);
        assert_eq!(
            app.session_mgr
                .current()
                .pending_bg_shell_notifications
                .len(),
            1
        );
        assert_no_request(&server).await;
        app.submit_message("继续用户原来的任务".into());
        let (prompt_id, method, params) = receive_request(&server).await;
        assert_eq!(method, "session/prompt", "恢复通知应随下一条主动消息发送");
        assert_eq!(params["sessionId"], "owner-a");
        let content = params["message"]["content"].to_string();
        assert!(
            content.contains("继续用户原来的任务"),
            "不能丢失用户主动提交的文本"
        );
        assert_eq!(
            content.matches("<task-id>resume-task</task-id>").count(),
            1,
            "通知应恰好一次包含在同一个 prompt 中"
        );
        assert!(app
            .session_mgr
            .current()
            .pending_bg_shell_notifications
            .is_empty());
        assert!(server.send_response(prompt_id, Ok(json!({}))).await.is_ok());
        for _ in 0..8 {
            tokio::task::yield_now().await;
            app.poll_bg_shell_notifications();
        }
        assert_no_request(&server).await;
    }
}

#[tokio::test]
async fn test_queued_user_message_includes_pending_notification_without_extra_turn() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    make_submission_ready(&mut app);
    let (client, server) = make_client("owner-a").await;
    app.acp_client = Some(client);
    app.session_mgr.current_mut().current_thread_id = Some("owner-a".into());
    app.set_loading(true);
    app.inject_bg_shell_notifications(vec![make_notification(Some("owner-a"), "queued-task")]);
    let (id, method, _) = receive_request(&server).await;
    assert_eq!(method, "peri/session/steer");
    assert!(server
        .send_response(
            id,
            Err(AcpError::new(STEERING_NOT_ENQUEUED, "当前队列已关闭"))
        )
        .await
        .is_ok());
    wait_for_receipt(&mut app).await;
    app.queue_user_message("用户排队的下一项任务".into());
    app.handle_agent_event(crate::app::AgentEvent::Done);
    let (prompt_id, method, params) = receive_request(&server).await;
    assert_eq!(method, "session/prompt", "优先发送用户排队消息");
    let content = params["message"]["content"].to_string();
    assert!(content.contains("用户排队的下一项任务"));
    assert_eq!(
        content.matches("<task-id>queued-task</task-id>").count(),
        1,
        "用户排队优先时后台通知仍应随该轮送达"
    );
    assert!(app
        .session_mgr
        .current()
        .messages
        .pending_messages
        .is_empty());
    assert!(app
        .session_mgr
        .current()
        .pending_bg_shell_notifications
        .is_empty());
    assert!(server.send_response(prompt_id, Ok(json!({}))).await.is_ok());
    for _ in 0..8 {
        tokio::task::yield_now().await;
        app.poll_bg_shell_notifications();
    }
    assert_no_request(&server).await;
}
