use super::*;
use crate::app::{AgentEvent, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;

fn make_started(app: &mut App, id: &str, preview: &str) {
    app.handle_subagent_lifecycle(&json!({
        "event": "started", "instanceId": id, "agentType": "general-purpose",
        "taskPreview": preview, "isBackground": true, "model": "child-model", "timeoutSecs": 3600,
    }));
}

fn make_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    crate::event::keyboard::handle_key_event(app, KeyEvent::new(code, modifiers))
        .expect("按键处理成功");
}

fn make_text(messages: &[MessageViewModel]) -> String {
    format!("{messages:?}")
}

#[tokio::test]
async fn test_subagent_interleaved_reasoning_tools_and_tokens_are_isolated() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    make_started(&mut app, "two", "随州天气");
    for (id, text, tokens) in [("one", "武汉独立思考", 11), ("two", "随州独立思考", 22)]
    {
        app.handle_session_update_peri(&json!({
            "_peri": {"sourceAgentId": id},
            "update": {"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": text}},
        }));
        app.handle_session_update_peri(&json!({
            "_peri": {"sourceAgentId": id},
            "update": {"sessionUpdate": "tool_call", "toolCallId": "shared-id", "title": "Bash", "rawInput": {"command": text}},
        }));
        app.handle_session_update_peri(&json!({
            "_peri": {"sourceAgentId": id},
            "update": {"sessionUpdate": "usage_update", "_meta": {"inputTokens": tokens, "outputTokens": 3}},
        }));
    }
    app.handle_session_update_peri(&json!({"update": {"sessionUpdate": "agent_message_chunk", "content": {"type":"text", "text":"主线程仍可输出"}}}));
    app.handle_session_update_peri(&json!({
        "_peri": {"sourceAgentId": "one"}, "update": {"sessionUpdate": "tool_call_update", "toolCallId": "shared-id", "title": "Bash", "status": "completed", "rawOutput": "武汉结果"},
    }));
    app.request_rebuild();
    // 隐藏子视口按需物化；这里显式观察两条独立时间线以验证隔离。
    for sub in app.session_mgr.current_mut().subagent_sessions.values_mut() {
        sub.refresh_view_if_dirty();
    }
    let session = app.session_mgr.current();
    let one = &session.subagent_sessions["one"];
    let two = &session.subagent_sessions["two"];
    assert!(make_text(&one.view_messages).contains("武汉结果"));
    assert!(!make_text(&one.view_messages).contains("随州独立思考"));
    assert!(make_text(&two.view_messages).contains("随州独立思考"));
    assert!(!make_text(&session.messages.view_messages).contains("独立思考"));
    assert!(!make_text(&session.messages.view_messages).contains("武汉结果"));
    assert!(make_text(&session.messages.view_messages).contains("主线程仍可输出"));
    assert_eq!(one.tokens_used(), 14);
    assert_eq!(two.tokens_used(), 25);
    assert_eq!(one.agent.session_tool_stats.get("Bash"), Some(&1));
    assert!(two.agent.session_tool_stats.is_empty());
    assert!(
        session.agent.session_tool_stats.is_empty(),
        "子工具不能累加主任务统计"
    );
    assert_eq!(session.agent.session_token_tracker.total_input_tokens, 0);
    assert_eq!(
        two.running_tool_times.len(),
        1,
        "相同工具 ID 不能更新另一任务"
    );
}

/// 抽屉导航态打字必须交还输入框（对齐 CC `PromptInput.tsx` 的 type-to-exit footer）。
///
/// 症状：在子视口内按 `↓` 进抽屉后想直接说话，字符被静默吞掉，只能再按一次
/// `Enter` 落回输入框。这里锁定：可打印字符退出抽屉并写进输入框，而 `x` 仍留在
/// 抽屉里执行「停止该 Agent」。
#[tokio::test]
async fn test_drawer_typing_hands_focus_back_to_input() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    make_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().focused_instance_id.as_deref(),
        Some("one")
    );
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().ui.bg_bar_cursor,
        Some(1),
        "子视口内按 ↓ 应进入抽屉且不切换视口"
    );

    make_key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().ui.bg_bar_cursor,
        None,
        "打字应退出抽屉"
    );
    assert_eq!(
        app.session_mgr.current().focused_instance_id.as_deref(),
        Some("one"),
        "退出抽屉不得切换视口"
    );
    assert_eq!(
        app.session_mgr.current().ui.textarea.lines()[0],
        "h",
        "字符应落进输入框"
    );

    // `x` 是抽屉自己的「停止该 Agent」，不得退化成输入
    app.session_mgr.current_mut().ui.textarea = crate::app::build_textarea(false);
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.session_mgr.current().ui.bg_bar_cursor, Some(1));
    make_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().ui.bg_bar_cursor,
        Some(1),
        "'x' 不得退出抽屉"
    );
    assert!(app.session_mgr.current().ui.textarea.lines()[0].is_empty());
    assert!(
        app.session_mgr.current().subagent_sessions["one"].stopped_by_user,
        "'x' 应停止游标所在的子 Agent"
    );
}

#[tokio::test]
async fn test_subagent_viewport_keyboard_cursor_and_read_only_completion() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    make_started(&mut app, "two", "随州天气");
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.session_mgr.current().ui.bg_bar_cursor, Some(1));
    assert!(
        app.session_mgr.current().focused_instance_id.is_none(),
        "移动游标不得切换视口"
    );
    make_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().focused_instance_id.as_deref(),
        Some("one")
    );
    app.handle_subagent_event(
        "one",
        AgentEvent::AssistantChunk {
            chunk: "独立回复".into(),
            source_agent_id: None,
        },
    );
    app.poll_agent();
    assert!(make_text(app.session_mgr.current().current_view_messages()).contains("独立回复"));
    app.handle_subagent_lifecycle(&json!({"event":"stopped", "instanceId":"one", "result":"独立回复", "isError":false, "elapsedMs":32000}));
    make_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(app.session_mgr.current().ui.textarea.lines()[0].is_empty());
    handle
        .terminal
        .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
        .expect("渲染成功");
    assert!(handle.contains("Task ended (read-only)"));
    assert!(handle.contains("child-model"));
    make_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.session_mgr.current().focused_instance_id.is_none());
    assert!(!make_text(app.session_mgr.current().current_view_messages()).contains("独立回复"));
    // 结束时不得把正在阅读的视口抽走；离开视口这一下才回收它的会话与抽屉行。
    assert!(
        !app.session_mgr
            .current()
            .subagent_sessions
            .contains_key("one"),
        "离开已结束视口应回收其会话句柄"
    );
    assert!(
        app.session_mgr
            .current()
            .background_agents
            .iter()
            .all(|agent| agent.instance_id != "one"),
        "离开已结束视口应回收其抽屉行"
    );
    assert_eq!(
        app.session_mgr.current().subagent_sessions["two"].status,
        SubagentRunStatus::Running
    );
}

#[tokio::test]
async fn test_subagent_drawer_overflow_reaches_every_task_and_caps_height() {
    let (mut app, mut handle) = App::new_headless(80, 24).await;
    for index in 0..8 {
        make_started(&mut app, &format!("id-{index}"), &format!("任务{index}"));
    }
    // 抽屉高度含顶部 1 行空行（与状态栏分隔）：封顶 5 行内容 → 6；矮终端折叠为 1 行内容 → 2
    assert_eq!(crate::ui::main_ui::bg_agent_bar::bg_bar_height(&app, 24), 6);
    assert_eq!(crate::ui::main_ui::bg_agent_bar::bg_bar_height(&app, 17), 2);
    make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    for _ in 0..8 {
        make_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.session_mgr.current().ui.bg_bar_cursor, Some(8));
    handle
        .terminal
        .draw(|frame| crate::ui::main_ui::render(frame, &mut app))
        .expect("渲染成功");
    assert!(handle.contains("more"));
    make_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(
        app.session_mgr.current().focused_instance_id.as_deref(),
        Some("id-7")
    );
}

#[tokio::test]
async fn test_subagent_lifecycle_started_is_idempotent() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    make_started(&mut app, "one", "任务");
    app.handle_subagent_event(
        "one",
        AgentEvent::AssistantChunk {
            chunk: "内容保留".into(),
            source_agent_id: None,
        },
    );
    make_started(&mut app, "one", "任务");
    assert_eq!(app.session_mgr.current().background_agents.len(), 1);
    assert_eq!(app.session_mgr.current().subagent_sessions.len(), 1);
    app.switch_subagent_viewport(Some("one".into()));
    assert!(
        make_text(&app.session_mgr.current().subagent_sessions["one"].view_messages)
            .contains("内容保留")
    );
}

#[tokio::test]
async fn test_subagent_pending_approvals_survive_parent_idle_and_queue() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    make_started(&mut app, "two", "随州天气");
    for (index, id) in ["one", "two"].into_iter().enumerate() {
        app.handle_acp_request_permission(
            cc_acp::transport::types::RequestId::Number(index as i64),
            json!({
                "sessionId": "main", "_peri": {"sourceAgentId":id},
                "toolCall":{"toolCallId":id, "title":"Bash", "rawInput":{"command":"echo hello"}},
                "options":[{"optionId":"allow_once", "name":"Allow", "kind":"allow_once"}],
            }),
        );
    }
    assert_eq!(
        app.session_mgr
            .current()
            .approval_source_agent_id
            .as_deref(),
        Some("one")
    );
    assert_eq!(app.session_mgr.current().pending_thread_approvals.len(), 1);
    assert_eq!(
        app.session_mgr.current().subagent_sessions["two"].status,
        SubagentRunStatus::AwaitingApproval
    );
    make_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
    app.poll_agent();
    assert_eq!(
        app.session_mgr
            .current()
            .approval_source_agent_id
            .as_deref(),
        Some("two")
    );
    make_key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(app.session_mgr.current().agent.interaction_prompt.is_none());
}

#[tokio::test]
async fn test_subagent_inherits_language_and_syncs_on_change() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    let _ = app.services.lc.switch("en");
    make_started(&mut app, "sub-en", "check weather");
    let en_sub = &app.session_mgr.current().subagent_sessions["sub-en"];
    assert_eq!(
        en_sub.spinner.verb_list(),
        cc_widgets::spinner::verb::EN_VERBS
    );
    let _ = app.services.lc.switch("zh-CN");
    app.session_mgr.current_mut().sync_language("zh-CN");
    let zh_sub = &app.session_mgr.current().subagent_sessions["sub-en"];
    assert_eq!(
        zh_sub.spinner.verb_list(),
        cc_widgets::spinner::verb::EN_VERBS
    );
    make_started(&mut app, "sub-zh", "检查天气");
    let zh_new_sub = &app.session_mgr.current().subagent_sessions["sub-zh"];
    assert_eq!(
        zh_new_sub.spinner.verb_list(),
        cc_widgets::spinner::verb::EN_VERBS
    );
}

#[tokio::test]
async fn test_drawer_activity_is_throttled_within_window_and_synced_on_finish() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    let sub = app
        .session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("one")
        .expect("子会话存在");

    // 首次填充立即可见（不被节流窗口挡住）
    sub.current_activity = "句子一".into();
    sub.sync_activity_display();
    assert_eq!(sub.activity_shown, "句子一");

    // 节流窗口内的高频更新被丢弃——丢的是渲染帧，不是状态
    sub.current_activity = "句子二".into();
    sub.sync_activity_display();
    assert_eq!(sub.activity_shown, "句子一");

    // 子 Agent 结束后（不再有新值）立即同步，最后一句不被吞掉
    sub.status = SubagentRunStatus::Completed;
    sub.sync_activity_display();
    assert_eq!(sub.activity_shown, "句子二");
}

#[tokio::test]
async fn test_drawer_activity_refreshes_after_throttle_window() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    make_started(&mut app, "one", "武汉天气");
    let sub = app
        .session_mgr
        .current_mut()
        .subagent_sessions
        .get_mut("one")
        .expect("子会话存在");

    sub.current_activity = "句子一".into();
    sub.sync_activity_display();
    assert_eq!(sub.activity_shown, "句子一");

    // 越过节流窗口后，新值照常刷新
    tokio::time::sleep(DRAWER_ACTIVITY_THROTTLE + std::time::Duration::from_millis(20)).await;
    sub.current_activity = "句子二".into();
    sub.sync_activity_display();
    assert_eq!(sub.activity_shown, "句子二");
}
