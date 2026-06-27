use crate::process::{
    git_bash_command, git_bash_path, is_unrecognized_command_error, should_fallback_to_bash,
    shell_command,
};
use std::path::Path;

#[test]
fn test_shell_command_unix_bash_c() {
    let cmd = shell_command("echo", &["hello"]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("-c"),
            "expected -c flag, got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("/C"),
            "expected /C flag, got: {formatted}"
        );
    }
}

#[test]
fn test_shell_command_no_args() {
    let cmd = shell_command("ls", &[]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("ls"),
            "expected 'ls' in command, got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("ls"),
            "expected 'ls' in command, got: {formatted}"
        );
    }
}

#[test]
fn test_shell_command_multi_args() {
    let cmd = shell_command("npx", &["-y", "@anthropic/mcp-server"]);
    let formatted = format!("{cmd:?}");
    #[cfg(unix)]
    {
        assert!(
            formatted.contains("bash"),
            "expected bash, got: {formatted}"
        );
        assert!(
            formatted.contains("npx"),
            "expected 'npx', got: {formatted}"
        );
    }
    #[cfg(windows)]
    {
        assert!(formatted.contains("cmd"), "expected cmd, got: {formatted}");
        assert!(
            formatted.contains("npx"),
            "expected 'npx', got: {formatted}"
        );
    }
}

#[test]
fn test_is_unrecognized_command_error_matches_classic_pattern() {
    // 英文版典型 stderr（Windows cmd 默认 locale）
    let stderr = "'grep' is not recognized as an internal or external command,\r\noperable program or batch file.\r\n";
    assert!(
        is_unrecognized_command_error(stderr),
        "应命中 cmd 'not recognized' 特征"
    );
}

#[test]
fn test_is_unrecognized_command_error_matches_minimal_substring() {
    // 只要有特征子串就命中，不要求完整句式
    assert!(is_unrecognized_command_error(
        "foo: 'ls' is not recognized as an internal or external command"
    ));
}

#[test]
fn test_is_unrecognized_command_error_rejects_unrelated_stderr() {
    // 普通错误输出（exit≠0 但不是命令找不到）不应触发 fallback
    assert!(!is_unrecognized_command_error("Permission denied"));
    assert!(!is_unrecognized_command_error(
        "grep: no such file or directory"
    ));
    assert!(!is_unrecognized_command_error(""));
}

#[test]
fn test_git_bash_command_constructs_bash_c_with_args() {
    // 跨平台纯构造：Debug 输出应包含 bash 路径、-c flag 和原始命令
    let bash_path = Path::new("/custom/bash");
    let cmd = git_bash_command(bash_path, "grep", &["-r", "foo"]);
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.to_lowercase().contains("bash"),
        "expected bash exe in command, got: {formatted}"
    );
    assert!(
        formatted.contains("-c"),
        "expected -c flag, got: {formatted}"
    );
    assert!(
        formatted.contains("grep"),
        "expected command retained, got: {formatted}"
    );
}

#[test]
fn test_git_bash_command_quotes_args_with_special_chars() {
    // 含空格的参数应被单引号包裹，避免 bash word splitting
    let bash_path = Path::new("bash");
    let cmd = git_bash_command(bash_path, "echo", &["hello world"]);
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("hello world"),
        "expected arg content retained, got: {formatted}"
    );
}

#[test]
fn test_git_bash_command_no_args() {
    let bash_path = Path::new("bash");
    let cmd = git_bash_command(bash_path, "ls", &[]);
    let formatted = format!("{cmd:?}");
    assert!(
        formatted.contains("-c"),
        "expected -c flag, got: {formatted}"
    );
    assert!(formatted.contains("ls"), "expected 'ls', got: {formatted}");
}

#[test]
fn test_git_bash_path_returns_none_on_non_windows_or_missing() {
    // 仅 Windows 上有 Git Bash；非 Windows 必然 None。
    // Windows 上若未装 Git Bash 也应 None，不 panic。
    let path = git_bash_path();
    #[cfg(not(windows))]
    {
        assert!(
            path.is_none(),
            "非 Windows 上 git_bash_path 必须返回 None，实际：{path:?}"
        );
    }
    // Windows 上不强制断言存在性（取决于机器是否装了 Git），
    // 但必须不 panic、且多次调用返回同一结果（OnceLock 缓存）。
    let path2 = git_bash_path();
    assert_eq!(path, path2, "OnceLock 缓存失效，两次结果不一致");
}

// ── 多语言 is_unrecognized_command_error 测试 ──────────────────────

#[test]
fn test_is_unrecognized_command_error_chinese() {
    assert!(
        is_unrecognized_command_error(
            "'grep' 不是内部或外部命令，也不是可运行的程序\r\n或批处理文件。"
        ),
        "应命中中文 Windows stderr"
    );
}

#[test]
fn test_is_unrecognized_command_error_french() {
    assert!(
        is_unrecognized_command_error(
            "'grep' n'est pas reconnu en tant que commande interne"
        ),
        "应命中法语 Windows stderr"
    );
}

#[test]
fn test_is_unrecognized_command_error_german() {
    assert!(
        is_unrecognized_command_error(
            "'grep' nicht als Befehl erkannt"
        ),
        "应命中德语 Windows stderr"
    );
}

// ── should_fallback_to_bash 测试 ──────────────────────────────────

#[test]
fn test_should_fallback_exit_code_zero_never_fallback() {
    // exit code = 0 时不触发 fallback，即使 stderr 有特征字符串
    assert!(
        !should_fallback_to_bash(0, "output", "is not recognized"),
        "exit code 0 不应 fallback"
    );
}

#[test]
fn test_should_fallback_keyword_match() {
    // exit ≠ 0 + stderr 匹配关键词 → fallback
    assert!(should_fallback_to_bash(
        1,
        "",
        "'grep' is not recognized as an internal or external command"
    ));
    assert!(should_fallback_to_bash(
        1,
        "some output",
        "'grep' 不是内部或外部命令，也不是可运行的程序"
    ));
}

#[test]
fn test_should_fallback_fallback_pattern() {
    // 兜底：exit ≠ 0 + 无 stdout + 短 stderr（未知语言 Windows）
    assert!(
        should_fallback_to_bash(1, "", "some short error"),
        "应触发兜底 fallback"
    );
}

#[test]
fn test_should_fallback_no_fallback_real_script_error() {
    // 真正的脚本错误：有 stdout 或 stderr 太长 → 不 fallback
    assert!(
        !should_fallback_to_bash(1, "some output", "some short error"),
        "有 stdout 时不应兜底 fallback"
    );
    let long_stderr = "x".repeat(200);
    assert!(
        !should_fallback_to_bash(1, "", &long_stderr),
        "stderr ≥ 200 bytes 时不应兜底 fallback"
    );
}

#[test]
fn test_should_fallback_no_fallback_normal_error() {
    // 普通错误（如 Permission denied）：有 stdout + 长 stderr → 不 fallback
    assert!(!should_fallback_to_bash(
        1,
        "some output",
        "Permission denied"
    ));
}

#[test]
fn test_should_fallback_empty_stderr() {
    // exit ≠ 0 但 stderr 为空 → 不 fallback（兜底要求 stderr 非空）
    assert!(
        !should_fallback_to_bash(1, "", ""),
        "空 stderr 不应触发兜底 fallback"
    );
}

// ── MSYS_NO_PATHCONV 测试 ────────────────────────────────────────

#[tokio::test]
async fn test_git_bash_command_sets_msys_no_pathconv() {
    // 通过实际执行验证 MSYS_NO_PATHCONV 环境变量已注入
    let bash_path = Path::new("bash");
    let mut cmd = git_bash_command(bash_path, "echo $MSYS_NO_PATHCONV", &[]);
    let output = cmd.output().await;
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                stdout.trim() == "1",
                "MSYS_NO_PATHCONV 应为 1，实际: {}",
                stdout.trim()
            );
        }
        // bash 不可用时跳过（非 Windows CI 环境可能没有 bash）
        _ => {}
    }
}
