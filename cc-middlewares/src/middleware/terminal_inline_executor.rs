//! 无后台宿主的 ShellExecutor：确定解释器后执行一次，并发捕获输出。
//!
//! 命令任务的 kill 句柄由前台等待者持有；超时显式 abort。
//! 本执行器没有后台任务服务，必须在 spawn 前拒绝直接后台请求。

use std::sync::Arc;

use async_trait::async_trait;
use cc_agent::shell::{
    AgentShellHandle, ExitSignal, ShellAbortHandle, ShellCommandOutput, ShellExecutor,
    ShellHandoff, ShellOutcome, ShellRequest,
};

pub struct InlineShellExecutor;

struct FinishCancelledOnDrop(Arc<ExitSignal>);

impl Drop for FinishCancelledOnDrop {
    fn drop(&mut self) {
        self.0.finish(ShellOutcome::Cancelled);
    }
}

#[async_trait]
impl ShellExecutor for InlineShellExecutor {
    async fn execute(&self, req: ShellRequest) -> anyhow::Result<AgentShellHandle> {
        anyhow::ensure!(
            !req.run_in_background,
            "This host does not support background commands. Run in the foreground; no command was executed."
        );
        let mut command = crate::process::managed_shell_command(&req.command, req.shell)?;
        command.current_dir(&req.cwd);
        // 注：ManagedChild::spawn 内已统一设置 process_group(0)，此处无需重复。
        let task_id = uuid::Uuid::now_v7().to_string();
        // 前台结果直接返回；无后台服务，不向调用者承诺此占位路径已落盘。
        let output_path = std::env::temp_dir().join(format!("peri-tool-output-{task_id}"));
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let exit_signal = Arc::new(ExitSignal::new());
        let exit_signal_clone = Arc::clone(&exit_signal);
        // 在 spawn 外创建，任务尚未首次 poll 就取消时也能报告终态。
        let cancel_guard = FinishCancelledOnDrop(Arc::clone(&exit_signal));
        let join = tokio::spawn(async move {
            let _cancel_guard = cancel_guard;
            let result = crate::process::output_with_input_timeout(
                command,
                &[],
                std::time::Duration::from_millis(req.execution_timeout_ms),
            )
            .await;
            let timed_out = result
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::TimedOut);
            let result = result
                .map_err(anyhow::Error::from)
                .map(|output| ShellCommandOutput {
                    stdout: cc_agent::encoding::decode_output_bytes(&output.stdout),
                    stderr: cc_agent::encoding::decode_output_bytes(&output.stderr),
                    exit_code: output.status.code().unwrap_or(-1),
                });
            let outcome = match &result {
                Ok(output) => ShellOutcome::Exited(output.exit_code),
                Err(_) if timed_out => ShellOutcome::TimedOut,
                Err(error) => ShellOutcome::Failed(error.to_string()),
            };
            exit_signal_clone.finish(outcome);
            let _ = result_tx.send(result);
        });
        Ok(AgentShellHandle {
            task_id,
            output_path,
            result_rx,
            exit_signal,
            handoff: Arc::new(ShellHandoff::new(false, false)),
            kill: ShellAbortHandle::from_tokio_abort(join.abort_handle()),
        })
    }
}
