//! Cross-platform shell command spawning.
//!
//! On Unix, wraps commands in `bash -c "<command> <args...>"`.
//! On Windows, wraps commands in `cmd /C <command> <args...>`. When `cmd` fails
//! with the classic "is not recognized as an internal or external command" error
//! (e.g. the Agent tried to run `grep`/`ls`/`find`), callers can fall back to
//! Git Bash via [`git_bash_path`] + [`git_bash_command`].

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Build a `tokio::process::Command` that executes the given command through the
/// platform shell.
///
/// - **Unix**: `bash -c "<command> <args...>"`
/// - **Windows**: `cmd /C <command> <args...>"`
///
/// Returns the `Command` object so callers can add custom configuration
/// (env, current_dir, stdin/stdout/stderr, kill_on_drop, etc.).
pub fn shell_command(command: &str, args: &[&str]) -> tokio::process::Command {
    if cfg!(target_os = "windows") {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        for arg in args {
            cmd.arg(arg);
        }
        cmd
    } else {
        let mut parts = vec![command.to_string()];
        for arg in args {
            if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.contains('\\') {
                parts.push(format!("'{}'", arg.replace('\'', "'\\''")));
            } else {
                parts.push(arg.to_string());
            }
        }
        let shell_cmd = parts.join(" ");
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(&shell_cmd);
        cmd
    }
}

/// 检测 Git Bash 可执行文件路径。仅 Windows 上有实际意义，其他平台直接返回 None。
///
/// 检测顺序：
/// 1. 常见安装路径（`C:\Program Files\Git\bin\bash.exe` 等）
/// 2. `where bash` 输出的第一行
///
/// 结果用 `OnceLock` 缓存，整个进程只检测一次。
pub fn git_bash_path() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(detect_git_bash_path).clone()
}

fn detect_git_bash_path() -> Option<PathBuf> {
    let candidates: &[&str] = &[
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];
    for path in candidates {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    let output = std::process::Command::new("where")
        .arg("bash")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next()?;
    let trimmed = first.trim();
    if trimmed.is_empty() {
        return None;
    }
    let p = Path::new(trimmed);
    if p.exists() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

/// 判断 stderr 是否包含 Windows `cmd /C` 在命令找不到时输出的特征字符串。
///
/// 典型形态（Windows GBK/UTF-8 控制台均会输出英文版）：
/// ```text
/// 'grep' is not recognized as an internal or external command,
/// operable program or batch file.
/// ```
pub fn is_unrecognized_command_error(stderr: &str) -> bool {
    stderr.contains("is not recognized as an internal or external command")
}

/// 用显式指定的 bash 可执行文件构造 `bash -c "<command> <args...>"`。
///
/// 与 [`shell_command`] 的 Unix 分支语义一致，但允许调用方指定 Git Bash 路径，
/// 用于 Windows fallback 场景。
pub fn git_bash_command(bash_exe: &Path, command: &str, args: &[&str]) -> tokio::process::Command {
    let mut parts = vec![command.to_string()];
    for arg in args {
        if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.contains('\\') {
            parts.push(format!("'{}'", arg.replace('\'', "'\\''")));
        } else {
            parts.push(arg.to_string());
        }
    }
    let shell_cmd = parts.join(" ");
    let mut cmd = tokio::process::Command::new(bash_exe);
    cmd.arg("-c").arg(&shell_cmd);
    cmd
}

#[cfg(test)]
mod process_test;
