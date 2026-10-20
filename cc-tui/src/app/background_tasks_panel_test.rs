use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::{task_items, BackgroundTaskView, BackgroundTasksPanel};
use crate::app::agent_shell_executor::{AgentShellRegistration, AgentShellSlot};
use crate::app::panel_component::PanelComponent;
use crate::app::panel_manager::{PanelContext, PanelState};
use crate::app::{App, BackgroundShell};
use crate::shell_exec::CommandOutput;
use crate::test_lock::PROCESS_TEST_TIMEOUT;
use cc_agent::shell::ShellAbortHandle;
use tui_textarea::{Input, Key};

fn make_shell_item(id: &str) -> String {
    id.to_string()
}

fn make_panel_input(key: Key, ctrl: bool) -> Input {
    Input {
        key,
        ctrl,
        alt: false,
        shift: false,
    }
}

#[test]
fn test_background_tasks_selection_tracks_id_after_preceding_row_disappears() {
    let mut panel = BackgroundTasksPanel::new();
    panel.refresh_items(vec![
        make_shell_item("a"),
        make_shell_item("b"),
        make_shell_item("c"),
    ]);
    panel.select_shell("b".to_string());
    panel.refresh_items(vec![make_shell_item("b"), make_shell_item("c")]);
    assert_eq!(
        panel.selected_item,
        Some(make_shell_item("b")),
        "选中项不能跟随旧索引漂移"
    );
    assert_eq!(panel.selected_index(), Some(0), "高亮随同一任务移动");
}

#[test]
fn test_background_tasks_selection_disappearing_requires_explicit_navigation() {
    let mut panel = BackgroundTasksPanel::new();
    panel.refresh_items(vec![
        make_shell_item("a"),
        make_shell_item("b"),
        make_shell_item("c"),
    ]);
    panel.select_shell("b".to_string());
    panel.refresh_items(vec![make_shell_item("a"), make_shell_item("c")]);
    assert_eq!(
        panel.selected_item,
        Some(make_shell_item("b")),
        "保留消失目标而非偷偷替换"
    );
    assert_eq!(panel.selected_index(), None, "不能高亮其他任务");
    panel.move_selection(true);
    assert_eq!(
        panel.selected_item,
        Some(make_shell_item("a")),
        "显式向下才重新选择首项"
    );
    panel.move_selection(true);
    assert_eq!(panel.selected_item, Some(make_shell_item("c")));
    panel.move_selection(true);
    assert_eq!(
        panel.selected_item,
        Some(make_shell_item("c")),
        "下移不越界"
    );
    panel.move_selection(false);
    assert_eq!(panel.selected_item, Some(make_shell_item("a")));
}

#[tokio::test]
async fn test_background_tasks_stop_keeps_target_before_next_render() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    for id in ["a", "b", "c"] {
        inject_bg_shell(&mut app, id, PathBuf::from("unused.output"));
    }
    let mut panel = BackgroundTasksPanel::new();
    panel.refresh_items(task_items(app.session_mgr.current()));
    panel.select_shell("b".to_string());
    app.session_mgr.current_mut().background_shells.remove(0);
    let mut ctx = PanelContext {
        services: &mut app.services,
        session_mgr: &mut app.session_mgr,
        acp_client: None,
    };
    panel.handle_key(make_panel_input(Key::Char('x'), false), &mut ctx);
    let shells = &ctx.session_mgr.current().background_shells;
    assert_eq!(shells[0].id, "b");
    assert_eq!(
        shells[0].status,
        crate::app::ShellStatus::Killed,
        "仍然停止原选中的 b"
    );
    assert_eq!(
        shells[1].status,
        crate::app::ShellStatus::Running,
        "旧索引上的 c 不受影响"
    );
}

#[tokio::test]
async fn test_background_tasks_missing_target_does_not_apply_actions_to_replacement() {
    let (mut app, _handle) = App::new_headless(80, 30).await;
    for id in ["a", "b", "c"] {
        inject_bg_shell(&mut app, id, PathBuf::from("unused.output"));
    }
    let mut panel = BackgroundTasksPanel::new();
    panel.refresh_items(task_items(app.session_mgr.current()));
    panel.select_shell("b".to_string());
    app.session_mgr.current_mut().background_shells.remove(1);
    let mut ctx = PanelContext {
        services: &mut app.services,
        session_mgr: &mut app.session_mgr,
        acp_client: None,
    };
    for refresh in [false, true] {
        if refresh {
            panel.refresh_items(task_items(ctx.session_mgr.current()));
        }
        panel.handle_key(make_panel_input(Key::Char('x'), false), &mut ctx);
        panel.handle_key(make_panel_input(Key::Char('b'), true), &mut ctx);
        panel.handle_key(make_panel_input(Key::Enter, false), &mut ctx);
        assert_eq!(
            panel.view,
            BackgroundTaskView::List,
            "目标消失时不能打开替补行详情"
        );
        assert!(
            ctx.session_mgr
                .current()
                .background_shells
                .iter()
                .all(|shell| { shell.status == crate::app::ShellStatus::Running }),
            "重绘前后都不能停止其他任务"
        );
    }
}

/// helper：构造后台 shell 并注入 app
fn inject_bg_shell(app: &mut App, id: &str, output_path: PathBuf) {
    let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
    let bg = BackgroundShell::new(
        id.to_string(),
        "python kcb50.py".to_string(),
        PathBuf::from("."),
        output_path,
        rx,
        ShellAbortHandle::noop(),
        Instant::now(),
    );
    app.session_mgr.current_mut().background_shells.push(bg);
}

/// helper：打开 BackgroundTasks 面板并直接进入 Detail 视图
fn open_detail_panel(app: &mut App, item_id: &str) {
    let mut panel = BackgroundTasksPanel::new();
    panel.view = BackgroundTaskView::Detail {
        item_id: item_id.to_string(),
    };
    app.open_panel(PanelState::BackgroundTasks(panel));
}

/// 等真实 read_tail 推送就绪，再由下一次 render 消费，避免依赖磁盘在 100ms 内完成。
async fn wait_for_detail_output(app: &mut App) {
    let panel = app
        .global_panels
        .get_mut::<BackgroundTasksPanel>()
        .expect("后台任务面板应位于 global scope");
    let rx = panel.output_rx.as_ref().expect("首次渲染应启动输出读取");
    tokio::time::timeout(PROCESS_TEST_TIMEOUT, async {
        while rx.is_empty() {
            assert!(!rx.is_closed(), "输出通道不能在首次推送前关闭");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("应收到磁盘输出，不以固定 sleep 代替就绪信号");
}

/// helper：生成 N 行带编号的输出文本
fn make_output_lines(n: usize) -> String {
    (1..=n)
        .map(|i| format!("[{:02}/{}] 13:35:{} line {}", i, n, i, i))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_detail_output_shows_all_lines_in_large_terminal() {
    // Arrange：20 行输出 + 80x40 终端（output inner 约 28 行，远超旧常量 10）
    let tmp = tempfile::tempdir().unwrap();
    let output_path = tmp.path().join("out.output");
    tokio::fs::write(&output_path, make_output_lines(20))
        .await
        .unwrap();

    let (mut app, mut handle) = App::new_headless(80, 40).await;
    inject_bg_shell(&mut app, "task-big", output_path);
    open_detail_panel(&mut app, "task-big");

    // Act：渲染两次——首次触发后台 read_tail task，第二次读取 output_cache
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    wait_for_detail_output(&mut app).await;
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();

    // Assert：应显示 > 10 行（旧常量限制为 10，修复后应更多）
    let snap = handle.snapshot().join("\n");
    let showing_line = snap
        .lines()
        .find(|l| l.contains("Showing") && l.contains("lines"))
        .unwrap_or_else(|| panic!("未找到 'Showing N lines' 行:\n{}", snap));
    let count: usize = showing_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(
        count > 10,
        "80x40 终端应显示超过旧常量 10 行，实际显示 {} 行:\n{}",
        count,
        snap
    );
}

#[tokio::test]
async fn test_detail_output_truncates_to_available_height_in_small_terminal() {
    // Arrange：30 行输出 + 80x15 终端（output inner 约 5 行，远少于 30）
    let tmp = tempfile::tempdir().unwrap();
    let output_path = tmp.path().join("out.output");
    tokio::fs::write(&output_path, make_output_lines(30))
        .await
        .unwrap();

    let (mut app, mut handle) = App::new_headless(80, 15).await;
    inject_bg_shell(&mut app, "task-small", output_path);
    open_detail_panel(&mut app, "task-small");

    // Act
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    wait_for_detail_output(&mut app).await;
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();

    // Assert：显示行数应 < 30（被终端高度截断）
    let snap = handle.snapshot().join("\n");
    let showing_line = snap
        .lines()
        .find(|l| l.contains("Showing") && l.contains("lines"))
        .unwrap_or_else(|| panic!("未找到 'Showing N lines' 行:\n{}", snap));
    // 提取 "Showing N" 中的 N
    let count: usize = showing_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(
        count < 30,
        "80x15 终端不应显示全部 30 行，实际显示 {} 行",
        count
    );
    assert!(count > 0, "应至少显示 1 行输出，实际显示 {} 行", count);
}

#[tokio::test]
async fn test_detail_output_shows_zero_lines_for_empty_output() {
    // Arrange：空输出文件
    let tmp = tempfile::tempdir().unwrap();
    let output_path = tmp.path().join("out.output");
    tokio::fs::write(&output_path, "").await.unwrap();

    let (mut app, mut handle) = App::new_headless(80, 30).await;
    inject_bg_shell(&mut app, "task-empty", output_path);
    open_detail_panel(&mut app, "task-empty");

    // Act
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    wait_for_detail_output(&mut app).await;
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();

    // Assert
    let snap = handle.snapshot().join("\n");
    assert!(
        snap.contains("Showing 0 lines"),
        "空输出应显示 0 行，实际:\n{}",
        snap
    );
}

/// 子 Agent 不进「后台任务」面板：它们已经由底栏抽屉逐行展示，再来一份只是
/// 重复入口、还会让人分不清哪份是权威。
#[tokio::test]
async fn test_background_tasks_panel_omits_local_agents_section() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.handle_subagent_lifecycle(&serde_json::json!({
        "event": "started",
        "instanceId": "01a15876-ad99-7bf0-a09c-1d0a2ccde4a0",
        "agentType": "general-purpose",
        "taskPreview": "check weather",
        "isBackground": true,
    }));
    app.open_background_tasks_panel();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    assert!(
        !snap.contains("LOCAL AGENTS"),
        "面板不得再列 Local agents 分组：\n{snap}"
    );
    // 抽屉行是 `◯ general-purpose   任务`，面板行曾是 `general-purpose "<id>"`
    assert!(
        !snap.contains("\"01a15876-ad99-7bf0-a09c-1d0a2ccde4a0\""),
        "面板不得再列子 Agent 行：\n{snap}"
    );
}

/// 回归：仍在**前台**运行的 shell 不得出现在「后台任务」面板里。
///
/// 症状：面板 SHELLS 分组下多出一条 `[前台] sleep 590; ...`，用户在抽屉里
/// 已经看到它在主时间线即时输出，再在「后台任务」里看到一次只会以为还有第二条
/// 任务在跑。
#[tokio::test]
async fn test_background_tasks_panel_excludes_foreground_shell() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    {
        let session = app.session_mgr.current_mut();
        session
            .agent_shells
            .push(make_agent_shell("fg-task", "sleep 590; echo done", false));
        session
            .agent_shells
            .push(make_agent_shell("bg-task", "cargo build", true));
    }
    app.open_background_tasks_panel();
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    assert!(
        !snap.contains("sleep 590"),
        "前台运行的 shell 不得出现在后台任务面板：\n{snap}"
    );
    assert!(
        snap.contains("SHELLS (1)"),
        "只应统计已后台化的那条：\n{snap}"
    );
    assert!(
        snap.contains("cargo build"),
        "已后台化的 shell 应显示：\n{snap}"
    );
}

/// helper：构造 agent shell 槽位（`backgrounded=false` 即前台运行中）。
fn make_agent_shell(task_id: &str, command: &str, backgrounded: bool) -> AgentShellSlot {
    AgentShellSlot::from_registration(AgentShellRegistration {
        task_id: task_id.to_string(),
        owner_session_id: None,
        tool_call_id: None,
        source_agent_id: None,
        execution_timeout_ms: 600_000,
        timeout_ms: 120_000,
        command: command.to_string(),
        cwd: ".".to_string(),
        output_path: PathBuf::from("unused.output"),
        exit_signal: std::sync::Arc::new(cc_agent::shell::ExitSignal::new()),
        handoff: std::sync::Arc::new(cc_agent::shell::ShellHandoff::new(true, backgrounded)),
        kill: ShellAbortHandle::noop(),
        started_instant: Instant::now(),
        direct_background: backgrounded,
    })
}
