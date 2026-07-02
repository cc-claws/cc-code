use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use crate::app::panel_manager::PanelState;
use crate::app::{App, BackgroundShell};
use super::{BackgroundTasksPanel, BackgroundTaskView};
use crate::shell_exec::CommandOutput;
use peri_agent::shell::ShellAbortHandle;

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
    app.session_mgr
        .current_mut()
        .background_shells
        .push(bg);
}

/// helper：打开 BackgroundTasks 面板并直接进入 Detail 视图
fn open_detail_panel(app: &mut App, item_id: &str) {
    let mut panel = BackgroundTasksPanel::new();
    panel.view = BackgroundTaskView::Detail {
        item_id: item_id.to_string(),
    };
    app.session_mgr
        .current_mut()
        .session_panels
        .open(PanelState::BackgroundTasks(panel));
}

/// helper：生成 N 行带编号的输出文本
fn make_output_lines(n: usize) -> String {
    (1..=n)
        .map(|i| format!("[{:02}/{}] 13:35:{} line {}", i, n, i, i))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_detail_output_大终端显示全部行() {
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
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
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
async fn test_detail_output_小终端按可用高度截断() {
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
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
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
    assert!(
        count > 0,
        "应至少显示 1 行输出，实际显示 {} 行",
        count
    );
}

#[tokio::test]
async fn test_detail_output_空输出显示0行() {
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
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
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
