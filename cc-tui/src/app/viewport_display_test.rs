use crate::app::App;
use crate::ui::message_view::MessageViewModel;
use crate::ui::render_thread::RenderEvent;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

fn make_started(app: &mut App, id: &str) {
    app.handle_subagent_lifecycle(&json!({
        "event": "started", "instanceId": id, "agentType": "general-purpose",
        "taskPreview": id, "isBackground": true, "model": "child-model",
    }));
}

fn make_display_flags(app: &App) -> (bool, bool, bool) {
    let ui = &app.session_mgr.current().ui;
    (ui.detail_mode, ui.diff_visible, ui.show_tool_messages)
}

fn make_detail_key(app: &mut App) {
    crate::event::keyboard::handle_key_event(
        app,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
    )
    .expect("详细模式快捷键应正常处理");
}

#[tokio::test]
async fn test_viewport_display_child_does_not_inherit_parent_detail_mode() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    if !app.session_mgr.current().ui.detail_mode {
        make_detail_key(&mut app);
    }
    app.switch_subagent_viewport(Some("child".into()));
    assert!(
        !app.session_mgr.current().ui.detail_mode,
        "新子视口必须保持默认折叠，不能继承主视口的临时详细模式"
    );
    app.switch_subagent_viewport(None);
    assert!(
        app.session_mgr.current().ui.detail_mode,
        "返回主视口必须恢复主视口的详细模式"
    );
}

#[tokio::test]
async fn test_viewport_display_three_views_remember_independent_flags() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "one");
    make_started(&mut app, "two");
    let main_flags = make_display_flags(&app);
    app.switch_subagent_viewport(Some("one".into()));
    make_detail_key(&mut app);
    app.toggle_diff();
    app.toggle_collapsed_messages();
    let first_flags = make_display_flags(&app);
    app.switch_subagent_viewport(Some("two".into()));
    assert_eq!(
        make_display_flags(&app),
        (false, false, false),
        "新子视口不能继承另一子视口的展开状态"
    );
    app.toggle_diff();
    let second_flags = make_display_flags(&app);
    app.switch_subagent_viewport(None);
    assert_eq!(make_display_flags(&app), main_flags, "主视口状态应独立保存");
    app.switch_subagent_viewport(Some("one".into()));
    assert_eq!(
        make_display_flags(&app),
        first_flags,
        "第一个子视口状态应恢复"
    );
    app.switch_subagent_viewport(Some("two".into()));
    assert_eq!(
        make_display_flags(&app),
        second_flags,
        "第二个子视口状态应恢复"
    );
}

#[tokio::test]
async fn test_viewport_display_full_queue_keeps_latest_child_snapshot() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "one");
    make_started(&mut app, "two");
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::user("main content".into())];
    for id in ["one", "two"] {
        let sub = app
            .session_mgr
            .current_mut()
            .subagent_sessions
            .get_mut(id)
            .expect("子会话已建立");
        sub.refresh_view_if_dirty();
        sub.view_messages = vec![MessageViewModel::user(format!("{id} content"))];
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    sender
        .try_send(RenderEvent::Rebuild(Vec::new()))
        .expect("占满队列");
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.switch_subagent_viewport(Some("one".into()));
    make_detail_key(&mut app);
    app.switch_subagent_viewport(Some("two".into()));
    make_detail_key(&mut app);
    app.toggle_diff();
    assert!(app.session_mgr.current().messages.render_updates_pending());
    receiver.try_recv().expect("移除占位事件");
    app.session_mgr
        .current_mut()
        .messages
        .retry_render_rebuild();
    match receiver.try_recv().expect("最新完整快照必须补发") {
        RenderEvent::WithDisplay { display, event } => {
            assert!(display.detail_mode, "旧快照不能覆盖后续 Ctrl+O");
            assert!(display.diff_visible, "同一快照必须带最新 Diff 状态");
            match event.into_payload() {
                RenderEvent::Rebuild(messages) => {
                    assert_eq!(messages.len(), 1);
                    assert!(
                        format!("{:?}", messages[0]).contains("two content"),
                        "不能把旧视口内容和新视口显示状态拼在一起"
                    );
                }
                _ => panic!("应补发内容快照"),
            }
        }
        _ => panic!("显示状态必须和内容原子入队"),
    }
    assert!(!app.session_mgr.current().messages.render_updates_pending());
    app.switch_subagent_viewport(None);
    assert_eq!(make_display_flags(&app), (false, false, false));
}

#[tokio::test]
async fn test_viewport_display_toggle_preserves_scroll_and_drafts() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(37, false);
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("主草稿");
    make_detail_key(&mut app);
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 37);
    assert!(!app.session_mgr.current().ui.scroll_follow);
    app.switch_subagent_viewport(Some("child".into()));
    app.session_mgr
        .current_mut()
        .ui
        .set_scroll_position(12, false);
    app.session_mgr
        .current_mut()
        .ui
        .textarea
        .insert_str("子草稿");
    make_detail_key(&mut app);
    app.switch_subagent_viewport(None);
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 37);
    assert!(!app.session_mgr.current().ui.scroll_follow);
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["主草稿"]);
    app.switch_subagent_viewport(Some("child".into()));
    assert_eq!(app.session_mgr.current().ui.scroll_offset, 12);
    assert!(!app.session_mgr.current().ui.scroll_follow);
    assert_eq!(app.session_mgr.current().ui.textarea.lines(), &["子草稿"]);
}

#[tokio::test]
async fn test_viewport_display_readonly_child_detail_does_not_change_parent() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    app.switch_subagent_viewport(Some("child".into()));
    app.handle_subagent_lifecycle(&json!({
        "event": "stopped", "instanceId": "child", "result": "done",
        "isError": false, "elapsedMs": 1000,
    }));
    make_detail_key(&mut app);
    assert!(
        app.session_mgr.current().ui.detail_mode,
        "只读子视口仍应允许展开"
    );
    crate::event::keyboard::handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
    )
    .expect("退出只读子视口");
    assert!(app.session_mgr.current().focused_instance_id.is_none());
    assert!(
        !app.session_mgr.current().ui.detail_mode,
        "展开不能泄漏到主视口"
    );
}

#[tokio::test]
async fn test_viewport_display_approval_enter_restores_child_flags() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    make_detail_key(&mut app);
    app.handle_acp_request_permission(
        cc_acp::transport::types::RequestId::Number(1),
        json!({
            "sessionId": "main", "_peri": {"sourceAgentId": "child"},
            "toolCall": {"toolCallId": "approval", "title": "Bash", "rawInput": {"command": "echo hello"}},
            "options": [{"optionId": "allow_once", "name": "Allow", "kind": "allow_once"}],
        }),
    );
    crate::event::keyboard::handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    )
    .expect("审批提示 Enter 应切换到子视口");
    assert_eq!(
        app.session_mgr.current().focused_instance_id.as_deref(),
        Some("child")
    );
    assert!(
        !app.session_mgr.current().ui.detail_mode,
        "审批跳转也必须恢复子显示状态"
    );
    app.switch_subagent_viewport(None);
    assert!(
        app.session_mgr.current().ui.detail_mode,
        "主视口详细模式应保留"
    );
}

#[tokio::test]
async fn test_viewport_display_global_modes_retry_before_latest_child_snapshot() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    sender
        .try_send(RenderEvent::Rebuild(Vec::new()))
        .expect("占满队列");
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.session_mgr
        .current()
        .send_render_event(RenderEvent::ToggleHookRows(false))
        .ok();
    app.session_mgr
        .current()
        .send_render_event(RenderEvent::SetDetailStrategy("virtual".into()))
        .ok();
    app.switch_subagent_viewport(Some("child".into()));
    make_detail_key(&mut app);
    app.render_rebuild();
    receiver.try_recv().expect("移除占位事件");
    app.session_mgr
        .current_mut()
        .messages
        .retry_render_rebuild();
    assert!(matches!(
        receiver.try_recv().expect("补发 Hook 模式").into_payload(),
        RenderEvent::ToggleHookRows(false)
    ));
    app.session_mgr
        .current_mut()
        .messages
        .retry_render_rebuild();
    assert!(
        matches!(receiver.try_recv().expect("补发渲染策略").into_payload(), RenderEvent::SetDetailStrategy(strategy) if strategy == "virtual")
    );
    app.session_mgr
        .current_mut()
        .messages
        .retry_render_rebuild();
    match receiver.try_recv().expect("模式后补发最新子快照") {
        RenderEvent::WithDisplay { display, event } => {
            assert!(
                display.detail_mode,
                "全局模式重试不能恢复子视口的旧详细模式"
            );
            assert!(matches!(event.into_payload(), RenderEvent::Rebuild(_)));
        }
        _ => panic!("最后的快照必须保留当前子视口显示状态"),
    }
    assert!(!app.session_mgr.current().messages.render_updates_pending());
}

#[tokio::test]
async fn test_viewport_display_theme_refresh_keeps_child_content_and_flags() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    make_started(&mut app, "child");
    app.handle_session_update_peri(&json!({
        "_peri": {"sourceAgentId": "child"},
        "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "child content marker"}},
    }));
    app.switch_subagent_viewport(Some("child".into()));
    make_detail_key(&mut app);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    app.session_mgr.current_mut().messages.render_tx = sender;
    app.apply_theme(crate::ui::theme::current_name());
    match receiver.try_recv().expect("主题刷新应提交当前视口") {
        RenderEvent::WithDisplay { display, event } => {
            assert!(display.detail_mode);
            match event.into_payload() {
                RenderEvent::Rebuild(messages) => {
                    assert!(
                        format!("{messages:?}").contains("child content marker"),
                        "不能把主内容配上子显示状态发布"
                    );
                }
                _ => panic!("应重建当前视口"),
            }
        }
        _ => panic!("主题刷新必须原子保留显示状态与内容"),
    }
}
