use super::*;
use crate::app::PendingAttachment;
use peri_agent::messages::ContentBlock;

fn make_image(id: usize) -> PendingAttachment {
    PendingAttachment {
        label: format!("clipboard_{id}.png"),
        media_type: "image/png".into(),
        base64_data: format!("image-{id}"),
        size_bytes: 7,
        image_id: id,
    }
}

#[tokio::test]
async fn test_queue_user_message_owns_images_and_expands_pasted_text() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().ui.loading = true;
    app.paste_text_into_textarea("补充第一行\n第二行");
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str(" [Image #1]");
    app.session_mgr.current_mut().metadata.pending_attachments = vec![make_image(1), make_image(2)];
    let input = app.session_mgr.current().ui.textarea.lines().join("\n");
    app.queue_user_message(input);
    app.paste_text_into_textarea("新的草稿\n不能混入前一条");
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_image(3));
    let queued = &app.session_mgr.current().messages.pending_messages[0];
    assert_eq!(queued.text, "补充第一行\n第二行 [Image #1]");
    assert_eq!(queued.attachments.len(), 1, "删除占位符的图片不应入队");
    assert_eq!(queued.attachments[0].image_id, 1);
    let content = queued.content();
    assert_eq!(content.content_blocks().len(), 2, "文字和图片一起提交");
    assert!(matches!(
        content.content_blocks()[1],
        ContentBlock::Image { .. }
    ));
    assert_eq!(
        app.session_mgr.current().metadata.pending_attachments[0].image_id,
        3
    );
    assert_eq!(
        app.session_mgr.current().ui.pasted_text_blocks.len(),
        1,
        "草稿映射独立"
    );
}

#[tokio::test]
async fn test_queued_message_delete_only_selected_message_and_images() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let first = QueuedMessage::new("[Image #1]".into(), vec![make_image(1)]);
    let selected = QueuedMessage::new("[Image #2]".into(), vec![make_image(2)]);
    let first_id = first.id;
    let selected_id = selected.id;
    app.session_mgr.current_mut().messages.pending_messages = vec![first, selected];
    app.session_mgr.current_mut().ui.textarea.insert_str("草稿");
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_image(3));
    app.handle_queued_message_action(QueuedMessageAction::Delete(selected_id));
    let session = app.session_mgr.current();
    assert_eq!(session.messages.pending_messages.len(), 1);
    assert_eq!(session.messages.pending_messages[0].id, first_id);
    assert_eq!(
        session.messages.pending_messages[0].attachments[0].image_id,
        1
    );
    assert_eq!(session.metadata.pending_attachments[0].image_id, 3);
    assert_eq!(session.ui.textarea.lines(), ["草稿"]);
}

#[tokio::test]
async fn test_queued_message_sending_cannot_delete_or_auto_resubmit() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let mut pending = QueuedMessage::new("补充".into(), vec![make_image(1)]);
    pending.sending = true;
    let id = pending.id;
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .push(pending);
    app.handle_queued_message_action(QueuedMessageAction::Delete(id));
    app.handle_queued_message_action(QueuedMessageAction::Steer(id));
    app.flush_pending_messages();
    assert_eq!(
        app.session_mgr.current().messages.pending_messages.len(),
        1,
        "等待确认期间不得丢弃或重复发送"
    );
}

#[tokio::test]
async fn test_queued_message_failure_retains_images_and_success_removes_only_selected() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().ui.loading = true;
    let mut pending = QueuedMessage::new("[Image #1]".into(), vec![make_image(1)]);
    pending.sending = true;
    let id = pending.id;
    app.session_mgr.current_mut().messages.pending_messages =
        vec!["保留第一条".to_string().into(), pending];
    let tx = app
        .session_mgr
        .current()
        .messages
        .steering_result_tx
        .clone();
    assert!(tx.send((id, Err("执行刚结束".into()))).is_ok());
    assert!(app.poll_steering_results());
    let pending = &app.session_mgr.current().messages.pending_messages[1];
    assert!(!pending.sending);
    assert_eq!(pending.attachments.len(), 1);
    app.session_mgr.current_mut().messages.pending_messages[1].sending = true;
    assert!(tx.send((id, Ok(()))).is_ok());
    assert!(app.poll_steering_results());
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    assert_eq!(
        app.session_mgr.current().messages.pending_messages[0].text,
        "保留第一条"
    );
}

#[tokio::test]
async fn test_queued_message_flush_preserves_current_draft() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.paste_text_into_textarea("草稿第一行\n第二行");
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str(" [Image #2]");
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_image(2));
    let draft = app.session_mgr.current().ui.textarea.lines().join("\n");
    // 本地 /streaming 命令无需真实模型，验证队列提交与草稿资源隔离。
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .push("/streaming".to_string().into());
    app.flush_pending_messages();
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines().join("\n"),
        draft
    );
    assert_eq!(
        app.session_mgr.current().metadata.pending_attachments[0].image_id,
        2
    );
    assert_eq!(
        app.expand_pasted_text(&draft),
        "草稿第一行\n第二行 [Image #2]"
    );
}

#[tokio::test]
async fn test_queued_message_mouse_delete_routes_to_selected_item() {
    use ratatui::crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let (mut app, mut handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().messages.pending_messages =
        vec!["甲".to_string().into(), "乙".to_string().into()];
    let id = app.session_mgr.current().messages.pending_messages[1].id;
    assert!(handle
        .terminal
        .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
        .is_ok());
    let area = app
        .session_mgr
        .current()
        .ui
        .queued_message_actions
        .iter()
        .find(|(_, action)| *action == QueuedMessageAction::Delete(id))
        .expect("删除按钮存在")
        .0;
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x,
        row: area.y,
        modifiers: KeyModifiers::NONE,
    });
    assert!(crate::event::handle_event(&mut app, event).await.is_ok());
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    assert_eq!(
        app.session_mgr.current().messages.pending_messages[0].text,
        "甲"
    );
}

#[tokio::test]
async fn test_queued_message_steer_sends_selected_images_through_acp() {
    use peri_acp::transport::{mpsc::mpsc_transport_pair, types::IncomingMessage, AcpTransport};
    use serde_json::json;
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (transport, server) = mpsc_transport_pair();
    let (client, _notifications) = crate::acp_client::AcpTuiClient::new(transport);
    let server_handshake = async {
        let Some(IncomingMessage::Request { id, .. }) = server.recv().await else {
            panic!("应收到创建会话请求");
        };
        assert!(server
            .send_response(id, Ok(json!({ "sessionId": "queue-test" })))
            .await
            .is_ok());
    };
    let (session, ()) = tokio::join!(client.new_session(".", None), server_handshake);
    assert!(session.is_ok());
    app.acp_client = Some(client);
    app.session_mgr.current_mut().ui.loading = true;
    let selected = QueuedMessage::new("补充截图 [Image #2]".into(), vec![make_image(2)]);
    let selected_id = selected.id;
    let expected = selected.content();
    app.session_mgr.current_mut().messages.pending_messages =
        vec!["继续排队".to_string().into(), selected];
    app.handle_queued_message_action(QueuedMessageAction::Steer(selected_id));
    assert!(app.session_mgr.current().messages.pending_messages[1].sending);
    assert_eq!(
        app.session_mgr.current().messages.pending_messages.len(),
        2,
        "服务端确认前不能删队列消息"
    );
    let Some(IncomingMessage::Request { id, method, params }) = server.recv().await else {
        panic!("应收到补充请求");
    };
    assert_eq!(method, "peri/session/steer");
    assert_eq!(params["sessionId"], "queue-test");
    assert_eq!(
        params["message"]["content"],
        serde_json::to_value(expected).expect("序列化消息")
    );
    assert!(server
        .send_response(id, Ok(json!({ "consumed": true })))
        .await
        .is_ok());
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !app.poll_steering_results() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_ok()
    );
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    assert_eq!(
        app.session_mgr.current().messages.pending_messages[0].text,
        "继续排队"
    );
}

#[tokio::test]
async fn test_queued_message_interrupt_preserves_queue_and_new_draft() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().ui.loading = true;
    app.session_mgr.current_mut().messages.last_submitted_text = Some("旧问题".into());
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .push(QueuedMessage::new("[Image #1]".into(), vec![make_image(1)]));
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("新的草稿 [Image #2]");
    app.session_mgr
        .current_mut()
        .metadata
        .pending_attachments
        .push(make_image(2));
    app.handle_agent_event(crate::app::AgentEvent::Interrupted);
    assert_eq!(app.session_mgr.current().messages.pending_messages.len(), 1);
    assert_eq!(
        app.session_mgr.current().messages.pending_messages[0].attachments[0].image_id,
        1
    );
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines(),
        ["新的草稿 [Image #2]"]
    );
    assert_eq!(
        app.session_mgr.current().metadata.pending_attachments[0].image_id,
        2
    );
}

#[tokio::test]
async fn test_queued_message_busy_paste_key_is_not_inserted_as_literal() {
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().ui.loading = true;
    // Option+V 与 Alt+V 共用图片粘贴分支；旧 loading 守卫会让它落入普通文字输入。
    let key = KeyEvent::new(KeyCode::Char('√'), KeyModifiers::NONE);
    assert!(crate::event::handle_event(&mut app, Event::Key(key))
        .await
        .is_ok());
    assert!(
        !app.session_mgr
            .current()
            .ui
            .textarea
            .lines()
            .join("")
            .contains('√'),
        "执行中粘贴按键应被粘贴处理器消费，不能作为普通字符输入"
    );
    assert!(app.session_mgr.current().ui.loading);
}
