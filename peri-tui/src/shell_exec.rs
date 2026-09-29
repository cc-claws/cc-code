use std::sync::{Arc, Mutex};
use std::{process::Stdio, time::Duration, time::Instant};

use anyhow::{Context, Result};
use peri_agent::encoding::decode_output_bytes;
use peri_agent::shell::{ShellAbortHandle, ShellDialect};
use peri_middlewares::process::ManagedChild;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// 流式执行累积 stdout/stderr 的最大字节数（超出截断，防止大输出命令 OOM）。
/// 完整输出仍写入磁盘（DiskOutput），acc 截断仅影响 result CommandOutput（用于 shell history）。
const MAX_ACCUMULATED_BYTES: usize = 8 * 1024 * 1024;

/// 主进程退出后等待 stdout/stderr 管道 EOF 的超时时间。
///
/// Windows 上通过 `Start-Process` 等方式 fork 出的常驻子进程会继承父进程的管道写句柄，
/// 即使主进程已退出，写端仍然打开，读取端永远收不到 EOF。超时后中止读取任务，
/// 使用已累积的数据返回结果，避免后台任务永远停留在 running 状态。
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Captured shell command output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// 根进程未在执行期限内退出；输出排空耗时不属于执行超时。
#[derive(Debug, thiserror::Error)]
#[error("Shell command exceeded its hard execution deadline")]
pub struct ShellExecutionTimedOut;

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

    let mut child = ManagedChild::spawn(cmd)
        .with_context(|| format!("Failed to spawn shell command: {}", command))?;
    let mut io_tasks = ShellIoTasks::default();

    if let Some(mut rx) = stdin_rx {
        if let Some(mut stdin) = child.take_stdin() {
            io_tasks.spawn(async move {
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
        .take_stdout()
        .context("Failed to capture shell stdout")?;
    let mut stderr = child
        .take_stderr()
        .context("Failed to capture shell stderr")?;

    let stdout_acc = Arc::new(Mutex::new(Vec::new()));
    let stdout_acc_clone = stdout_acc.clone();
    let stdout_task = io_tasks.spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = stdout_acc_clone.lock() {
                        if guard.len() < MAX_ACCUMULATED_BYTES {
                            let remaining = MAX_ACCUMULATED_BYTES - guard.len();
                            guard.extend_from_slice(&buf[..n.min(remaining)]);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "stdout read 失败");
                    break;
                }
            }
        }
    });

    let stderr_acc = Arc::new(Mutex::new(Vec::new()));
    let stderr_acc_clone = stderr_acc.clone();
    let stderr_task = io_tasks.spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = stderr_acc_clone.lock() {
                        if guard.len() < MAX_ACCUMULATED_BYTES {
                            let remaining = MAX_ACCUMULATED_BYTES - guard.len();
                            guard.extend_from_slice(&buf[..n.min(remaining)]);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "stderr read 失败");
                    break;
                }
            }
        }
    });

    let status = child.wait().await?;
    // 根进程退出即结束 Job 生命周期；后代不能继续占用管道或运行副作用。
    drop(child);
    tokio::join!(drain_pipe_task(stdout_task), drain_pipe_task(stderr_task));

    let stdout_bytes = stdout_acc.lock().map(|g| g.clone()).unwrap_or_default();
    let stderr_bytes = stderr_acc.lock().map(|g| g.clone()).unwrap_or_default();

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
    execute_shell_command_streaming_with_shell(
        command,
        cwd,
        stdin_rx,
        ShellDialect::PlatformDefault,
    )
    .unwrap_or_else(|error| {
        let (output_tx, output_rx) = mpsc::channel(1);
        let (result_tx, result) = oneshot::channel();
        drop(output_tx);
        let _ = result_tx.send(Err(error));
        ShellExecution {
            result,
            abort: ShellAbortHandle::noop(),
            output_rx,
            started_instant: Instant::now(),
        }
    })
}

/// Agent 命令必须透传启动前选定的 shell，不能回到平台默认值或自行重试。
/// 启动失败同步返回错误，直接后台调用不能把失败包装成已启动任务。
pub fn execute_shell_command_streaming_with_shell(
    command: &str,
    cwd: &str,
    stdin_rx: Option<mpsc::Receiver<String>>,
    shell: ShellDialect,
) -> Result<ShellExecution> {
    execute_streaming(command, cwd, stdin_rx, shell, None)
}

/// 执行期限从实际启动时刻计算，只约束根进程运行时间。
/// 根进程退出或被终止后仍有界排空输出，不能把这段收尾误报成执行超时。
pub fn execute_shell_command_streaming_with_timeout(
    command: &str,
    cwd: &str,
    stdin_rx: Option<mpsc::Receiver<String>>,
    shell: ShellDialect,
    execution_timeout: Duration,
) -> Result<ShellExecution> {
    execute_streaming(command, cwd, stdin_rx, shell, Some(execution_timeout))
}

fn execute_streaming(
    command: &str,
    cwd: &str,
    stdin_rx: Option<mpsc::Receiver<String>>,
    shell: ShellDialect,
    execution_timeout: Option<Duration>,
) -> Result<ShellExecution> {
    let (child, started_instant) = spawn_streaming_child(command, cwd, stdin_rx.is_some(), shell)?;
    let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);
    let (result_tx, result_rx) = oneshot::channel::<Result<CommandOutput>>();

    let handle = tokio::spawn(async move {
        let deadline = execution_timeout.map(|duration| started_instant + duration);
        let result = run_streaming_child(child, stdin_rx, output_tx, deadline).await;
        // task 被 abort 时 result_tx drop，result_rx 收到 Canceled，调用方需处理
        let _ = result_tx.send(result);
    });
    let abort = ShellAbortHandle::from_tokio_abort(handle.abort_handle());
    drop(handle);
    Ok(ShellExecution {
        result: result_rx,
        abort,
        output_rx,
        started_instant,
    })
}

fn spawn_streaming_child(
    command: &str,
    cwd: &str,
    has_stdin: bool,
    shell: ShellDialect,
) -> Result<(ManagedChild, Instant)> {
    let command = streaming_command_with_unbuffered_interpreters(command);
    let mut cmd = peri_middlewares::process::managed_shell_command(&command, shell)?;
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

    let child = ManagedChild::spawn(cmd)
        .with_context(|| format!("Failed to spawn shell command: {}", command))?;
    let started_instant = Instant::now();

    Ok((child, started_instant))
}

/// 流式执行的实际逻辑：stdout/stderr 流式读取推送 + 累积，
/// 进程退出后返回累积的 CommandOutput。
async fn run_streaming_child(
    mut child: ManagedChild,
    mut stdin_rx: Option<mpsc::Receiver<String>>,
    output_tx: mpsc::Sender<Vec<u8>>,
    deadline: Option<Instant>,
) -> Result<CommandOutput> {
    let mut io_tasks = ShellIoTasks::default();
    // stdin 写入 task（与 execute_shell_command_with_stdin 一致）
    if let Some(mut rx) = stdin_rx.take() {
        if let Some(mut stdin) = child.take_stdin() {
            io_tasks.spawn(async move {
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
        .take_stdout()
        .context("Failed to capture shell stdout")?;
    let mut stderr = child
        .take_stderr()
        .context("Failed to capture shell stderr")?;

    // 流式读取 stdout/stderr：每个 chunk 推送到 output_tx（合并），同时累积用于 result。
    // 两个 reader task 各持 output_tx 的 clone，原始 output_tx 在末尾 drop，
    // 两者都结束后 channel 关闭，output_rx 消费者收到 None。
    let stdout_acc = Arc::new(Mutex::new(Vec::new()));
    let stdout_acc_clone = stdout_acc.clone();
    let stdout_task = {
        let tx = output_tx.clone();
        io_tasks.spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        let _ = tx.send(chunk.clone()).await;
                        if let Ok(mut guard) = stdout_acc_clone.lock() {
                            if guard.len() < MAX_ACCUMULATED_BYTES {
                                let remaining = MAX_ACCUMULATED_BYTES - guard.len();
                                guard.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        })
    };
    let stderr_acc = Arc::new(Mutex::new(Vec::new()));
    let stderr_acc_clone = stderr_acc.clone();
    let stderr_task = {
        let tx = output_tx.clone();
        io_tasks.spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        let _ = tx.send(chunk.clone()).await;
                        if let Ok(mut guard) = stderr_acc_clone.lock() {
                            if guard.len() < MAX_ACCUMULATED_BYTES {
                                let remaining = MAX_ACCUMULATED_BYTES - guard.len();
                                guard.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        })
    };
    drop(output_tx);

    let status = if let Some(deadline) = deadline {
        match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), child.wait()).await
        {
            Ok(status) => status.map_err(anyhow::Error::from),
            Err(_) => Err(ShellExecutionTimedOut.into()),
        }
    } else {
        child.wait().await.map_err(anyhow::Error::from)
    };
    drop(child);

    // 主进程已退出，带超时等待管道 reader task 结束。
    // Windows 上常驻子进程可能继承管道写句柄导致 EOF 永远不到达，
    // 超时后停止等待并使用已累积的数据返回，防止后台任务永久挂起。
    tokio::join!(drain_pipe_task(stdout_task), drain_pipe_task(stderr_task));

    let status = status?;

    let stdout_bytes = stdout_acc.lock().map(|g| g.clone()).unwrap_or_default();
    let stderr_bytes = stderr_acc.lock().map(|g| g.clone()).unwrap_or_default();

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

/// 带超时等待管道 reader task：主进程退出后最多等 [`PIPE_DRAIN_TIMEOUT`]，
/// 超时则取消并回收任务；丢弃 JoinHandle 本身只会 detach，不能停止读取。
///
/// Windows 上 `Start-Process` / `cmd /C start` 等方式 fork 的常驻子进程会继承
/// 父进程的 stdout/stderr 管道写句柄。主进程退出后写端仍未关闭，`read()` 永远
/// 等不到 EOF，导致 reader task 无限挂起。超时机制确保后台任务能正常完成。
async fn drain_pipe_task<T>(mut task: tokio::task::JoinHandle<T>) {
    if tokio::time::timeout(PIPE_DRAIN_TIMEOUT, &mut task)
        .await
        .is_err()
    {
        tracing::debug!(
            "主进程退出后管道 reader 超时（{}s），可能存在继承管道句柄的常驻子进程",
            PIPE_DRAIN_TIMEOUT.as_secs()
        );
        task.abort();
        let _ = task.await;
    }
}

/// 执行 future 被取消或提前报错时，所有独立 IO 任务必须一同停止。
/// JoinHandle 默认 drop 会分离任务，无法单独承担这个生命周期约束。
#[derive(Default)]
struct ShellIoTasks(Vec<tokio::task::AbortHandle>);

impl ShellIoTasks {
    fn spawn(
        &mut self,
        future: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> tokio::task::JoinHandle<()> {
        let task = tokio::spawn(future);
        self.0.push(task.abort_handle());
        task
    }
}

impl Drop for ShellIoTasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

#[cfg(test)]
#[path = "shell_exec_test.rs"]
mod tests;

#[cfg(test)]
#[path = "shell_exec_lifecycle_test.rs"]
mod lifecycle_tests;
