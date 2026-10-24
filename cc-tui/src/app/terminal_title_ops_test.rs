use super::*;
use std::time::Duration;

fn make_app() -> App {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(App::new())
}

#[test]
fn test_app_current_title_status_idle_by_default() {
    let app = make_app();
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::Idle,
        "新启动且未执行任务的会话状态应为 Idle"
    );
}

#[test]
fn test_app_current_title_status_thinking_when_loading() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = true;
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::Thinking,
        "加载中且无工具执行时应为 Thinking"
    );
}

#[test]
fn test_app_current_title_status_working_when_tool_active() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = true;
    app.session_mgr.current_mut().agent.active_tool = Some(crate::app::ActiveToolInfo {
        tool_call_id: "call_1".to_string(),
        name: "Bash".to_string(),
        display: "Bash".to_string(),
        args_summary: "cargo test".to_string(),
    });
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::Working,
        "加载中且有活跃工具执行时应为 Working"
    );
}

#[test]
fn test_app_current_title_status_action_required_highest_priority() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = true;
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let prompt = crate::app::HitlBatchPrompt::new(vec![], tx);
    app.session_mgr.current_mut().agent.interaction_prompt =
        Some(crate::app::InteractionPrompt::Approval(prompt));
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::ActionRequired,
        "无论是否 loading，交互弹窗激活时必须优先判定为 ActionRequired"
    );
}

#[test]
fn test_app_current_title_status_done_after_task_completed() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = false;
    app.session_mgr.current_mut().agent.last_task_duration = Some(Duration::from_secs(5));
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::Done,
        "任务完成且退出 loading 后状态应为 Done"
    );
}

#[test]
fn test_app_refresh_terminal_title_caches_last_title() {
    let mut app = make_app();
    app.session_mgr.current_mut().metadata.thread_title = Some("unit-test".to_string());
    app.refresh_terminal_title();
    assert!(
        app.global_ui.last_terminal_title.is_some(),
        "刷新终端标题后应记录 last_terminal_title 缓存"
    );
    let cached = app.global_ui.last_terminal_title.clone().unwrap();
    assert!(cached.contains("unit-test"), "缓存的标题应包含会话主题");
}

#[test]
fn test_app_terminal_title_animates_with_rotating_chrysanthemum() {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = true;
    app.session_mgr.current_mut().metadata.thread_title = Some("task".to_string());

    // 初始帧
    app.refresh_terminal_title();
    let frame1 = app.global_ui.last_terminal_title.clone().unwrap();

    // 菊花按单调时钟呼吸（对齐参考实现：2s 往返、端点停留约 205ms），
    // 因此必须真的等过一个字形停留窗口，光推进 tick 计数不再改变字形。
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.session_mgr.current_mut().spinner_state.advance_tick();
    app.refresh_terminal_title();
    let frame2 = app.global_ui.last_terminal_title.clone().unwrap();

    assert_ne!(
        frame1, frame2,
        "运行中推进动画帧后，标题中的菊花动画必须动态旋转变化"
    );
    let project = app.project_name().to_string();
    assert!(
        frame1.contains("task") && frame1.ends_with(&project),
        "标题应包含任务名并以项目名结尾"
    );
    assert!(
        frame2.contains("task") && frame2.ends_with(&project),
        "标题应包含任务名并以项目名结尾"
    );
}

/// 构造「主 agent 已收尾、后台子 agent 仍在跑」的会话状态
fn make_app_waiting_subagents() -> App {
    let mut app = make_app();
    app.session_mgr.current_mut().ui.loading = false;
    app.session_mgr.current_mut().agent.last_task_duration = Some(Duration::from_secs(5));
    app.session_mgr
        .current_mut()
        .background_agents
        .push(crate::app::RunningBgAgent {
            agent_name: "explore".to_string(),
            instance_id: "bg-1".to_string(),
            started_at: std::time::Instant::now(),
            total_steps: 0,
        });
    app
}

#[test]
fn test_app_current_title_status_working_while_waiting_subagents() {
    let app = make_app_waiting_subagents();
    assert_eq!(
        app.current_title_status(),
        TerminalTitleStatusKind::Working,
        "主 agent 已收尾但后台子 agent 仍在跑时，标题应保持工作态而不是 Done"
    );
}

#[test]
fn test_app_terminal_title_animates_while_waiting_subagents() {
    let mut app = make_app_waiting_subagents();
    app.session_mgr.current_mut().metadata.thread_title = Some("waiting".to_string());

    app.refresh_terminal_title();
    let frame1 = app.global_ui.last_terminal_title.clone().unwrap();

    // 菊花按单调时钟呼吸（对齐参考实现：2s 往返、端点停留约 205ms），
    // 因此必须真的等过一个字形停留窗口，光推进 tick 计数不再改变字形。
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.session_mgr.current_mut().spinner_state.advance_tick();
    app.refresh_terminal_title();
    let frame2 = app.global_ui.last_terminal_title.clone().unwrap();

    assert_ne!(
        frame1, frame2,
        "等后台子 agent 期间标题动画应继续旋转，不能停在静态状态"
    );
}
