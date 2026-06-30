use super::*;
use std::time::Duration;

#[tokio::test]
async fn test_spawn_stall_watchdog_检测stall并通知() {
    // Arrange：output 文件末行匹配 prompt pattern，且不增长（模拟命令等待输入）
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.output");
    tokio::fs::write(&path, "running...\nContinue? (y/n)")
        .await
        .unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(8);
    let handle = spawn_stall_watchdog(
        "task-1".to_string(),
        "rm -i node_modules".to_string(),
        path,
        tx,
    );

    // Act：等待 stall 通知（cfg(test) 下 STALL_CHECK_INTERVAL_MS=200、STALL_THRESHOLD_MS=500，约 0.8-1s 触发）
    let event = tokio::time::timeout(Duration::from_secs(4), rx.recv()).await;

    // Assert
    assert!(
        event.is_ok(),
        "应在 stall 阈值后收到 BackgroundShellStalled 通知"
    );
    let event = event.unwrap().unwrap();
    assert!(
        matches!(
            event,
            AgentEvent::BackgroundShellStalled { ref task_id, ref last_output, .. }
                if task_id == "task-1" && last_output.contains("(y/n)")
        ),
        "应是 BackgroundShellStalled 且携带 task_id/last_output"
    );
    // watchdog one-shot，触发后 task 自然结束
    let _ = handle.await;
}

#[tokio::test]
async fn test_spawn_stall_watchdog_末行不匹配时不通知() {
    // Arrange：output 文件无增长，但末行不匹配 prompt pattern（普通输出）
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.output");
    tokio::fs::write(&path, "compiling...\nfinished step 42")
        .await
        .unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(8);
    let handle = spawn_stall_watchdog("task-2".to_string(), "build".to_string(), path, tx);

    // Act：等待超过 stall 阈值（cfg(test) 500ms），不应收到通知（末行不匹配 pattern）
    let event = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await;

    // Assert：timeout 返回 Err 表示未收到通知
    assert!(
        event.is_err(),
        "末行不匹配 prompt pattern 时不应触发 stall 通知"
    );
    handle.abort();
}

#[test]
fn test_shell_completion_notification_完成格式() {
    // Arrange
    let path = Path::new("/tmp/peri-1000/app/sess/tasks/abc.output");
    // Act
    let msg = shell_completion_notification("abc", "npm test", Some(0), path);
    // Assert
    assert!(
        msg.contains("<background-task-completed>"),
        "缺少 XML 根标签: {}",
        msg
    );
    assert!(
        msg.contains("<task-id>abc</task-id>"),
        "缺少 task-id: {}",
        msg
    );
    assert!(
        msg.contains("<command>npm test</command>"),
        "缺少 command: {}",
        msg
    );
    assert!(msg.contains("completed (exit 0)"), "缺少完成状态: {}", msg);
    assert!(msg.contains("abc.output"), "缺少 output 路径: {}", msg);
}

#[test]
fn test_shell_notification_display_text_完成提示不泄露_xml() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification("abc", "npm test", Some(0), path);
    // Act
    let display = shell_notification_display_text(&msg).expect("应识别后台 shell 完成通知");
    // Assert
    assert!(
        display.contains("后台 shell 已完成"),
        "应显示完成提示: {}",
        display
    );
    assert!(display.contains("npm test"), "应保留命令摘要: {}", display);
    assert!(
        !display.contains("<background-task-completed>"),
        "不应泄露 XML 标签: {}",
        display
    );
}

#[test]
fn test_shell_notification_display_text_支持_system_reminder_包裹() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification("abc", "cargo test", Some(0), path);
    let wrapped = format!("<system-reminder>\n{}\n</system-reminder>", msg);
    // Act
    let display = shell_notification_display_text(&wrapped).expect("应识别包裹后的后台 shell 通知");
    // Assert
    assert!(
        display.contains("后台 shell 已完成"),
        "应显示完成提示: {}",
        display
    );
    assert!(
        display.contains("cargo test"),
        "应保留命令摘要: {}",
        display
    );
    assert!(
        !display.contains("<system-reminder>"),
        "不应泄露 system-reminder 标签: {}",
        display
    );
}

#[test]
fn test_shell_notification_display_text_清理终端控制序列() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification(
        "abc",
        "git \u{1b}[31mfetch\u{1b}[0m origin \u{1b}[<555;106;49M",
        Some(0),
        path,
    );
    // Act
    let display = shell_notification_display_text(&msg).expect("应识别后台 shell 完成通知");
    // Assert
    assert!(
        !display.contains('\u{1b}'),
        "不应保留 ESC 控制字符: {}",
        display
    );
    assert!(
        !display.contains("[<555;106;49M"),
        "不应保留 SGR 鼠标坐标序列: {}",
        display
    );
    assert!(
        display.contains("git fetch origin"),
        "普通命令文本应保留: {}",
        display
    );
}

#[test]
fn test_shell_notification_display_text_等待输入提示() {
    // Arrange
    let msg = shell_stalled_notification("t1", "npm publish", "continue?");
    // Act
    let display = shell_notification_display_text(&msg).expect("应识别等待输入通知");
    // Assert
    assert!(
        display.contains("后台 shell 等待输入"),
        "应显示等待输入: {}",
        display
    );
    assert!(
        display.contains("npm publish"),
        "应保留命令摘要: {}",
        display
    );
}

#[test]
fn test_shell_completion_notification_失败状态() {
    // Arrange
    let path = Path::new("/tmp/x.output");
    // Act
    let msg = shell_completion_notification("t1", "npm test", Some(1), path);
    // Assert
    assert!(
        msg.contains("failed (exit 1)"),
        "失败时应显示 failed 状态: {}",
        msg
    );
}

#[test]
fn test_shell_completion_notification_terminated状态() {
    // Arrange
    let path = Path::new("/tmp/y.output");
    // Act
    let msg = shell_completion_notification("t2", "cmd", None, path);
    // Assert
    assert!(
        msg.contains("terminated"),
        "exit_code=None 时应显示 terminated: {}",
        msg
    );
}

#[test]
fn test_shell_completion_notification_转义特殊字符() {
    // Arrange：command 含 XML 特殊字符（> < &，如 shell 重定向/链式命令）
    let path = Path::new("/tmp/x.output");
    // Act
    let msg = shell_completion_notification("t1", "echo hi > log.txt && cat < in", Some(0), path);
    // Assert：特殊字符应被转义，避免破坏 XML 结构
    assert!(msg.contains("&gt;"), "> 应转义为 &gt;: {}", msg);
    assert!(msg.contains("&lt;"), "< 应转义为 &lt;: {}", msg);
    assert!(msg.contains("&amp;"), "& 应转义为 &amp;: {}", msg);
    assert!(
        !msg.contains("> log"),
        "原始 > 不应残留（会破坏 XML）: {}",
        msg
    );
}
