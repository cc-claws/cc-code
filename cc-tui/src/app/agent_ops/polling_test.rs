use super::{
    notification_poll_budget_exhausted, AGENT_NOTIFICATION_POLL_BUDGET,
    MAX_AGENT_NOTIFICATIONS_PER_POLL,
};
use crate::{
    acp_client::AcpNotification,
    app::App,
    ui::message_view::{ContentBlockView, MessageViewModel},
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn make_message_chunk(index: usize) -> AcpNotification {
    AcpNotification::SessionUpdate {
        session_id: String::new(),
        params: json!({
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": format!("[{index}]")},
            },
        }),
    }
}

#[test]
fn test_notification_poll_budget_preserves_progress_and_bounds_each_batch() {
    let started_at = Instant::now();
    let deadline = started_at + AGENT_NOTIFICATION_POLL_BUDGET;
    assert!(
        !notification_poll_budget_exhausted(0, started_at, deadline + Duration::from_secs(1)),
        "即使时间预算已满，也要允许每批至少消费一条"
    );
    assert!(!notification_poll_budget_exhausted(
        MAX_AGENT_NOTIFICATIONS_PER_POLL - 1,
        started_at,
        deadline - Duration::from_nanos(1)
    ));
    assert!(
        notification_poll_budget_exhausted(1, started_at, deadline),
        "少量慢通知也必须在时间预算耗尽后让出"
    );
    assert!(
        notification_poll_budget_exhausted(
            MAX_AGENT_NOTIFICATIONS_PER_POLL,
            started_at,
            started_at
        ),
        "大量快通知也不能突破条数上限"
    );
}

#[tokio::test]
async fn test_poll_agent_yields_backlog_and_preserves_fifo_through_done() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, receiver) = mpsc::unbounded_channel();
    let count = MAX_AGENT_NOTIFICATIONS_PER_POLL * 3 + 17;
    for index in 0..count {
        sender
            .send(make_message_chunk(index))
            .expect("测试通知应能入队");
    }
    sender
        .send(AcpNotification::AgentDone {
            session_id: String::new(),
        })
        .expect("完成通知应能入队");
    app.session_mgr.current_mut().agent.acp_notification_rx = Some(receiver);
    app.session_mgr.current_mut().ui.loading = true;
    assert!(app.poll_agent());
    let remaining = app
        .session_mgr
        .current()
        .agent
        .acp_notification_rx
        .as_ref()
        .expect("分批轮询不能关闭通知通道")
        .len();
    assert!(remaining >= count + 1 - MAX_AGENT_NOTIFICATIONS_PER_POLL);
    assert!(remaining <= count, "每批至少应消费一条通知");
    assert!(app.session_mgr.current().ui.loading, "末尾完成事件尚未消费");
    crate::event::keyboard::handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
    )
    .expect("两批通知之间应能处理真实键盘输入");
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["x"]);
    for _ in 0..=count {
        if !app.session_mgr.current().ui.loading {
            break;
        }
        assert!(app.poll_agent(), "积压尚未消费完时应继续安排刷新");
    }
    assert!(
        !app.session_mgr.current().ui.loading,
        "最终必须消费完成事件"
    );
    let text: String = app
        .session_mgr
        .current()
        .messages
        .pipeline
        .build_tail_vms()
        .into_iter()
        .filter_map(|message| match message {
            MessageViewModel::AssistantBubble { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flatten()
        .filter_map(|block| match block {
            ContentBlockView::Text { raw, .. } => Some(raw),
            _ => None,
        })
        .collect();
    let expected: String = (0..count).map(|index| format!("[{index}]")).collect();
    assert_eq!(text, expected, "分批处理不能丢失、重复或重排流式内容");
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["x"]);
    assert!(app
        .session_mgr
        .current()
        .agent
        .acp_notification_rx
        .as_ref()
        .expect("完成后仍应保留既有通道语义")
        .is_empty());
}

#[tokio::test]
async fn test_poll_agent_filtered_backlog_still_requests_next_frame() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    let (sender, receiver) = mpsc::unbounded_channel();
    let count = MAX_AGENT_NOTIFICATIONS_PER_POLL + 1;
    for _ in 0..count {
        sender
            .send(AcpNotification::Peri {
                session_id: String::new(),
                method: "peri/ignored".into(),
                params: json!({}),
            })
            .expect("测试通知应能入队");
    }
    app.session_mgr.current_mut().agent.acp_notification_rx = Some(receiver);
    assert!(
        app.poll_agent(),
        "被过滤通知也会占用预算，剩余积压必须唤醒下一帧"
    );
    let remaining = app
        .session_mgr
        .current()
        .agent
        .acp_notification_rx
        .as_ref()
        .expect("预算耗尽不能关闭通道")
        .len();
    assert!(remaining > 0);
    assert!(remaining < count);
}
