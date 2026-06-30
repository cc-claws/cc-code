//! 内联 Shell 执行器：默认 [`ShellExecutor`] 实现，保留 BashTool 原同步行为。
//!
//! peri-tui 会注入真正的实现（接入 shell 池 + Ctrl+B 后台化）。本实现用于：
//! - 测试 / 非 TUI 场景
//! - CLI 等不需要后台化的宿主
//!
//! 行为：spawn 子进程 → 读 stdout/stderr → wait 退出 → 通过 oneshot 发回
//! [`ShellCommandOutput`] + fire [`ExitSignal`]。
//!
//! # 子进程生命周期（超时/取消清理）
//!
//! 子进程由独立 tokio task 持有（`child` 作为 task 栈帧的局部变量）。
//! `child` 设了 `kill_on_drop(true)`：当 task 被 `AbortHandle::abort()` 取消
//! 时，task future 被 drop → `child` 被 drop → 进程被 kill。
//! [`BashTool::invoke`] 在 `result_rx` 超时后会丢弃 [`AgentShellHandle`]，
//! 其 `Drop` 会 abort 该 task，确保子进程被清理，不泄漏。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use peri_agent::shell::{
    AgentShellHandle, ExitSignal, ShellAbortHandle, ShellCommandOutput, ShellExecutor, ShellRequest,
};
use tokio::io::AsyncReadExt;

/// 默认 ShellExecutor：直接 spawn 子进程并等待，保持 BashTool 原 cmd.output() 行为。
pub struct InlineShellExecutor;

#[async_trait]
impl ShellExecutor for InlineShellExecutor {
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle> {
        let ShellRequest { command, cwd, .. } = req;

        let task_id = uuid::Uuid::now_v7().to_string();
        // 内联执行器不写磁盘，output_path 仅作占位（宿主真正实现时写 DiskOutput）。
        let output_path = std::env::temp_dir().join(format!("peri-tool-output-{task_id}"));

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let exit_signal = Arc::new(ExitSignal::new());
        let exit_signal_clone = Arc::clone(&exit_signal);
        // background_tx / background_rx：内联执行器无后台 UI，仅静默丢弃。
        let (background_tx, background_rx) = tokio::sync::oneshot::channel::<()>();

        let join = tokio::spawn(async move {
            // background_rx 在进程跑期间保持存活；收到信号仅表示"请求后台化"，
            // 内联执行器无后台语义，这里不做切换。drop 它以静默处理。
            drop(background_rx);

            let mut cmd = crate::process::shell_command(&command, &[]);
            cmd.current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            cmd.process_group(0);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let _ = result_tx.send(Err(anyhow::anyhow!("Error executing command: {e}")));
                    exit_signal_clone.fire();
                    return;
                }
            };

            // 读 stdout/stderr（拿管道所有权），再 wait。
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut stdout_buf).await;
            }
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_end(&mut stderr_buf).await;
            }
            // child 此时仍持有进程句柄（kill_on_drop 生效）；wait 拿退出码。
            let status = child.wait().await;

            let result = match status {
                Err(e) => Err(anyhow::anyhow!("Error executing command: {e}")),
                Ok(status) => Ok(ShellCommandOutput {
                    stdout: String::from_utf8_lossy(&stdout_buf).to_string(),
                    stderr: String::from_utf8_lossy(&stderr_buf).to_string(),
                    exit_code: status.code().unwrap_or(-1),
                }),
            };
            let _ = result_tx.send(result);
            exit_signal_clone.fire();
            // 若调用方已超时返回，result_tx 会 Err（recv 端 drop）——子进程仍正常退出，
            // child 被 drop 时若已退出则 kill_on_drop 无操作。
        });

        Ok(AgentShellHandle {
            task_id,
            output_path: PathBuf::from(output_path),
            result_rx,
            exit_signal,
            background_tx: Some(background_tx),
            kill: ShellAbortHandle::from_tokio_abort(join.abort_handle()),
        })
    }
}
