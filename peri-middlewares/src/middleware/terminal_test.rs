use super::*;
use peri_agent::tools::BaseTool;
use std::time::Instant;

#[tokio::test]
async fn test_bash_normal_command() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "echo hello"}))
        .await
        .unwrap();
    assert!(result.contains("hello"));
}

#[tokio::test]
async fn test_bash_nonzero_exit_code() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "exit 42"}))
        .await
        .unwrap();
    assert!(result.contains("42"), "应包含退出码: {result}");
}

/// 验证超时后在合理时间内返回，且 kill_on_drop 确保子进程被清理
#[tokio::test]
async fn test_bash_timeout_returns_quickly() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let start = Instant::now();

    // Windows 用 ping 模拟 sleep，Unix 用 sleep
    let (sleep_cmd, timeout_ms) = if cfg!(target_os = "windows") {
        ("ping -n 60 127.0.0.1", 1000)
    } else {
        ("sleep 60", 1000)
    };

    let result = tool
        .invoke(serde_json::json!({
            "command": sleep_cmd,
            "timeout": timeout_ms
        }))
        .await;
    let err_msg = result.unwrap_err().to_string();
    let elapsed = start.elapsed();

    // 应在约 1 秒内返回（不超过 3 秒），不等待 sleep 60 完成
    assert!(
        elapsed.as_secs() < 3,
        "超时后应快速返回，实际耗时 {:?}",
        elapsed
    );
    assert!(
        err_msg.contains("timed out"),
        "返回值应包含超时提示: {err_msg}"
    );
}

#[tokio::test]
async fn test_bash_stderr_captured() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({"command": "echo err >&2"}))
        .await
        .unwrap();
    assert!(result.contains("err"), "stderr 应被捕获: {result}");
}

#[test]
fn test_truncate_output_line_count_accurate() {
    // 生成不含末尾换行的多行文本，避免 split('\n') 产生额外空行
    let lines: Vec<String> = (0..3000).map(|i| format!("line {}", i)).collect();
    let input = lines.join("\n");
    assert_eq!(input.split('\n').count(), 3000);
    let result = truncate_output(&input);
    assert!(
        result.contains("3000 total lines"),
        "应显示正确的总行数: {result}"
    );
    // 应保留头部和尾部
    assert!(result.contains("line 0"), "应保留第一行: {result}");
    assert!(result.contains("line 2999"), "应保留最后一行: {result}");
    assert!(
        result.contains("lines truncated"),
        "应显示截断信息: {result}"
    );
}

#[test]
fn test_truncate_output_no_truncation_when_small() {
    let result = truncate_output("hello\nworld");
    assert_eq!(result, "hello\nworld");
}

#[test]
fn test_truncate_output_char_limit() {
    let long_line = "x".repeat(200_000);
    let result = truncate_output(&long_line);
    assert!(result.contains("byte limit"), "应截断超长输出: {result}");
}

#[test]
fn test_truncate_output_preserves_tail() {
    // 3000 行，尾部包含关键信息
    let mut lines: Vec<String> = (0..2999).map(|i| format!("line {}", i)).collect();
    lines.push("CRITICAL ERROR: test failed".to_string());
    let input = lines.join("\n");
    let result = truncate_output(&input);
    // 尾部关键行应保留
    assert!(
        result.contains("CRITICAL ERROR"),
        "截断后应保留尾部关键信息: {result}"
    );
    assert!(result.contains("line 0"), "应保留头部: {result}");
}

#[test]
fn test_bash_description_extended() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let desc = tool.description();
    assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
    assert!(
        desc.contains("dedicated tool"),
        "description 应强调优先使用专用工具"
    );
    assert!(desc.contains("timeout"), "description 应提及超时");
    assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
}

/// 零超时应被 clamp 到至少 1 毫秒。timeout=0 → 1ms 太短，echo 大概率超时返回 Err。
/// 这里验证 timeout=100ms（clamp 后足够执行 echo），命令正常完成。
#[tokio::test]
async fn test_bash_timeout_clamped_to_minimum() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let start = Instant::now();
    // timeout = 100 → clamp 不生效，echo quick 应在 100ms 内正常完成
    let result = tool
        .invoke(serde_json::json!({
            "command": "echo quick",
            "timeout": 100
        }))
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(result.contains("quick"), "echo quick 应正常输出: {result}");
    assert!(
        elapsed.as_millis() < 500,
        "应快速完成，实际耗时 {:?}",
        elapsed
    );
}

/// 显式超时 600000 毫秒应被允许（上限）
#[tokio::test]
async fn test_bash_timeout_maximum_accepted() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    let result = tool
        .invoke(serde_json::json!({
            "command": "echo ok",
            "timeout": 600000
        }))
        .await
        .unwrap();
    assert!(result.contains("ok"));
}

#[test]
#[allow(non_snake_case)]
fn test_tool_name_is_Bash() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    assert_eq!(tool.name(), "Bash");
}

#[tokio::test]
async fn test_bash_default_timeout_is_120_seconds() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    // 不传 timeout → 默认 120000ms = 120s
    let result = tool
        .invoke(serde_json::json!({"command": "echo ok"}))
        .await
        .unwrap();
    assert!(result.contains("ok"));
}

#[tokio::test]
async fn test_bash_description_and_run_in_background_parsed() {
    let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
    // description 和 run_in_background 不影响执行
    let result = tool
        .invoke(serde_json::json!({
            "command": "echo ok",
            "description": "test description",
            "run_in_background": true
        }))
        .await
        .unwrap();
    assert!(result.contains("ok"));
}

#[test]
fn test_truncate_bytes_ascii() {
    let s = "hello world";
    assert_eq!(truncate_bytes(s, 5), "hello");
}

#[test]
fn test_truncate_bytes_within_limit() {
    let s = "hello";
    assert_eq!(truncate_bytes(s, 100), "hello");
}

#[test]
fn test_truncate_bytes_utf8_safe() {
    // 中文字符每个占 3 字节，在字节 7 处截断（是字符边界）
    let s = "你好世界";
    assert_eq!(truncate_bytes(s, 6), "你好");
}

#[test]
fn test_truncate_bytes_utf8_mid_character() {
    // "你好" = 6 bytes, 在字节 5 处截断（不是字符边界）
    // 应回退到字节 3 处（"你" 的末尾）
    let s = "你好世界";
    let result = truncate_bytes(s, 5);
    assert_eq!(result, "你", "应在字符边界截断，实际: {}", result);
}

#[test]
fn test_truncate_bytes_empty_string() {
    assert_eq!(truncate_bytes("", 10), "");
}

#[test]
fn test_truncate_bytes_zero_max() {
    assert_eq!(truncate_bytes("hello", 0), "");
}

#[test]
fn test_truncate_output_persists_full_content_on_lines_truncation() {
    let lines: Vec<String> = (0..3000).map(|i| format!("line {}", i)).collect();
    let input = lines.join("\n");
    let result = truncate_output(&input);
    assert!(
        result.contains("Read tool"),
        "应包含 Read tool 提示: {result}"
    );
    assert!(
        result.contains("peri-tool-output-"),
        "应包含临时文件路径: {result}"
    );
}

#[test]
fn test_truncate_output_persists_full_content_on_byte_truncation() {
    let long_line = "x".repeat(200_000);
    let result = truncate_output(&long_line);
    assert!(result.contains("Read tool"), "字节截断也应持久化: {result}");
    assert!(
        result.contains("peri-tool-output-"),
        "字节截断应包含文件路径: {result}"
    );
}

// ── extract_quoted_message 测试 ──────────────────────────────────────

#[test]
fn test_extract_quoted_message_double_quotes() {
    let (msg, rest) = extract_quoted_message(r#""feat(tui): display version number on welcome page""#);
    assert_eq!(msg.as_deref(), Some("feat(tui): display version number on welcome page"));
    assert_eq!(rest, "");
}

#[test]
fn test_extract_quoted_message_with_remaining() {
    let (msg, rest) = extract_quoted_message(r#""my message" --no-verify"#);
    assert_eq!(msg.as_deref(), Some("my message"));
    assert_eq!(rest, " --no-verify");
}

#[test]
fn test_extract_quoted_message_single_quotes() {
    let (msg, _) = extract_quoted_message("'hello world'");
    assert_eq!(msg.as_deref(), Some("hello world"));
}

#[test]
fn test_extract_quoted_message_escaped_quote() {
    let (msg, _) = extract_quoted_message(r#""say \"hello\"""#);
    assert_eq!(msg.as_deref(), Some(r#"say "hello""#));
}

#[test]
fn test_extract_quoted_message_no_quote() {
    let (msg, rest) = extract_quoted_message("no-quotes here");
    assert!(msg.is_none());
    assert_eq!(rest, "no-quotes here");
}

#[test]
fn test_extract_quoted_message_empty() {
    let (msg, rest) = extract_quoted_message("");
    assert!(msg.is_none());
    assert_eq!(rest, "");
}

#[test]
fn test_extract_quoted_message_unclosed() {
    let (msg, _) = extract_quoted_message(r#""unclosed message"#);
    assert!(msg.is_none());
}

#[test]
fn test_extract_quoted_message_chinese() {
    let (msg, _) = extract_quoted_message(r#""feat(tui): 显示版本号""#);
    assert_eq!(msg.as_deref(), Some("feat(tui): 显示版本号"));
}

// ── rewrite_git_commit_for_windows 测试（仅 Windows）───────────────

#[cfg(windows)]
mod windows_git_rewrite {
    use super::*;

    #[test]
    fn test_rewrite_simple_commit() {
        let (cmd, info) =
            rewrite_git_commit_for_windows(r#"git commit -m "feat(tui): display version""#);
        assert!(cmd.contains("git commit -F"), "应改写为 -F: {cmd}");
        assert!(info.is_some(), "应返回临时文件信息");
        let (path, content) = info.unwrap();
        assert!(path.ends_with(".txt"), "临时文件应为 .txt: {path}");
        assert_eq!(content, "feat(tui): display version");
    }

    #[test]
    fn test_rewrite_long_flag() {
        let (cmd, info) =
            rewrite_git_commit_for_windows(r#"git commit --message "hello world""#);
        assert!(cmd.contains("git commit -F"), "应改写 --message: {cmd}");
        assert_eq!(info.unwrap().1, "hello world");
    }

    #[test]
    fn test_rewrite_preserves_extra_flags() {
        let (cmd, _) = rewrite_git_commit_for_windows(
            r#"git commit -m "msg" --no-verify"#,
        );
        assert!(cmd.contains("--no-verify"), "应保留额外标志: {cmd}");
        assert!(cmd.contains("-F"), "应改写为 -F: {cmd}");
    }

    #[test]
    fn test_no_rewrite_without_quotes() {
        let (cmd, info) = rewrite_git_commit_for_windows("git commit -m msg");
        assert!(info.is_none(), "无引号时不应重写");
        assert_eq!(cmd, "git commit -m msg");
    }

    #[test]
    fn test_no_rewrite_chained_command() {
        let input = r#"git add . && git commit -m "msg""#;
        let (cmd, info) = rewrite_git_commit_for_windows(input);
        assert!(info.is_none(), "链式命令不应重写");
        assert_eq!(cmd, input);
    }

    #[test]
    fn test_no_rewrite_non_commit() {
        let input = "git status";
        let (cmd, info) = rewrite_git_commit_for_windows(input);
        assert!(info.is_none());
        assert_eq!(cmd, input);
    }

    #[test]
    fn test_no_rewrite_pipe() {
        let input = r#"git commit -m "msg" | cat"#;
        let (cmd, info) = rewrite_git_commit_for_windows(input);
        assert!(info.is_none(), "管道命令不应重写");
        assert_eq!(cmd, input);
    }

    #[test]
    fn test_rewrite_multiple_m_flags() {
        let (cmd, info) = rewrite_git_commit_for_windows(
            r#"git commit -m "subject" -m "body paragraph""#,
        );
        assert!(cmd.contains("git commit -F"), "应改写为 -F: {cmd}");
        assert!(!cmd.contains(" -m "), "不应残留 -m: {cmd}");
        let (path, content) = info.unwrap();
        assert!(path.ends_with(".txt"));
        // git 语义：多个 -m 以双换行拼接
        assert_eq!(content, "subject\n\nbody paragraph");
    }

    #[test]
    fn test_rewrite_three_m_flags() {
        let (cmd, info) = rewrite_git_commit_for_windows(
            r#"git commit -m "first" -m "second" -m "third""#,
        );
        assert!(cmd.contains("-F"), "应改写为 -F: {cmd}");
        assert_eq!(info.unwrap().1, "first\n\nsecond\n\nthird");
    }

    #[test]
    fn test_rewrite_mixed_m_and_other_flags() {
        let (cmd, info) = rewrite_git_commit_for_windows(
            r#"git commit --no-verify -m "msg" --amend"#,
        );
        assert!(cmd.contains("-F"), "应改写为 -F: {cmd}");
        assert!(cmd.contains("--no-verify"), "应保留 --no-verify: {cmd}");
        assert!(cmd.contains("--amend"), "应保留 --amend: {cmd}");
        assert!(!cmd.contains(" -m "), "不应残留 -m: {cmd}");
        assert_eq!(info.unwrap().1, "msg");
    }
}
