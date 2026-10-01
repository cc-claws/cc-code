use super::*;
use std::{path::PathBuf, sync::Arc};

use cc_agent::messages::BaseMessage;
use cc_agent::shell::{ExitSignal, ShellAbortHandle, ShellHandoff};

use crate::app::{AgentShellRegistration, AgentShellSlot};

#[tokio::test]
async fn test_ctrl_b_multiple_shells_selects_only_one_through_keyboard_dispatch() {
    use crate::app::panel_manager::PanelKind;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (mut app, _handle) = App::new_headless(100, 30).await;
    for _ in 0..2 {
        let (slot, _) = make_agent_shell_slot(false, "same command");
        app.session_mgr.current_mut().agent_shells.push(slot);
    }
    let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    crate::event::keyboard::handle_key_event(&mut app, ctrl_b).expect("按键处理应成功");
    assert!(app.global_panels.is_active(PanelKind::BackgroundTasks));
    assert!(
        app.session_mgr
            .current()
            .agent_shells
            .iter()
            .all(|s| !s.is_backgrounded()),
        "首次只打开选择面板"
    );
    crate::event::keyboard::handle_key_event(
        &mut app,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
    )
    .expect("选择第二项");
    crate::event::keyboard::handle_key_event(&mut app, ctrl_b).expect("选中命令转后台");
    let slots = &app.session_mgr.current().agent_shells;
    assert!(!slots[0].is_backgrounded(), "不能批量后台化或重置为第一项");
    assert!(slots[1].is_backgrounded(), "面板必须只移交选中的任务");
}

#[tokio::test]
async fn test_ctrl_b_rejected_handoff_does_not_create_background_notification() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.set_loading(true);
    // 模拟命令已退出但工具尚未收口：不能再承诺 Ctrl+B。
    let (slot, exit_signal) = make_agent_shell_slot(false, "sleep 30");
    slot.handoff.settle_foreground();
    exit_signal.finish(cc_agent::shell::ShellOutcome::Exited(0));
    app.session_mgr.current_mut().agent_shells.push(slot);
    assert!(
        !app.background_agent_foreground(),
        "关闭的交接入口必须拒绝后台化"
    );
    let slot = &app.session_mgr.current().agent_shells[0];
    assert!(!slot.is_backgrounded(), "不能留下幽灵后台任务");
    assert!(slot.stall_watchdog.is_none(), "失败不得启动 watchdog");
    assert!(app.poll_agent_shells());
    assert!(
        app.session_mgr
            .current()
            .pending_bg_shell_notifications
            .is_empty(),
        "前台完成不能再生成后台通知"
    );
}

#[tokio::test]
async fn test_poll_agent_shells_reports_real_outcomes_once_even_after_early_exit() {
    use cc_agent::shell::ShellOutcome;
    let (mut app, _handle) = App::new_headless(100, 30).await;
    app.set_loading(true);
    for (index, (outcome, expected)) in [
        (ShellOutcome::Exited(0), "completed (exit 0)"),
        (ShellOutcome::Exited(7), "failed (exit 7)"),
        (ShellOutcome::TimedOut, "timed out"),
        (ShellOutcome::Cancelled, "cancelled"),
    ]
    .into_iter()
    .enumerate()
    {
        let slot = make_auto_background_agent_shell_slot("same command");
        // 命令在 UI 首次 poll 之前结束，但已成功向工具返回后台句柄。
        slot.exit_signal.finish(outcome);
        app.session_mgr.current_mut().agent_shells.push(slot);
        assert!(app.poll_agent_shells());
        let notifications = &mut app.session_mgr.current_mut().pending_bg_shell_notifications;
        assert_eq!(notifications.len(), 1, "每个后台任务恰好一次通知");
        let notification = notifications.pop_front().expect("完成通知");
        assert!(
            notification.content.contains(expected),
            "不能丢失真实退出原因: {}",
            notification.content
        );
        let display =
            super::background_shell::shell_notification_display_text(&notification.content)
                .expect("用户可读通知");
        if expected == "timed out" || expected == "cancelled" {
            assert!(!display.contains("已完成"), "超时或取消不能显示成功");
        }
        let changed = app.poll_agent_shells();
        assert!(
            !changed,
            "不能重复注入通知: iteration={index}, pending={}, slots={:?}",
            app.session_mgr
                .current()
                .pending_bg_shell_notifications
                .len(),
            app.session_mgr
                .current()
                .agent_shells
                .iter()
                .map(|slot| (slot.ended, slot.completion_notified, slot.is_backgrounded()))
                .collect::<Vec<_>>()
        );
    }
}

fn make_record(
    thread_id: &str,
    command: &str,
    anchor_message_id: Option<String>,
) -> ShellCommandRecord {
    let now = Utc::now();
    ShellCommandRecord {
        id: uuid::Uuid::now_v7().to_string(),
        thread_id: thread_id.to_string(),
        command: command.to_string(),
        cwd: ".".to_string(),
        stdin: Vec::new(),
        stdout: "done".to_string(),
        stderr: String::new(),
        exit_code: 0,
        started_at: now,
        completed_at: now,
        anchor_message_id,
    }
}

fn make_agent_shell_slot(
    direct_background: bool,
    command: &str,
) -> (AgentShellSlot, Arc<ExitSignal>) {
    let (reg, exit_signal) = make_agent_shell_registration(direct_background, command);
    (AgentShellSlot::from_registration(reg), exit_signal)
}

fn make_agent_shell_registration(
    direct_background: bool,
    command: &str,
) -> (AgentShellRegistration, Arc<ExitSignal>) {
    let exit_signal = Arc::new(ExitSignal::new());
    let reg = AgentShellRegistration {
        task_id: uuid::Uuid::now_v7().to_string(),
        owner_session_id: None,
        tool_call_id: Some("test-call".into()),
        source_agent_id: None,
        execution_timeout_ms: 600_000,
        command: command.to_string(),
        cwd: ".".to_string(),
        output_path: PathBuf::from("/tmp/cc-agent-shell.output"),
        exit_signal: Arc::clone(&exit_signal),
        handoff: Arc::new(ShellHandoff::new(true, direct_background)),
        kill: ShellAbortHandle::noop(),
        started_instant: std::time::Instant::now(),
        direct_background,
    };
    (reg, exit_signal)
}

fn make_auto_background_agent_shell_slot(command: &str) -> AgentShellSlot {
    let (slot, _) = make_agent_shell_slot(false, command);
    assert!(slot.handoff.background(), "自动后台必须经共享归属状态提交");
    slot
}

#[test]
fn test_set_pending_bash_tool_started_at_in_view() {
    let command = "echo hello";
    let mut view_messages = vec![MessageViewModel::tool_block_with_id(
        "test-call".into(),
        "Bash".to_string(),
        "Bash".to_string(),
        Some(command.to_string()),
        false,
    )];
    let spawned_at = std::time::Instant::now() - std::time::Duration::from_secs(3);

    let mut pipeline = crate::app::message_pipeline::MessagePipeline::new(".".into());
    pipeline.register_shell_runtime(None, "test-call", spawned_at, 600_000);
    assert!(
        pipeline.apply_shell_runtime(&mut view_messages),
        "应能回填当前 view 中的 pending Bash ToolBlock"
    );

    let MessageViewModel::ToolBlock { started_at, .. } = &view_messages[0] else {
        panic!("测试数据应为 ToolBlock");
    };
    assert!(
        started_at
            .as_ref()
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2)),
        "回填后 Ctrl+B 提示应按真实 spawn 时间计时"
    );
}

#[tokio::test]
async fn test_ctrl_b_background_state_reaches_pending_bash_view_immediately() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::tool_block_with_id(
            "test-call".into(),
            "Bash".to_string(),
            "Bash".to_string(),
            Some("sleep 30".to_string()),
            false,
        )];
    let (registration, _) = make_agent_shell_registration(false, "sleep 30");
    app.register_agent_shell(registration);
    assert!(
        matches!(
            app.session_mgr.current().messages.view_messages.first(),
            Some(MessageViewModel::ToolBlock {
                shell_backgrounded: false,
                started_at: Some(_),
                ..
            })
        ),
        "前台注册应关联 Bash 消息的启动时间"
    );

    assert!(app.background_agent_foreground(), "Ctrl+B 应成功移交任务");
    assert!(
        matches!(
            app.session_mgr.current().messages.view_messages.first(),
            Some(MessageViewModel::ToolBlock {
                shell_backgrounded: true,
                execution_timeout_ms: Some(600_000),
                ..
            })
        ),
        "Ctrl+B 必须立即把后台状态传到原始 Bash 调用"
    );
    if let Some(watchdog) = app.session_mgr.current_mut().agent_shells[0]
        .stall_watchdog
        .take()
    {
        watchdog.abort();
    }
}

#[tokio::test]
async fn test_poll_agent_shells_syncs_panel_handoff_into_bash_view() {
    let (mut app, _handle) = App::new_headless(100, 30).await;
    app.session_mgr.current_mut().messages.view_messages =
        vec![MessageViewModel::tool_block_with_id(
            "test-call".into(),
            "Bash".to_string(),
            "Bash".to_string(),
            Some("sleep 30".to_string()),
            false,
        )];
    let (registration, _) = make_agent_shell_registration(false, "sleep 30");
    app.register_agent_shell(registration);
    let transitioned = {
        let slot = &mut app.session_mgr.current_mut().agent_shells[0];
        super::background_agent_slot(slot, app.services.bg_event_tx.clone())
    };
    assert!(transitioned, "任务面板的 Ctrl+B 应完成后台移交");
    assert!(app.poll_agent_shells(), "主循环轮询应同步 UI 后台状态");
    assert!(
        matches!(
            app.session_mgr.current().messages.view_messages.first(),
            Some(MessageViewModel::ToolBlock {
                shell_backgrounded: true,
                execution_timeout_ms: Some(600_000),
                ..
            })
        ),
        "任务面板移交也必须更新原 Bash 行"
    );
    if let Some(watchdog) = app.session_mgr.current_mut().agent_shells[0]
        .stall_watchdog
        .take()
    {
        watchdog.abort();
    }
}

#[tokio::test]
async fn test_merge_shell_records_inserts_after_anchor_without_origin_messages() {
    let (app, _handle) = App::new_headless(80, 24).await;
    let base_msgs = vec![BaseMessage::human("q1"), BaseMessage::ai("a1")];
    let anchor_id = base_msgs[0].id().as_uuid().to_string();
    let view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, ".");
    let record = make_record("thread-a", "echo done", Some(anchor_id));

    let merged = app.merge_shell_records_into_view(view_msgs, &base_msgs, vec![record]);

    assert!(
        matches!(merged.get(1), Some(MessageViewModel::ShellCommand { command, .. }) if command == "echo done"),
        "shell 记录应按锚点插入到对应 BaseMessage 后"
    );
    assert_eq!(
        base_msgs.len(),
        2,
        "合并 shell VM 不应改变 Agent BaseMessage"
    );
}

#[tokio::test]
async fn test_merge_shell_records_without_anchor_stays_at_thread_start() {
    let (app, _handle) = App::new_headless(80, 24).await;
    let base_msgs = vec![BaseMessage::human("q1")];
    let view_msgs = message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, ".");
    let record = make_record("thread-a", "pwd", None);

    let merged = app.merge_shell_records_into_view(view_msgs, &base_msgs, vec![record]);

    assert!(
        matches!(merged.first(), Some(MessageViewModel::ShellCommand { command, .. }) if command == "pwd"),
        "无 Agent 锚点的 shell-only 记录应恢复到 thread 开头"
    );
}

#[tokio::test]
async fn test_cancel_shell_command_aborts_task_and_replaces_pending_vm() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let record_id = uuid::Uuid::now_v7().to_string();
    let thread_id = "thread-shell-cancel".to_string();
    let task = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    let abort_handle = ShellAbortHandle::from_tokio_abort(task.abort_handle());

    app.session_mgr.current_mut().current_thread_id = Some(thread_id.clone());
    app.session_mgr.current_mut().messages.view_messages.push(
        MessageViewModel::shell_command_pending(
            record_id.clone(),
            "sleep 60".to_string(),
            ".".to_string(),
        ),
    );
    app.session_mgr.current_mut().shell_pool.foreground.runtime = ShellCommandRuntime {
        stdin_tx: None,
        running_record_id: Some(record_id.clone()),
        stdin_lines: vec!["hello".to_string()],
        abort_handle: Some(abort_handle),
        command: "sleep 60".to_string(),
        cwd: ".".to_string(),
        thread_id: Some(thread_id),
        started_at: Some(Utc::now()),
        anchor_message_id: None,
    };
    app.set_loading(true);

    assert!(app.cancel_shell_command(), "应成功取消运行中的 shell 命令");
    let join_result = task.await;
    assert!(
        join_result.unwrap_err().is_cancelled(),
        "取消 shell 命令应 abort 后台任务"
    );
    assert!(
        !app.session_mgr
            .current()
            .shell_pool
            .foreground
            .runtime
            .is_running(),
        "取消后应清理 ShellCommandRuntime"
    );
    assert!(
        !app.session_mgr.current().ui.loading,
        "取消后应退出 loading"
    );
    assert!(
        matches!(
            app.session_mgr.current().messages.view_messages.last(),
            Some(MessageViewModel::ShellCommand {
                id,
                stderr,
                exit_code: Some(-1),
                ..
            }) if id == &record_id && stderr.contains("cancelled")
        ),
        "pending shell VM 应替换为取消结果"
    );
}

#[tokio::test]
async fn test_poll_agent_shells_skips_background_notification_when_foreground_finishes() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.set_loading(true);
    let (slot, exit_signal) = make_agent_shell_slot(false, "echo hi");
    slot.handoff.settle_foreground();
    app.session_mgr.current_mut().agent_shells.push(slot);

    exit_signal.fire();
    let changed = app.poll_agent_shells();

    assert!(changed, "前台 shell 退出也应产生状态变化用于重绘");
    assert!(
        !app.session_mgr.current().ui.force_terminal_clear_redraw,
        "agent Bash 退出后不再请求物理清屏，依赖 Ratatui 单元格 Diff 平滑重绘"
    );
    assert!(
        app.session_mgr.current().agent_shells[0].ended,
        "退出后应标记 ended"
    );
    assert!(
        app.session_mgr
            .current()
            .pending_bg_shell_notifications
            .is_empty(),
        "未后台化的前台小命令不应注入后台完成通知"
    );
}

#[tokio::test]
async fn test_poll_agent_shells_auto_backgrounds_and_keeps_running_on_timeout() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let slot = make_auto_background_agent_shell_slot("python long.py");
    app.session_mgr.current_mut().agent_shells.push(slot);
    let changed = app.poll_agent_shells();

    assert!(changed, "自动后台化应产生状态变化");
    assert!(
        app.session_mgr.current().agent_shells[0].is_backgrounded(),
        "自动后台化应通过共享归属状态切到后台继续运行"
    );
    assert!(
        !app.session_mgr.current().ui.force_terminal_clear_redraw,
        "自动后台化后不再请求物理清屏，依赖 Ratatui 单元格 Diff 平滑重绘"
    );
}

#[tokio::test]
async fn test_register_agent_shell_no_longer_requests_physical_clear() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (reg, _exit_signal) = make_agent_shell_registration(false, "sleep 3");

    app.register_agent_shell(reg);

    assert!(
        !app.session_mgr.current().ui.force_terminal_clear_redraw,
        "agent Bash 注册到状态栏后不再请求物理清屏，依赖 Ratatui 单元格 Diff 平滑重绘"
    );
}

#[tokio::test]
async fn test_background_agent_foreground_no_longer_requests_physical_clear() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (slot, _exit_signal) = make_agent_shell_slot(false, "sleep 60");
    app.session_mgr.current_mut().agent_shells.push(slot);

    assert!(
        app.background_agent_foreground(),
        "应能将前台 agent Bash 转入后台"
    );
    assert!(
        !app.session_mgr.current().ui.force_terminal_clear_redraw,
        "agent Bash 后台化后不再请求物理清屏，依赖 Ratatui 单元格 Diff 平滑重绘"
    );
}

#[tokio::test]
async fn test_poll_agent_shells_injects_notification_only_after_backgrounded_shell_finishes() {
    let (mut app, _handle) = App::new_headless(80, 24).await;
    app.set_loading(true);
    let (slot, exit_signal) = make_agent_shell_slot(true, "cargo test");
    app.session_mgr.current_mut().agent_shells.push(slot);

    exit_signal.fire();
    let changed = app.poll_agent_shells();

    assert!(changed, "后台 shell 退出应产生状态变化");
    let pending = &app.session_mgr.current().pending_bg_shell_notifications;
    assert_eq!(pending.len(), 1, "后台 shell 完成应注入一条通知");
    let notification = pending.front().expect("应有后台完成通知");
    assert!(
        notification.content.contains("<background-task-completed>"),
        "通知应保留 agent 可解析的 XML: {}",
        notification.content
    );
    assert!(
        notification
            .content
            .contains("<command>cargo test</command>"),
        "通知应包含命令: {}",
        notification.content
    );
}

#[tokio::test]
async fn test_cleanup_finished_background_shells_removes_oldest_finished_when_over_limit() {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;

    use crate::app::{BackgroundShell, ShellStatus};
    use crate::shell_exec::CommandOutput;

    let (mut app, _handle) = App::new_headless(80, 24).await;

    // 构造已完成任务的 helper（ended_at = ago_secs 前）
    let make_bg = |id: &str, ago_secs: u64| -> BackgroundShell {
        let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
        let mut bg = BackgroundShell::new(
            id.to_string(),
            "cmd".to_string(),
            PathBuf::from("."),
            PathBuf::from(format!("/tmp/peri-test-{}.output", id)),
            rx,
            ShellAbortHandle::noop(),
            std::time::Instant::now(),
        );
        bg.status = ShellStatus::Completed;
        bg.notified = true;
        bg.ended_at = Some(Instant::now() - Duration::from_secs(ago_secs));
        bg
    };

    // 填 21 个已完成任务（task-0 最旧，100s 前）
    for i in 0..21u64 {
        app.session_mgr
            .current_mut()
            .background_shells
            .push(make_bg(&format!("task-{}", i), 100 - i));
    }
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        21,
        "前置条件：21 个任务"
    );

    app.cleanup_finished_background_shells();
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        20,
        "超量时应移除最旧的 1 个已完成任务"
    );
    let remaining_ids: Vec<&str> = app
        .session_mgr
        .current()
        .background_shells
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert!(
        !remaining_ids.contains(&"task-0"),
        "最旧的 task-0 应被移除: {:?}",
        remaining_ids
    );
}

#[tokio::test]
async fn test_cleanup_finished_background_shells_keeps_all_when_under_limit() {
    use std::path::PathBuf;
    use tokio::sync::oneshot;

    use crate::app::{BackgroundShell, ShellStatus};
    use crate::shell_exec::CommandOutput;

    let (mut app, _handle) = App::new_headless(80, 24).await;
    let (_tx, rx) = oneshot::channel::<anyhow::Result<CommandOutput>>();
    let mut bg = BackgroundShell::new(
        "task-1".to_string(),
        "cmd".to_string(),
        PathBuf::from("."),
        PathBuf::from("/tmp/x.output"),
        rx,
        ShellAbortHandle::noop(),
        std::time::Instant::now(),
    );
    bg.status = ShellStatus::Completed;
    bg.notified = true;
    app.session_mgr.current_mut().background_shells.push(bg);

    app.cleanup_finished_background_shells();
    assert_eq!(
        app.session_mgr.current().background_shells.len(),
        1,
        "未超量时不应移除"
    );
}

// ── 前台 `!` 命令回流 Agent 上下文（shell_context_messages） ──────────────

fn make_context_record(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
) -> ShellCommandRecord {
    ShellCommandRecord {
        id: "01a0-test-record".to_string(),
        thread_id: ThreadId::from("test-thread".to_string()),
        command: command.to_string(),
        cwd: "/tmp".to_string(),
        stdin: Vec::new(),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code,
        started_at: Utc::now(),
        completed_at: Utc::now(),
        anchor_message_id: None,
    }
}

fn text_of(msg: &BaseMessage) -> String {
    msg.message_content().text_content()
}

#[test]
fn test_shell_context_messages_shape() {
    let record = make_context_record("deploy --prod", "done\n", "", 0);
    let msgs = shell_context_messages(&record);

    assert_eq!(msgs.len(), 3, "应为 caveat + input + output 三条");
    assert!(
        text_of(&msgs[0]).starts_with("<local-command-caveat>"),
        "首条应为 caveat，实际: {}",
        text_of(&msgs[0])
    );
    assert_eq!(text_of(&msgs[1]), "<bash-input>deploy --prod</bash-input>");
    let out = text_of(&msgs[2]);
    assert!(out.starts_with("<bash-stdout>"), "实际: {out}");
    assert!(out.contains("done"), "应含 stdout，实际: {out}");
    assert!(out.contains("<bash-stderr></bash-stderr>"), "实际: {out}");
}

#[test]
fn test_shell_context_messages_nonzero_exit_embeds_code() {
    let record = make_context_record("false", "", "boom\n", 1);
    let msgs = shell_context_messages(&record);
    let out = text_of(&msgs[2]);
    assert!(
        out.contains("[Exit code: 1]"),
        "非零退出码应内嵌于 stdout 段，实际: {out}"
    );
    assert!(
        out.contains("boom"),
        "stderr 应出现在独立标签内，实际: {out}"
    );
}

#[test]
fn test_shell_context_messages_xml_escaped() {
    // 命令与输出含 XML 特殊字符时应转义，避免破坏标签结构
    let record = make_context_record("echo '<a> & \"b\"'", "x < y && z > w\n", "", 0);
    let msgs = shell_context_messages(&record);
    let input = text_of(&msgs[1]);
    assert!(input.contains("&lt;a&gt;"), "命令应转义，实际: {input}");
    let out = text_of(&msgs[2]);
    assert!(out.contains("&lt;"), "输出应转义，实际: {out}");
    assert!(!out.contains("x < y"), "不应残留未转义的 `<`，实际: {out}");
}

/// 回归：`!` 回流片段被展示层跳过（不产生 VM），锚点计数必须采用同一口径，
/// 否则 shell 卡片位置会随 `!` 命令数量系统性右移。
#[tokio::test]
async fn test_merge_shell_records_anchor_not_drifted_by_context_fragments() {
    let (app, _handle) = App::new_headless(100, 30).await;

    let h1 = BaseMessage::human("第一轮");
    let a1 = BaseMessage::ai("回复一");
    let h2 = BaseMessage::human("第二轮");
    let a2 = BaseMessage::ai("回复二");
    let anchor_id = a2.id().as_uuid().to_string();
    let h3 = BaseMessage::human("第三轮");
    let a3 = BaseMessage::ai("回复三");
    let h4 = BaseMessage::human("第四轮");
    let a4 = BaseMessage::ai("回复四");

    let base_msgs = vec![
        h1,
        a1,
        // 三条回流片段：展示层跳过，不产生 VM
        BaseMessage::human("<local-command-caveat>Caveat: ...</local-command-caveat>"),
        BaseMessage::human("<bash-input>deploy</bash-input>"),
        BaseMessage::human("<bash-stdout>done</bash-stdout><bash-stderr></bash-stderr>"),
        h2,
        a2,
        h3,
        a3,
        h4,
        a4,
    ];

    let view_msgs =
        crate::app::message_pipeline::MessagePipeline::messages_to_view_models(&base_msgs, "/tmp");
    assert_eq!(view_msgs.len(), 8, "4 轮 × 2 条可见消息，片段不应产生 VM");

    let mut record = make_context_record("deploy", "done", "", 0);
    record.anchor_message_id = Some(anchor_id);

    let merged = app.merge_shell_records_into_view(view_msgs, &base_msgs, vec![record]);

    // a2 是第 4 条可见消息（VM index 3），卡片应紧随其后插入 index 4。
    // 若锚点把片段也计入，插入位置会右移到 index 7。
    assert!(
        matches!(merged[4], MessageViewModel::ShellCommand { .. }),
        "shell 卡片应插在 a2 之后（index 4），实际 index 4 是其它类型；总长 {}",
        merged.len()
    );
    assert_eq!(merged.len(), 9, "原有 8 条 + 插入 1 条");
}

/// 安全：`!` 输出会随 history 外发给模型，故凭据必须先脱敏。
#[test]
fn test_shell_context_messages_redacts_secrets() {
    let record = make_context_record(
        "curl -H 'Authorization: token ghp_abcdefghijklmnopqrstuvwxyz'",
        "api_key = supersecretvalue123\n正常输出",
        "password: hunter2hunter2",
        0,
    );
    let msgs = shell_context_messages(&record);
    let input = text_of(&msgs[1]);
    let out = text_of(&msgs[2]);

    // 命令前缀本身可读，但内嵌凭据必须被打码
    assert!(input.contains("<bash-input>"), "应仍是命令片段: {input}");
    assert!(
        !input.contains("ghp_abcdefghijklmnopqrstuvwxyz"),
        "命令中的 token 应脱敏: {input}"
    );
    assert!(
        !out.contains("supersecretvalue123"),
        "stdout 中的 key 应脱敏: {out}"
    );
    assert!(
        !out.contains("hunter2hunter2"),
        "stderr 中的 password 应脱敏: {out}"
    );
    assert!(out.contains("正常输出"), "非敏感内容应保留: {out}");
}
