//! 阻塞 sleep 检测与拦截的单元测试。
//!
//! 判定边界对齐 CC `detectBlockedSleepPattern`（BashTool.tsx:461）：
//! 只看首个子命令、只认整数秒、N ≥ 2 才拦。

use super::*;

#[test]
fn detects_standalone_sleep() {
    assert_eq!(
        detect_blocked_sleep_pattern("sleep 45").as_deref(),
        Some("standalone sleep 45")
    );
}

#[test]
fn detects_sleep_followed_by_command() {
    let pattern = detect_blocked_sleep_pattern(r#"sleep 45; cat "out.txt" | head -100"#)
        .expect("应命中间隔轮询");
    assert!(
        pattern.starts_with("sleep 45 followed by:"),
        "实际: {pattern}"
    );
    assert!(pattern.contains("cat"), "应回显后续命令，实际: {pattern}");
}

#[test]
fn detects_sleep_with_and_operator() {
    let pattern = detect_blocked_sleep_pattern("sleep 5 && curl -s localhost/health")
        .expect("&& 拼接也应命中");
    assert!(pattern.contains("curl"), "实际: {pattern}");
}

#[test]
fn allows_sub_two_second_sleep() {
    // 1 秒属于限流/节流，是合法用法。
    assert_eq!(detect_blocked_sleep_pattern("sleep 1"), None);
}

#[test]
fn allows_float_seconds() {
    // 浮点秒数是节流，不是轮询。
    assert_eq!(detect_blocked_sleep_pattern("sleep 0.5"), None);
    assert_eq!(detect_blocked_sleep_pattern("sleep 2.5"), None);
}

#[test]
fn allows_sleep_not_in_first_position() {
    // sleep 在管道第二段：shell 未被纯等待占住。
    assert_eq!(detect_blocked_sleep_pattern("echo hi | sleep 10"), None);
    // sleep 在 && 之后：首个子命令不是 sleep。
    assert_eq!(detect_blocked_sleep_pattern("make build && sleep 10"), None);
}

#[test]
fn allows_sleep_inside_quoted_string() {
    // 引号内的 `;` 不是命令分隔符。
    assert_eq!(
        detect_blocked_sleep_pattern(r#"echo "sleep 30; echo hi""#),
        None
    );
}

#[test]
fn allows_until_loop_polling() {
    // until-loop 是推荐写法，首个子命令是 until 而非 sleep。
    assert_eq!(
        detect_blocked_sleep_pattern("until curl -sf localhost/health; do sleep 2; done"),
        None
    );
}

#[test]
fn does_not_match_extra_sleep_args() {
    // `sleep 30s` 不是纯整数秒，交给 shell 自己报错。
    assert_eq!(detect_blocked_sleep_pattern("sleep 30s"), None);
}

#[test]
fn blocked_message_points_to_alternatives() {
    let message = blocked_sleep_message("standalone sleep 45");
    assert!(message.starts_with("Blocked: standalone sleep 45."), "{message}");
    // 三条替代路径都要出现，否则模型不知道往哪走。
    assert!(message.contains("until-loop"), "{message}");
    assert!(message.contains("run_in_background"), "{message}");
    assert!(message.contains("Sleep tool"), "{message}");
    // 明确堵住拆短 sleep 绕过的做法。
    assert!(message.contains("Do not chain shorter sleeps"), "{message}");
}

#[tokio::test]
async fn invoke_rejects_blocking_sleep_in_foreground() {
    let tool = BashTool::new(std::env::temp_dir().to_string_lossy());
    let error = tool
        .invoke(serde_json::json!({"command": "sleep 45"}))
        .await
        .expect_err("前台阻塞 sleep 应被拒绝");
    assert!(
        error.to_string().starts_with("Blocked: standalone sleep 45"),
        "实际: {error}"
    );
}

#[tokio::test]
async fn invoke_allows_blocking_sleep_when_backgrounded() {
    // 显式 run_in_background 放行：交由 InlineShellExecutor 决定是否支持后台，
    // 这里断言的是「不被 sleep 拦截挡下」。
    let tool = BashTool::new(std::env::temp_dir().to_string_lossy());
    let result = tool
        .invoke(serde_json::json!({"command": "sleep 45", "run_in_background": true}))
        .await;
    let error = result.expect_err("InlineShellExecutor 不支持后台，应报后台不支持");
    assert!(
        error.to_string().contains("does not support background"),
        "实际: {error}"
    );
}
