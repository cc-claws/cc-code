//! Cross-platform shell command spawning.
//!
//! On Unix, wraps commands in `bash -c "<command> <args...>"`.
//! On Windows, wraps commands in `cmd /C <command> <args...>`.

use std::{collections::HashMap, io, process::Stdio};

/// Build a `tokio::process::Command` that executes the given command through the
/// platform shell.
///
/// - **Unix**: `bash -c "<command> <args...>"`
/// - **Windows**: `cmd /C <command> <args...>`
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

/// Spawn a command through the platform shell with common defaults:
/// - piped stdin/stdout/stderr
/// - `kill_on_drop(true)`
/// - Unix: `process_group(0)` for clean process tree termination
pub fn spawn_shell(command: &str, args: &[&str]) -> io::Result<tokio::process::Child> {
    let mut cmd = shell_command(command, args);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}

/// Same as `spawn_shell` but with additional environment variables.
pub fn spawn_shell_with_env(
    command: &str,
    args: &[&str],
    env: &HashMap<String, String>,
) -> io::Result<tokio::process::Child> {
    let mut cmd = shell_command(command, args);
    cmd.envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.spawn()
}

#[cfg(test)]
mod process_test;
