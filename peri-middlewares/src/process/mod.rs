//! Cross-platform shell command spawning.
//!
//! On Unix, wraps commands in `bash -c "<command> <args...>"`.
//! On Windows, wraps commands in `cmd /C <command> <args...>`. When `cmd` fails
//! with the classic "is not recognized as an internal or external command" error
//! (e.g. the Agent tried to run `grep`/`ls`/`find`), callers can fall back to
//! Git Bash via [`git_bash_path`] + [`git_bash_command`]. Use
//! [`should_fallback_to_bash`] for multi-language matching and fallback logic.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 检测命令字符串是否包含 cmd.exe 特殊字符（`& | < > ^`）。
///
/// 这些字符在 `cmd /C` 中会被解析为命令分隔符/管道/重定向，
/// 需要用 `cmd /S /C "..."` 包裹以防止语法错误。
fn has_cmd_special_chars(command: &str) -> bool {
    command.contains('&')
        || command.contains('|')
        || command.contains('<')
        || command.contains('>')
        || command.contains('^')
}

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
        // cmd.exe 把 & | < > ^ 等字符解析为命令分隔符/管道/重定向。
        // 用 /S /C "..." 包裹可防止特殊字符被错误解析（/S 剥离外层引号）。
        if has_cmd_special_chars(command) {
            cmd.arg("/S").arg("/C").arg(format!("\"{}\"", command));
        } else {
            cmd.arg("/C").arg(command);
        }
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
    // 环境变量优先级最高：用户显式指定
    if let Ok(env_path) = std::env::var("GIT_BASH_PATH") {
        let p = PathBuf::from(&env_path);
        if p.exists() && verify_bash_executable(&p) {
            return Some(p);
        }
    }

    let candidates: &[&str] = &[
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];
    for path in candidates {
        let p = Path::new(path);
        if p.exists() && verify_bash_executable(p) {
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
    if p.exists() && verify_bash_executable(p) {
        Some(p.to_path_buf())
    } else {
        None
    }
}

/// 验证 bash 可执行文件是否能正常运行（`--version` 检查）。
fn verify_bash_executable(path: &Path) -> bool {
    std::process::Command::new(path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 判断 stderr 是否包含 Windows `cmd /C` 在命令找不到时输出的特征字符串。
///
/// 支持多语言 Windows：
/// - English: `is not recognized as an internal or external command`
/// - 中文: `不是内部或外部命令`
/// - 法语: `n'est pas reconnu`
/// - 德语: `nicht als Befehl erkannt`
pub fn is_unrecognized_command_error(stderr: &str) -> bool {
    stderr.contains("is not recognized as an internal or external command")
        || stderr.contains("不是内部或外部命令")
        || stderr.contains("n'est pas reconnu")
        || stderr.contains("nicht als Befehl erkannt")
}

/// 综合判断是否应 fallback 到 Git Bash。
///
/// 两种触发条件（满足任一即可）：
/// 1. stderr 匹配任一语言的"命令未识别"关键词
/// 2. 兜底：exit_code ≠ 0 且 stdout 为空且 stderr 长度 < 200 bytes
///    （排除真正的脚本错误——那些通常有较长的 stderr 输出）
pub fn should_fallback_to_bash(exit_code: i32, stdout: &str, stderr: &str) -> bool {
    if exit_code == 0 {
        return false;
    }
    if is_unrecognized_command_error(stderr) {
        return true;
    }
    // 兜底：短 stderr + 无 stdout → 大概率是命令找不到（未知语言 Windows）
    stdout.is_empty() && !stderr.is_empty() && stderr.len() < 200
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
    // 禁用 MSYS2/MinGW 的自动路径转换，防止 /pattern 等参数被转为 Windows 路径
    cmd.env("MSYS_NO_PATHCONV", "1");
    cmd
}

#[cfg(test)]
mod process_test;
