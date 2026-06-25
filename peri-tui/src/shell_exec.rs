use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// Captured shell command output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute a shell command in `cwd` and capture stdout/stderr.
pub async fn execute_shell_command(command: &str, cwd: &str) -> Result<CommandOutput> {
    execute_shell_command_with_stdin(command, cwd, None).await
}

/// Execute a shell command with an optional stdin channel.
///
/// When `stdin_rx` is present, every received string is written as one stdin line.
/// Dropping the sender closes stdin and lets commands such as `grep` finish.
pub async fn execute_shell_command_with_stdin(
    command: &str,
    cwd: &str,
    stdin_rx: Option<mpsc::Receiver<String>>,
) -> Result<CommandOutput> {
    let mut cmd = peri_middlewares::process::shell_command(command, &[]);
    if !cwd.trim().is_empty() {
        cmd.current_dir(cwd);
    }
    if stdin_rx.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("执行 shell 命令失败: {}", command))?;

    if let Some(mut rx) = stdin_rx {
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                while let Some(line) = rx.recv().await {
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    if stdin.write_all(b"\n").await.is_err() {
                        break;
                    }
                    if stdin.flush().await.is_err() {
                        break;
                    }
                }
            });
        }
    }

    let mut stdout = child.stdout.take().context("无法捕获 shell stdout")?;
    let mut stderr = child.stderr.take().context("无法捕获 shell stderr")?;

    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.map(|_| buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).await.map(|_| buf)
    });

    let status = child.wait().await?;
    let stdout_bytes = stdout_task.await.context("stdout 读取任务失败")??;
    let stderr_bytes = stderr_task.await.context("stderr 读取任务失败")??;

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr_bytes).to_string(),
        exit_code: status.code().unwrap_or(-1),
    })
}

#[cfg(test)]
#[path = "shell_exec_test.rs"]
mod tests;
