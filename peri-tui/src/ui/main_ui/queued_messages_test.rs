use super::*;
use crate::app::QueuedMessage;
use uuid::Uuid;

fn make_message(text: &str) -> QueuedMessage {
    QueuedMessage {
        id: Uuid::new_v4(),
        text: text.into(),
        attachments: Vec::new(),
        sending: false,
    }
}

#[tokio::test]
async fn test_queued_messages_pagination_stays_above_textarea() {
    let (mut app, mut handle) = App::new_headless(80, 24).await;
    app.session_mgr.current_mut().messages.pending_messages = (0..7)
        .map(|index| make_message(&format!("待发送消息{index}")))
        .collect();
    app.session_mgr.current_mut().ui.loading = true;
    assert!(handle
        .terminal
        .draw(|f| super::super::render(f, &mut app))
        .is_ok());
    let ui = &app.session_mgr.current().ui;
    let queue_area = ui.queued_messages_area.expect("应显示队列区域");
    let textarea_area = ui.textarea_area.expect("应显示输入框区域");
    assert_eq!(queue_area.height, 4, "三条消息加分页行必须分配四行");
    assert!(queue_area.bottom() <= textarea_area.y, "队列不能覆盖输入框");
    assert_eq!(
        ui.queued_message_actions.len(),
        7,
        "首屏应有六个消息按钮和下一页按钮"
    );
    assert!(
        ui.queued_message_actions.iter().all(|(rect, _)| {
            rect.x >= queue_area.x
                && rect.y >= queue_area.y
                && rect.right() <= queue_area.right()
                && rect.bottom() <= queue_area.bottom()
        }),
        "命中区域必须位于队列布局内"
    );
    app.session_mgr.current_mut().ui.queued_messages_offset = 6;
    assert!(handle
        .terminal
        .draw(|f| super::super::render(f, &mut app))
        .is_ok());
    let last_id = app.session_mgr.current().messages.pending_messages[6].id;
    assert!(
        app.session_mgr
            .current()
            .ui
            .queued_message_actions
            .iter()
            .any(|(_, action)| {
                matches!(action, QueuedMessageAction::Delete(id) if *id == last_id)
            }),
        "最后一页消息也必须可删除"
    );
    assert!(
        !app.session_mgr
            .current()
            .ui
            .queued_message_actions
            .iter()
            .any(|(_, action)| { matches!(action, QueuedMessageAction::NextPage) }),
        "末页不应生成下一页命中区域"
    );
}

#[tokio::test]
async fn test_queued_messages_sending_and_empty_clear_actions() {
    let (mut app, mut handle) = App::new_headless(80, 24).await;
    let mut message = make_message("已经提交的补充消息");
    message.sending = true;
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .push(message);
    assert!(handle
        .terminal
        .draw(|f| render(f, &mut app, Rect::new(0, 0, 80, 1)))
        .is_ok());
    assert!(
        app.session_mgr
            .current()
            .ui
            .queued_message_actions
            .is_empty(),
        "插入中的消息不能重复插入或删除"
    );
    app.session_mgr.current_mut().messages.pending_messages[0].sending = false;
    assert!(handle
        .terminal
        .draw(|f| render(f, &mut app, Rect::new(0, 0, 80, 1)))
        .is_ok());
    assert_eq!(app.session_mgr.current().ui.queued_message_actions.len(), 2);
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .clear();
    assert!(handle
        .terminal
        .draw(|f| render(f, &mut app, Rect::new(0, 0, 80, 0)))
        .is_ok());
    assert!(
        app.session_mgr
            .current()
            .ui
            .queued_message_actions
            .is_empty(),
        "清空队列后必须清理旧按钮"
    );
    assert!(app.session_mgr.current().ui.queued_messages_area.is_none());
}

#[tokio::test]
async fn test_queued_messages_cjk_multiline_preview_keeps_controls_visible() {
    let (mut app, mut handle) = App::new_headless(40, 24).await;
    app.session_mgr
        .current_mut()
        .messages
        .pending_messages
        .push(make_message(
            "这是一段很长的中文消息\n第二行应合并显示而不能覆盖其他消息行和按钮",
        ));
    assert!(handle
        .terminal
        .draw(|f| render(f, &mut app, Rect::new(2, 2, 36, 1)))
        .is_ok());
    let actions = &app.session_mgr.current().ui.queued_message_actions;
    assert_eq!(actions.len(), 2);
    assert!(actions
        .iter()
        .all(|(rect, _)| rect.y == 2 && rect.right() <= 38));
    assert!(handle.contains("…"), "长中文预览应按显示列宽省略");
    assert!(handle.contains("[×]"), "预览不应覆盖删除按钮");
    assert!(!handle.contains("第二行应"), "多行文本不能溢出到下一行");
}

#[tokio::test]
async fn test_queued_messages_small_area_clamps_hit_regions() {
    let (mut app, mut handle) = App::new_headless(40, 24).await;
    app.session_mgr.current_mut().messages.pending_messages =
        (0..7).map(|_| make_message("补充信息")).collect();
    app.session_mgr.current_mut().ui.queued_messages_offset = usize::MAX;
    let area = Rect::new(3, 4, 20, 2);
    assert!(handle.terminal.draw(|f| render(f, &mut app, area)).is_ok());
    assert_eq!(
        app.session_mgr.current().ui.queued_messages_offset,
        6,
        "两行布局每页只显示一条消息"
    );
    assert!(
        app.session_mgr
            .current()
            .ui
            .queued_message_actions
            .iter()
            .all(|(rect, _)| {
                rect.x >= area.x
                    && rect.y >= area.y
                    && rect.right() <= area.right()
                    && rect.bottom() <= area.bottom()
            }),
        "高度压缩后按钮仍不能越界"
    );
}
