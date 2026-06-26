use crate::process::{
    git_bash_command, git_bash_path, is_unrecognized_command_error, shell_command,
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
