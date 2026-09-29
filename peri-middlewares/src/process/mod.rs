//! Cross-platform shell command spawning.
//!
//! On Unix, wraps commands in `bash -c "<command> <args...>"`.
//! Legacy platform defaults use CMD on Windows, with pre-execution routing for
//! multiline commands and leading bash/sh invocations. Managed Bash requests
//! explicitly select Git Bash via [`managed_shell_command`].
//! 用户命令启动前选定解释器；执行后不自动切换 shell 或重放。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod capture;
pub(crate) use capture::output_with_input_timeout;
pub use capture::ManagedChild;

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

/// 受管理的命令执行器共用入口，不根据执行结果重放命令。
/// BashTool 在 Windows 上要求 Git Bash；缺失时不能退回 CMD 或 WSL。
pub fn managed_shell_command(
    command: &str,
    shell: peri_agent::shell::ShellDialect,
) -> std::io::Result<tokio::process::Command> {
    use peri_agent::shell::ShellDialect;
    match shell {
        ShellDialect::PlatformDefault => Ok(shell_command(command, &[])),
        ShellDialect::Bash => {
            #[cfg(windows)]
            {
                selected_git_bash_command(command, git_bash_path().as_deref())
            }
            #[cfg(not(windows))]
            {
                Ok(shell_command_with_shell(command, &[], Some("bash")))
            }
        }
    }
}

fn selected_git_bash_command(
    command: &str,
    bash: Option<&Path>,
) -> std::io::Result<tokio::process::Command> {
    selected_git_bash_shell_command("bash", command, bash)
}

fn selected_git_bash_shell_command(
    shell: &str,
    command: &str,
    bash: Option<&Path>,
) -> std::io::Result<tokio::process::Command> {
    let bash = bash.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Bash requires Git Bash on Windows. Install Git for Windows or set GIT_BASH_PATH, then restart. No command was executed; CMD fallback is disabled.",
        )
    })?;
    // PATH 可能解析到 System32/bash.exe（WSL）。它与当前 Windows cwd/进程域
    // 不兼容；只接受原生 Git/MSYS Bash 布局，不通过执行用户命令来试错。
    if !has_msys_runtime(bash) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Bash requires a native Git Bash/MSYS installation on Windows, not WSL. Set GIT_BASH_PATH to its bash.exe and restart. No command was executed.",
        ));
    }
    let executable = if shell == "sh" {
        bash.with_file_name("sh.exe")
    } else {
        bash.to_path_buf()
    };
    if !executable.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Git Bash interpreter '{}' is missing; no command was executed",
                executable.display()
            ),
        ));
    }
    Ok(git_bash_command(&executable, command, &[]))
}

/// Build a `tokio::process::Command` that executes the given command through the
/// specified shell or platform default.
///
/// - **shell = Some("powershell") / Some("pwsh")**: the selected PowerShell executable
/// - **shell = Some("bash") / Some("sh")**: the selected POSIX shell (Git on Windows)
/// - **shell = Some("cmd")**: CMD, without automatic Bash routing
/// - **shell = None**: platform default; Windows routes multiline commands and
///   explicit bash/sh invocations through Git Bash when available
/// - Other explicit shells are executable names/paths accepting `-c`.
///
/// Explicit shells never fall back to a different interpreter on spawn failure.
///
/// Returns the `Command` object so callers can add custom configuration.
pub fn shell_command_with_shell(
    command: &str,
    args: &[&str],
    shell: Option<&str>,
) -> tokio::process::Command {
    try_shell_command_with_shell(command, args, shell).unwrap_or_else(|_| {
        let name = match shell.map(str::to_ascii_lowercase).as_deref() {
            Some("sh") => "sh",
            _ => "bash",
        };
        unavailable_posix_shell_command(name, command, args)
    })
}

/// Build a shell command while surfacing missing explicit interpreters before spawn.
pub fn try_shell_command_with_shell(
    command: &str,
    args: &[&str],
    shell: Option<&str>,
) -> std::io::Result<tokio::process::Command> {
    let shell_lower = shell.map(|s| s.to_lowercase());

    match shell_lower.as_deref() {
        Some(name @ ("powershell" | "pwsh")) => {
            let mut cmd = background_shell(name);
            cmd.arg("-NoProfile").arg("-NonInteractive").arg("-Command");
            // PowerShell -Command 需要整个命令作为单个参数
            let full_command = if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            cmd.arg(full_command);
            Ok(cmd)
        }
        Some(name @ ("bash" | "sh")) => {
            if cfg!(target_os = "windows") {
                return selected_git_bash_shell_command(name, command, git_bash_path().as_deref());
            }
            // Let spawn report a missing interpreter; CMD cannot interpret POSIX syntax.
            Ok(posix_shell_command(Path::new(name), command, args))
        }
        Some("cmd") => Ok(shell_command_cmd(command, args)),
        Some(_) => Ok(posix_shell_command(
            Path::new(shell.unwrap_or_default()),
            command,
            args,
        )),
        None => {
            // Default: platform shell (cmd on Windows, bash on Unix)
            if cfg!(target_os = "windows") {
                // cmd /C 无法正确处理含字面换行符的多行命令——只执行第一行，
                // 后续行的 stdout 全部丢失（Issue #212）。检测到换行时直接走
                // Git Bash，bash -c 能正确处理多行命令。
                if let Some(bash_exe) = auto_git_bash_path(command) {
                    return Ok(git_bash_command(&bash_exe, command, args));
                }
                if starts_with_posix_shell(command) {
                    return selected_git_bash_command(command, None);
                }
                Ok(shell_command_cmd(command, args))
            } else {
                Ok(posix_shell_command(Path::new("bash"), command, args))
            }
        }
    }
}

/// Legacy platform-default routing for hooks and native user commands.
fn auto_git_bash_path(command: &str) -> Option<PathBuf> {
    if cfg!(windows) && (command.contains('\n') || starts_with_posix_shell(command)) {
        git_bash_path()
    } else {
        None
    }
}

/// An explicit POSIX interpreter request must not fall through to CMD's PATH,
/// where `bash.exe` may be the WSL launcher rather than Git Bash.
fn unavailable_posix_shell_command(
    shell: &str,
    command: &str,
    args: &[&str],
) -> tokio::process::Command {
    let missing =
        std::env::temp_dir().join(format!("peri-missing-{shell}-{}.exe", uuid::Uuid::new_v4()));
    posix_shell_command(&missing, command, args)
}

/// Only recognize a complete leading interpreter name, not arbitrary shell syntax.
/// Keep the original command intact, including quotes, flags and redirections.
fn starts_with_posix_shell(command: &str) -> bool {
    let command = command.trim_start();
    let boundary = |c: char| c.is_ascii_whitespace() || matches!(c, '&' | '|' | ';' | '<' | '>');
    let name = if let Some(quote @ ('\'' | '"')) = command.chars().next() {
        let rest = &command[quote.len_utf8()..];
        let Some(end) = rest.find(quote) else {
            return false;
        };
        if !rest[end + quote.len_utf8()..].starts_with(boundary)
            && rest.len() != end + quote.len_utf8()
        {
            return false;
        }
        &rest[..end]
    } else {
        command.split(boundary).next().unwrap_or_default()
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "bash" | "bash.exe" | "sh" | "sh.exe"
    )
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
        if p.exists() && has_msys_runtime(&p) && verify_bash_executable(&p) {
            return Some(p);
        }
    }

    let candidates: &[&str] = &[
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];
    for path in candidates {
        let p = Path::new(path);
        if p.exists() && has_msys_runtime(p) && verify_bash_executable(p) {
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
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let p = Path::new(trimmed);
        if p.exists() && has_msys_runtime(p) && verify_bash_executable(p) {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn has_msys_runtime(bash: &Path) -> bool {
    bash.parent().is_some_and(|bin| {
        bin.join("msys-2.0.dll").is_file()
            || bin
                .parent()
                .is_some_and(|root| root.join("usr/bin/msys-2.0.dll").is_file())
    })
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

/// 用显式指定的 bash 可执行文件构造 `bash -c "<command> <args...>"`。
///
/// 与 [`shell_command`] 的 Unix 分支语义一致，但允许调用方指定 Git Bash 路径，
/// 用于显式选择 Git Bash 的执行请求。
pub fn git_bash_command(bash_exe: &Path, command: &str, args: &[&str]) -> tokio::process::Command {
    let mut cmd = posix_shell_command(bash_exe, command, args);
    // 禁用 MSYS2/MinGW 的自动路径转换，防止 /pattern 等参数被转为 Windows 路径
    cmd.env("MSYS_NO_PATHCONV", "1");
    cmd
}

fn posix_shell_command(shell: &Path, command: &str, args: &[&str]) -> tokio::process::Command {
    let mut parts = vec![command.to_string()];
    for arg in args {
        if arg.contains(' ') || arg.contains('"') || arg.contains('\'') || arg.contains('\\') {
            parts.push(format!("'{}'", arg.replace('\'', "'\\''")));
        } else {
            parts.push(arg.to_string());
        }
    }
    let shell_cmd = parts.join(" ");
    let mut cmd = background_shell(shell);
    cmd.arg("-c").arg(&shell_cmd);
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
