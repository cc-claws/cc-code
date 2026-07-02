use std::{process::Stdio, time::Instant};

use anyhow::{Context, Result};
use peri_agent::encoding::decode_output_bytes;
use peri_agent::shell::ShellAbortHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// 流式执行累积 stdout/stderr 的最大字节数（超出截断，防止大输出命令 OOM）。
/// 完整输出仍写入磁盘（DiskOutput），acc 截断仅影响 result CommandOutput（用于 shell history）。
const MAX_ACCUMULATED_BYTES: usize = 8 * 1024 * 1024;

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
        .with_context(|| format!("Failed to spawn shell command: {}", command))?;

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

    let mut stdout = child
        .stdout
        .take()
        .context("Failed to capture shell stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .context("Failed to capture shell stderr")?;

    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await.map(|_| buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).await.map(|_| buf)
    });

    let status = child.wait().await?;
    let stdout_bytes = stdout_task.await.context("stdout read task failed")??;
    let stderr_bytes = stderr_task.await.context("stderr read task failed")??;

    Ok(CommandOutput {
        stdout: decode_output_bytes(&stdout_bytes),
        stderr: decode_output_bytes(&stderr_bytes),
        exit_code: status.code().unwrap_or(-1),
    })
}

/// 流式 Shell 执行句柄：持有进程退出 result channel、abort handle 和流式输出 channel。
///
/// 与 [`execute_shell_command_with_stdin`] 不同，本函数不一次性读取 stdout/stderr，
/// 而是通过 `output_rx` 流式推送每个读取块，供 Ctrl+B 后台化时切换输出目标
/// （前台渲染到 UI / 后台写磁盘），进程全程不中断。
pub struct ShellExecution {
    /// 进程退出时 resolve 的 result channel（含 stdout/stderr/exit_code）
    pub result: oneshot::Receiver<Result<CommandOutput>>,
    /// 进程 kill 句柄
    pub abort: ShellAbortHandle,
    /// 流式输出 channel（stdout + stderr 合并推送）
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    /// 子进程成功 spawn 后的真实启动时刻。
    pub started_instant: Instant,
}

/// 流式执行 shell 命令：stdout/stderr 通过 `output_rx` 流式推送，进程退出时
/// 通过 `result` 返回完整 [`CommandOutput`]。
///
/// `stdin_rx` 由调用方创建并持有 sender（与 [`execute_shell_command_with_stdin`]
/// 一致），用于向前台 shell 命令发送 stdin 输入；直接后台 spawn 路径传 `None`。
///
/// **注意**：调用方必须持续消费 `output_rx`，否则 channel 缓冲（256）写满后
/// 会阻塞内部 reader task，导致进程 stdout/stderr 管道阻塞。
pub fn execute_shell_command_streaming(
    command: &str,
    cwd: &str,
    stdin_rx: Option<mpsc::Receiver<String>>,
) -> ShellExecution {
    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);
    let (result_tx, result_rx) = oneshot::channel::<Result<CommandOutput>>();

    match spawn_streaming_child(command, cwd, stdin_rx.is_some()) {
        Ok((child, started_instant)) => {
            let handle = tokio::spawn(async move {
                let result = run_streaming_child(child, stdin_rx, output_tx).await;
                // task 被 abort 时 result_tx drop，result_rx 收到 Canceled，调用方需处理
                let _ = result_tx.send(result);
            });
            let abort = ShellAbortHandle::from_tokio_abort(handle.abort_handle());
            drop(handle);

            ShellExecution {
                result: result_rx,
                abort,
                output_rx,
                started_instant,
            }
        }
        Err(error) => {
            drop(output_tx);
            let _ = result_tx.send(Err(error));
            ShellExecution {
                result: result_rx,
                abort: ShellAbortHandle::noop(),
                output_rx,
                started_instant: Instant::now(),
            }
        }
    }
}

fn spawn_streaming_child(
    command: &str,
    cwd: &str,
    has_stdin: bool,
) -> Result<(tokio::process::Child, Instant)> {
    let command = streaming_command_with_unbuffered_interpreters(command);
    let mut cmd = peri_middlewares::process::shell_command(&command, &[]);
    apply_streaming_unbuffered_env(&mut cmd);
    if !cwd.trim().is_empty() {
        cmd.current_dir(cwd);
    }
    if has_stdin {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn shell command: {}", command))?;
    let started_instant = Instant::now();

    Ok((child, started_instant))
}

/// 流式执行的实际逻辑：stdout/stderr 流式读取推送 + 累积，
/// 进程退出后返回累积的 CommandOutput。
async fn run_streaming_child(
    mut child: tokio::process::Child,
    mut stdin_rx: Option<mpsc::Receiver<String>>,
    output_tx: mpsc::Sender<Vec<u8>>,
) -> Result<CommandOutput> {
    // stdin 写入 task（与 execute_shell_command_with_stdin 一致）
    if let Some(mut rx) = stdin_rx.take() {
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

    let mut stdout = child
        .stdout
        .take()
        .context("Failed to capture shell stdout")?;
    let mut stderr = child
        .stderr
        .take()
        .context("Failed to capture shell stderr")?;

    // 流式读取 stdout/stderr：每个 chunk 推送到 output_tx（合并），同时累积用于 result。
    // 两个 reader task 各持 output_tx 的 clone，原始 output_tx 在末尾 drop，
    // 两者都结束后 channel 关闭，output_rx 消费者收到 None。
    let stdout_task = {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            let mut acc = Vec::new();
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        let _ = tx.send(chunk.clone()).await;
                        if acc.len() < MAX_ACCUMULATED_BYTES {
                            let remaining = MAX_ACCUMULATED_BYTES - acc.len();
                            acc.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                        }
                    }
                    Err(_) => break,
                }
            }
            acc
        })
    };
    let stderr_task = {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            let mut acc = Vec::new();
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        let _ = tx.send(chunk.clone()).await;
                        if acc.len() < MAX_ACCUMULATED_BYTES {
                            let remaining = MAX_ACCUMULATED_BYTES - acc.len();
                            acc.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                        }
                    }
                    Err(_) => break,
                }
            }
            acc
        })
    };
    drop(output_tx);

    let status = child.wait().await?;
    let stdout_bytes = match stdout_task.await {
        Ok(acc) => acc,
        Err(e) => {
            tracing::warn!(error = %e, "stdout reader task failed");
            Vec::new()
        }
    };
    let stderr_bytes = match stderr_task.await {
        Ok(acc) => acc,
        Err(e) => {
            tracing::warn!(error = %e, "stderr reader task failed");
            Vec::new()
        }
    };

    Ok(CommandOutput {
        stdout: decode_output_bytes(&stdout_bytes),
        stderr: decode_output_bytes(&stderr_bytes),
        exit_code: status.code().unwrap_or(-1),
    })
}

fn apply_streaming_unbuffered_env(cmd: &mut tokio::process::Command) {
    // Python 在 stdout 是 pipe 时默认块缓冲；后台面板需要长脚本的首行能及时落盘。
    cmd.env("PYTHONUNBUFFERED", "1");
    cmd.env("PYTHONIOENCODING", "utf-8");
}

fn streaming_command_with_unbuffered_interpreters(command: &str) -> String {
    let trimmed = command.trim_start();
    let leading_len = command.len() - trimmed.len();
    let (program, rest) = split_first_shell_token(trimmed);
    if command_name_matches(program, "php") && !trimmed.contains("implicit_flush") {
        return format!(
            "{}{} -d output_buffering=0 -d implicit_flush=1{}",
            &command[..leading_len],
            program,
            rest
        );
    }
    command.to_string()
}

fn split_first_shell_token(command: &str) -> (&str, &str) {
    let split_at = command
        .char_indices()
        .find_map(|(idx, c)| c.is_whitespace().then_some(idx))
        .unwrap_or(command.len());
    command.split_at(split_at)
}

fn command_name_matches(program: &str, name: &str) -> bool {
    let unquoted = program.trim_matches('"').trim_matches('\'');
    let file_name = unquoted.rsplit(['\\', '/']).next().unwrap_or(unquoted);
    let stem = file_name.strip_suffix(".exe").unwrap_or(file_name);
    stem.eq_ignore_ascii_case(name)
}

#[cfg(test)]
#[path = "shell_exec_test.rs"]
mod tests;
