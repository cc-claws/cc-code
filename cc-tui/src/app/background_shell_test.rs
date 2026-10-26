use super::*;
use crate::ui::message_view::NoteTone;
use std::time::Duration;

/// 显式中文语言注册表（验证 i18n 中文分支）。
fn lc_zh() -> crate::i18n::LcRegistry {
    crate::i18n::LcRegistry::new(Some("zh-CN"))
}

/// #320 Windows 根治：Defender/杀毒软件会对新文件短暂加锁扫描，
/// write 返回后文件可能暂时不可读。轮询等待文件稳定可读再继续，
/// 避免 watchdog 启动即遇到 PermissionDenied。
async fn wait_until_readable(path: &std::path::Path) {
    // Windows CI 上文件锁可能持续 10s+，100 次 * 100ms = 10s
    for _ in 0..100 {
        if tokio::fs::metadata(path).await.is_ok() {
            // 再确认能实际读出内容
            if tokio::fs::read(path).await.is_ok() {
                // 额外稳定期：确保不是"刚好在两次扫描间隙可读"
                tokio::time::sleep(Duration::from_millis(500)).await;
                if tokio::fs::read(path).await.is_ok() {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("测试文件在 10s 内始终不可读，Windows 文件锁异常");
}

#[tokio::test]
// #320: Windows CI runner 文件系统存在系统性问题，新写入文件 10s+ 不可读
// （Defender/慢盘/高负载），与 watchdog 逻辑无关。Linux/macOS 正常覆盖。
// 跟踪 issue：待创建
#[cfg_attr(target_os = "windows", ignore)]
async fn test_spawn_stall_watchdog_detects_stall_and_notifies() {
    // Arrange：output 文件末行匹配 prompt pattern，且不增长（模拟命令等待输入）
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.output");
    tokio::fs::write(&path, "running...\nContinue? (y/n)")
        .await
        .unwrap();
    wait_until_readable(&path).await;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(8);
    let handle = spawn_stall_watchdog(
        "task-1".to_string(),
        "rm -i node_modules".to_string(),
        Some("Remove node_modules interactively".to_string()),
        path,
        tx,
    );

    // Act：等待 stall 通知（cfg(test) 下 STALL_CHECK_INTERVAL_MS=200、STALL_THRESHOLD_MS=500，约 0.8-1s 触发）
    // Windows 文件锁可能让 watchdog 在错误重试中消耗十几秒；20s 覆盖重试窗口 + stall 阈值
    let event = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await;

    // Assert
    assert!(
        event.is_ok(),
        "应在 stall 阈值后收到 BackgroundShellStalled 通知"
    );
    let event = event.unwrap().unwrap();
    assert!(
        matches!(
            event,
            AgentEvent::BackgroundShellStalled { ref task_id, ref last_output, ref description, .. }
                if task_id == "task-1"
                    && last_output.contains("(y/n)")
                    && description.as_deref() == Some("Remove node_modules interactively")
        ),
        "应是 BackgroundShellStalled 且携带 task_id/last_output/description"
    );
    // watchdog one-shot，触发后 task 自然结束
    let _ = handle.await;
}

#[tokio::test]
// #320: 同上，Windows CI 文件系统问题，ignore
#[cfg_attr(target_os = "windows", ignore)]
async fn test_spawn_stall_watchdog_skips_notification_when_last_line_does_not_match() {
    // Arrange：output 文件无增长，但末行不匹配 prompt pattern（普通输出）
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.output");
    tokio::fs::write(&path, "compiling...\nfinished step 42")
        .await
        .unwrap();
    wait_until_readable(&path).await;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(8);
    let handle = spawn_stall_watchdog("task-2".to_string(), "build".to_string(), None, path, tx);

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
fn test_shell_completion_notification_completed_format() {
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
fn test_shell_notification_display_text_completion_does_not_leak_xml() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification("abc", "npm test", Some(0), path);
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别后台 shell 完成通知")
        .text;
    // Assert
    assert!(display.contains("已完成"), "应显示完成提示: {}", display);
    assert!(display.contains("npm test"), "应保留命令摘要: {}", display);
    assert!(
        !display.contains("<background-task-completed>"),
        "不应泄露 XML 标签: {}",
        display
    );
}

#[test]
fn test_shell_notification_display_text_supports_system_reminder_wrapper() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification("abc", "cargo test", Some(0), path);
    let wrapped = format!("<system-reminder>\n{}\n</system-reminder>", msg);
    // Act
    let display = shell_notification_display_text_with(&wrapped, &lc_zh())
        .expect("应识别包裹后的后台 shell 通知")
        .text;
    // Assert
    assert!(display.contains("已完成"), "应显示完成提示: {}", display);
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
fn test_shell_notification_display_text_strips_terminal_control_sequences() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification(
        "abc",
        "git \u{1b}[31mfetch\u{1b}[0m origin \u{1b}[<555;106;49M",
        Some(0),
        path,
    );
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别后台 shell 完成通知")
        .text;
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
fn test_shell_notification_display_text_waits_for_input_prompt() {
    // Arrange
    let msg = shell_stalled_notification("t1", "npm publish", None, "continue?");
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别等待输入通知")
        .text;
    // Assert
    assert!(
        display.contains("等待交互输入"),
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
fn test_shell_completion_notification_failure_status() {
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
fn test_shell_completion_notification_terminated_status() {
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
fn test_shell_completion_notification_escapes_special_characters() {
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

#[test]
fn test_shell_notification_display_text_english() {
    // 英文分支：展示文案应跟随传入的 en 语言注册表（不再是硬编码中文）。
    let lc_en = crate::i18n::LcRegistry::new(Some("en"));
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = shell_completion_notification("abc", "npm test", Some(0), path);
    let display = shell_notification_display_text_with(&msg, &lc_en)
        .expect("应识别后台 shell 完成通知")
        .text;
    assert!(
        display.contains("completed (exit code 0)"),
        "英文语言下应显示英文完成提示: {}",
        display
    );
    assert!(display.contains("npm test"), "应保留命令摘要: {}", display);

    let stalled = shell_stalled_notification("t1", "npm publish", None, "continue?");
    let display = shell_notification_display_text_with(&stalled, &lc_en)
        .expect("应识别等待输入通知")
        .text;
    assert!(
        display.contains("appears to be waiting for interactive input"),
        "英文语言下应显示英文等待输入提示: {}",
        display
    );
}

#[test]
fn test_shell_notification_display_text_merged_multiple_completed() {
    let path1 = Path::new("/tmp/peri/tasks/abc1.output");
    let path2 = Path::new("/tmp/peri/tasks/abc2.output");
    let msg1 = shell_completion_notification("abc1", "npm test", Some(0), path1);
    let msg2 = shell_completion_notification("abc2", "cargo check", Some(1), path2);
    let merged = format!("{}\n\n{}", msg1, msg2);

    let display = shell_notification_display_text_with(&merged, &lc_zh())
        .expect("应识别合并的后台 shell 完成通知");
    let text = &display.text;

    assert!(text.contains("npm test"), "应包含第一条任务命令");
    assert!(text.contains("cargo check"), "应包含第二条任务命令");
    assert!(
        text.contains("退出码 1"),
        "应包含第二条任务退出码: {}",
        text
    );
    // 一条成功一条失败：合并后取最严重的语义色（失败=错误色）
    assert_eq!(display.tone, crate::ui::message_view::NoteTone::Error);
    assert!(
        !text.contains("<background-task-completed>"),
        "不应泄露 XML 标签"
    );
    assert_eq!(text.lines().count(), 2, "两条任务合并应对应两行展示文本");
}

#[test]
fn test_shell_notification_display_text_merged_mixed_types_preserves_order() {
    // 合并消息可能混合「完成」与「等待输入」两类块，按文档顺序各产出 1 行。
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let completed = shell_completion_notification("abc", "npm test", Some(0), path);
    let stalled = shell_stalled_notification("def", "npm publish", None, "continue? (y/n)");
    let merged = format!("{}\n\n{}", completed, stalled);

    let display = shell_notification_display_text_with(&merged, &lc_zh())
        .expect("应识别混合类型的合并通知")
        .text;

    let lines: Vec<&str> = display.lines().collect();
    assert_eq!(lines.len(), 2, "两类块各一行: {display}");
    assert!(
        lines[0].contains("npm test"),
        "第一行应是完成通知: {display}"
    );
    assert!(
        lines[1].contains("npm publish") && lines[1].contains("交互输入"),
        "第二行应是等待输入通知: {display}"
    );
    assert!(
        !display.contains("<background-task"),
        "不应泄露内部标签: {display}"
    );
}

#[test]
fn test_agent_completion_notification_includes_description_tag() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    // Act
    let msg = agent_shell_completion_notification(
        "abc",
        "cd /repo && cargo test --all-targets",
        Some("Run the full test suite"),
        &cc_agent::shell::ShellOutcome::Exited(0),
        path,
    );
    // Assert
    assert!(
        msg.contains("<description>Run the full test suite</description>"),
        "应写入模型描述: {}",
        msg
    );
    assert!(
        msg.contains("<command>cd /repo"),
        "命令仍需保留给模型定位任务: {}",
        msg
    );
}

#[test]
fn test_agent_completion_notification_omits_description_tag_when_blank() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    // Act：空串与纯空白都视同未提供
    for description in ["", "   "] {
        let msg = agent_shell_completion_notification(
            "abc",
            "cargo test",
            Some(description),
            &cc_agent::shell::ShellOutcome::Exited(0),
            path,
        );
        // Assert
        assert!(!msg.contains("<description>"), "空描述不应写标签: {}", msg);
    }
}

#[test]
fn test_notification_display_prefers_description_over_command() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = agent_shell_completion_notification(
        "abc",
        "cd /repo && export HTTPS_PROXY=socks5h://127.0.0.1:10808; cargo test",
        Some("Run the test suite behind the proxy"),
        &cc_agent::shell::ShellOutcome::Exited(0),
        path,
    );
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别完成通知")
        .text;
    // Assert
    assert!(
        display.contains("Run the test suite behind the proxy"),
        "应显示模型描述: {}",
        display
    );
    assert!(
        !display.contains("HTTPS_PROXY"),
        "不应再显示命令原文: {}",
        display
    );
    assert!(
        display.contains("（退出码 0）"),
        "应保留退出码: {}",
        display
    );
    assert!(
        !display.contains("(completed ("),
        "不应出现双层括号: {}",
        display
    );
}

#[test]
fn test_notification_display_falls_back_to_command_without_description() {
    // Arrange
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = agent_shell_completion_notification(
        "abc",
        "npm test",
        None,
        &cc_agent::shell::ShellOutcome::Exited(0),
        path,
    );
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别完成通知")
        .text;
    // Assert
    assert!(
        display.contains("npm test"),
        "无描述时应回退命令文本: {}",
        display
    );
}

#[test]
fn test_notification_subject_truncates_by_display_width() {
    // Arrange：40 个 CJK 字符 = 80 显示列，按字符数截断会得到 120 列的成品
    let block = format!("<command>{}</command>", "命令".repeat(40));
    // Act
    let subject = notification_subject(&block);
    // Assert
    let width = unicode_width::UnicodeWidthStr::width(subject.as_str());
    assert!(
        width <= NOTIFICATION_SUBJECT_MAX_WIDTH,
        "标题超宽: {}",
        subject
    );
    assert!(subject.ends_with('…'), "应以 … 收口: {}", subject);
}

#[test]
fn test_status_detail_extracts_inner_bracket() {
    // Arrange + Act + Assert：动词已表达状态，只留括号内的细节
    assert_eq!(status_detail("completed (exit 0)"), Some("exit 0"));
    assert_eq!(status_detail("failed (exit 1)"), Some("exit 1"));
    assert_eq!(
        status_detail("timed out (execution deadline exceeded)"),
        Some("execution deadline exceeded")
    );
    assert_eq!(status_detail("terminated"), None);
    assert_eq!(status_detail("completed ()"), None);
}

#[test]
fn test_notification_description_is_xml_escaped() {
    // Arrange：描述里带会破坏 XML 结构的字符
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let msg = agent_shell_completion_notification(
        "abc",
        "ls <a>",
        Some("run <a> & </description>"),
        &cc_agent::shell::ShellOutcome::Exited(0),
        path,
    );
    // Act
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别完成通知")
        .text;
    // Assert：XML 里被转义，展示层还原原文，且没有被拆出第二个块
    assert!(
        msg.contains("&lt;/description&gt;"),
        "描述中的闭标签必须转义: {}",
        msg
    );
    assert_eq!(
        msg.matches("<description>").count(),
        1,
        "转义后仍只应有一个 description 块: {}",
        msg
    );
    assert!(
        display.contains("run <a> & </description>"),
        "展示层应还原转义后的原文: {}",
        display
    );
}

#[test]
fn test_stalled_notification_includes_description_tag() {
    // Arrange + Act
    let msg = shell_stalled_notification(
        "t1",
        "npm publish",
        Some("Publish the package to npm"),
        "publish? (y/n)",
    );
    // Assert
    assert!(
        msg.contains("<description>Publish the package to npm</description>"),
        "stall 通知也应写入模型描述: {}",
        msg
    );
    let display = shell_notification_display_text_with(&msg, &lc_zh())
        .expect("应识别等待输入通知")
        .text;
    assert!(
        display.contains("Publish the package to npm"),
        "展示层应优先用描述: {}",
        display
    );
    assert!(
        !display.contains("npm publish"),
        "有描述时不应再显示命令原文: {}",
        display
    );
}

/// 与参考实现逐字对齐的**契约测试**。
///
/// 下面这些字符串直接取自参考实现的 `UserAgentNotificationMessage` 所渲染的 summary
/// （`Background command "<description>" completed (exit code 0)` 等）。改动通知文案或
/// 状态色映射时，如果不再与参考实现一致，这个测试必须变红——它是「用户看到一样内容」
/// 的唯一定量保证。
#[test]
fn test_notification_text_matches_reference_implementation() {
    // Arrange
    let lc_en = crate::i18n::LcRegistry::new(Some("en"));
    let path = Path::new("/tmp/peri/tasks/abc.output");
    let description = Some("Run the test suite");
    let cases = [
        (
            cc_agent::shell::ShellOutcome::Exited(0),
            "Background command \"Run the test suite\" completed (exit code 0)",
            NoteTone::Success,
        ),
        (
            cc_agent::shell::ShellOutcome::Exited(1),
            "Background command \"Run the test suite\" failed with exit code 1",
            NoteTone::Error,
        ),
        (
            cc_agent::shell::ShellOutcome::Cancelled,
            "Background command \"Run the test suite\" was stopped",
            NoteTone::Warning,
        ),
        (
            cc_agent::shell::ShellOutcome::TimedOut,
            "Background command \"Run the test suite\" timed out",
            NoteTone::Warning,
        ),
    ];
    for (outcome, expected_text, expected_tone) in cases {
        // Act
        let msg =
            agent_shell_completion_notification("abc", "cargo test", description, &outcome, path);
        let display = shell_notification_display_text_with(&msg, &lc_en).expect("应识别完成通知");
        // Assert
        assert_eq!(display.text, expected_text, "文案必须与参考实现逐字一致");
        assert_eq!(display.tone, expected_tone, "状态色映射必须与参考实现一致");
    }

    // 等待输入：参考实现同样是 `Background command "<desc>" appears to be waiting ...`
    let stalled = shell_stalled_notification("def", "npm publish", description, "publish? (y/n)");
    let display =
        shell_notification_display_text_with(&stalled, &lc_en).expect("应识别等待输入通知");
    assert_eq!(
        display.text,
        "Background command \"Run the test suite\" appears to be waiting for interactive input",
        "等待输入文案必须与参考实现逐字一致"
    );
    assert_eq!(display.tone, NoteTone::Warning);
}
