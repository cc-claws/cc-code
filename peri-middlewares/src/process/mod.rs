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

/// 后台/管道 shell 不能共享 TUI 控制台，否则 PHP 等程序会修改其代码页或模式。
/// stdin/stdout/stderr 仍由调用方配置；不用于需要真实终端的交互式 PTY。
fn background_shell(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
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
    shell_command_with_shell(command, args, None)
}

/// Build a `tokio::process::Command` that executes the given command through the
/// specified shell or platform default.
///
/// - **shell = Some("powershell") / Some("pwsh")**: `powershell -Command "<command> <args...>"`
/// - **shell = Some("bash")**: Git Bash fallback on Windows, `bash -c` on Unix
/// - **shell = None**: Platform default (`cmd /C` on Windows, `bash -c` on Unix)
///
/// Returns the `Command` object so callers can add custom configuration.
pub fn shell_command_with_shell(
    command: &str,
    args: &[&str],
    shell: Option<&str>,
) -> tokio::process::Command {
    let shell_lower = shell.map(|s| s.to_lowercase());

    match shell_lower.as_deref() {
        Some("powershell" | "pwsh") => {
            // PowerShell: powershell -Command "..."
            let mut cmd = background_shell("powershell");
            cmd.arg("-NoProfile").arg("-NonInteractive").arg("-Command");
            // PowerShell -Command 需要整个命令作为单个参数
            let full_command = if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            cmd.arg(full_command);
            cmd
        }
        Some("bash") => {
            // Explicit bash: use Git Bash on Windows, bash on Unix
            if cfg!(target_os = "windows") {
                if let Some(bash_exe) = git_bash_path() {
                    git_bash_command(&bash_exe, command, args)
                } else {
                    // Fallback to cmd if no bash available
                    tracing::warn!(
                        "bash shell requested but Git Bash not found, falling back to cmd"
                    );
                    shell_command_cmd(command, args)
                }
            } else {
                // Unix: direct bash
                let mut parts = vec![command.to_string()];
                for arg in args {
                    if arg.contains(' ')
                        || arg.contains('"')
                        || arg.contains('\'')
                        || arg.contains('\\')
                    {
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
        _ => {
            // Default: platform shell (cmd on Windows, bash on Unix)
            if cfg!(target_os = "windows") {
                // cmd /C 无法正确处理含字面换行符的多行命令——只执行第一行，
                // 后续行的 stdout 全部丢失（Issue #212）。检测到换行时直接走
                // Git Bash，bash -c 能正确处理多行命令。
                if command.contains('\n') {
                    if let Some(bash_exe) = git_bash_path() {
                        return git_bash_command(&bash_exe, command, args);
                    }
                    // Git Bash 不可用时回退 cmd（行为不变，至少不会 panic）
                }
                shell_command_cmd(command, args)
            } else {
                let mut parts = vec![command.to_string()];
                for arg in args {
                    if arg.contains(' ')
                        || arg.contains('"')
                        || arg.contains('\'')
                        || arg.contains('\\')
                    {
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
    }
}

/// Helper: build a `cmd /C` command on Windows
fn shell_command_cmd(command: &str, args: &[&str]) -> tokio::process::Command {
    let mut cmd = background_shell("cmd");
    cmd.arg("/C");
    push_cmd_raw_command(&mut cmd, command);
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}

#[cfg(windows)]
fn push_cmd_raw_command(cmd: &mut tokio::process::Command, command: &str) {
    // `cmd /C` expects the rest of the process command line to be the command
    // text. Passing it as a normal argument makes Rust quote the whole string,
    // so commands like `python "D:/script.py"` leak the quote into argv.
    cmd.raw_arg(command);
}

#[cfg(not(windows))]
fn push_cmd_raw_command(cmd: &mut tokio::process::Command, command: &str) {
    cmd.arg(command);
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
    let mut cmd = background_shell(bash_exe);
    cmd.arg("-c").arg(&shell_cmd);
    // 禁用 MSYS2/MinGW 的自动路径转换，防止 /pattern 等参数被转为 Windows 路径
    cmd.env("MSYS_NO_PATHCONV", "1");
    cmd
}

/// 检测 RTK (Rust Token Killer) 可执行文件路径。
///
/// 检测顺序：
/// 1. 环境变量 `RTK_PATH`
/// 2. `where rtk` (Windows) 或 `which rtk` (Unix)
///
/// 结果用 `OnceLock` 缓存，整个进程只检测一次。
pub fn rtk_path() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(detect_rtk_path).clone()
}

fn detect_rtk_path() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("RTK_PATH") {
        let p = PathBuf::from(&env_path);
        if p.exists() && verify_rtk_executable(&p) {
            return Some(p);
        }
    }

    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    let output = std::process::Command::new(which_cmd)
        .arg("rtk")
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
    if p.exists() && verify_rtk_executable(p) {
        Some(p.to_path_buf())
    } else {
        None
    }
}

fn verify_rtk_executable(path: &Path) -> bool {
    std::process::Command::new(path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 快速判断命令是否属于 RTK 可能支持的工具，避免对 echo/cd/rm 等命令产生额外的进程探测开销。
pub fn is_potential_rtk_command(command: &str) -> bool {
    let trimmed = command.trim_start();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    // 跳过环境变量前缀（如 `FOO=bar git status`）
    let cmd = if first_word.contains('=') {
        trimmed
            .split_whitespace()
            .find(|w| !w.contains('='))
            .unwrap_or("")
    } else {
        first_word
    };
    matches!(
        cmd,
        "git"
            | "cargo"
            | "npm"
            | "pnpm"
            | "npx"
            | "yarn"
            | "bun"
            | "bunx"
            | "docker"
            | "kubectl"
            | "pytest"
            | "python"
            | "php"
            | "go"
            | "dotnet"
            | "tsc"
            | "eslint"
            | "gh"
            | "find"
            | "grep"
            | "rg"
            | "ls"
            | "tree"
            | "cat"
            | "diff"
            | "curl"
            | "wget"
    )
}

/// 尝试使用 `rtk rewrite "<command>"` 重写命令。
///
/// 如果系统中存在 `rtk`，且 `rtk rewrite` 执行成功（exit_code == 0 且 stdout 非空），
/// 则返回重写后的命令（例如 "rtk git status"）。
/// 否则返回 None。
pub async fn rtk_rewrite_command(command: &str) -> Option<String> {
    if !is_potential_rtk_command(command) {
        return None;
    }
    let rtk_exe = rtk_path()?;
    let output = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        tokio::process::Command::new(rtk_exe)
            .arg("rewrite")
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if output.status.success() {
        let rewritten = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !rewritten.is_empty() && rewritten != command {
            return Some(rewritten);
        }
    }
    None
}

#[cfg(test)]
mod process_test;
