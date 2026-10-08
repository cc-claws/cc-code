use async_trait::async_trait;
use cc_agent::{
    agent::state::State, middleware::r#trait::Middleware, shell::ShellExecutor, tools::BaseTool,
};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::Duration;

use crate::tools::output_persist::truncate_shell_output;

/// BashTool - 终端命令执行工具，与 TypeScript TerminalMiddleware 对齐
const BASH_DESCRIPTION: &str = r#"Executes one command using Bash (Git Bash on Windows) and returns its output.

Usage:
- Each invocation starts in the configured working directory. Changes made with cd, environment assignments, and shell state do not persist across calls. Commands use bash -c, not a login shell
- Prefer dedicated tools for file search, reading, and editing when they support the task. Use Bash for commands, builds, scripts, and operations that need shell execution.
- You can specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). Default is 120000ms (2 minutes)
- Keep dependent commands ordered; use && when a later command should run only after success. Independent operations may run separately.
- In hosts with background support, timeout limits foreground waiting and the command continues in the background; without background support, timeout cancels the command
- execution_timeout is a separate hard runtime limit in milliseconds (default and maximum 600000). It applies from process start, including after manual or automatic backgrounding
- run_in_background requires host support. When a task handle is returned, reuse that task/output path and await its completion notification; do not rerun the command to retrieve output

Platform behavior:
- Windows: requires Git Bash; a missing interpreter is reported before execution
- Unix/macOS: uses bash -c to execute commands
- The interpreter is selected before execution. Failed commands are never automatically rerun in another shell
- Use an explicit interpreter for native CMD or PowerShell scripts; do not mix their syntax into Bash
- Pipelines return the last command status by default. Use set -o pipefail when an earlier failure must fail the whole pipeline

Output handling:
- Output exceeding 50000 bytes is returned as a compact head/tail preview; the full output is saved to a temp file
- If omitted output is needed, use the Read tool on the saved file path rather than rerunning the command
- Non-zero exit codes are reported
- Both stdout and stderr are captured"#;
pub struct BashTool {
    pub cwd: String,
    /// Shell 执行器：把命令委托给应用层（cc-tui shell 池）以支持 Ctrl+B 后台化。
    /// 默认为 [`InlineShellExecutor`]（仅前台，并发捕获 stdout/stderr），
    /// cc-tui 会注入真正的实现。
    pub executor: Arc<dyn ShellExecutor>,
}

impl BashTool {
    /// 创建使用默认 [`InlineShellExecutor`] 的仅前台 BashTool。
    ///
    /// 供测试 / 非 TUI 场景使用。TUI 场景应改用 [`BashTool::with_executor`]。
    pub fn new(cwd: impl Into<String>) -> Self {
        Self::with_executor(cwd, Arc::new(InlineShellExecutor))
    }

    /// 创建注入自定义 [`ShellExecutor`] 的 BashTool。
    pub fn with_executor(cwd: impl Into<String>, executor: Arc<dyn ShellExecutor>) -> Self {
        Self {
            cwd: cwd.into(),
            executor,
        }
    }
}

fn truncate_output(output: &str) -> String {
    // Bash 输出面向 LLM 使用更小 preview；完整内容由 output_persist 写入临时文件。
    truncate_shell_output(output)
}

/// 把 stdout/stderr/exit_code 拼装为给 LLM 的工具结果字符串。
///
/// 保留 stdout、stderr 和真实退出码，不对失败命令重新执行。
///
/// 公开供 TUI 复用：前台 `!` 命令回流 Agent 上下文时（`shell_context_messages`），
/// 需与 agent 自身执行 Bash 保持同一输出格式。
pub fn format_command_output(stdout: &str, stderr: &str, exit_code: i32) -> String {
    let mut output = String::new();
    if !stdout.is_empty() {
        output.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("[stderr]\n");
        output.push_str(stderr);
    }
    if exit_code != 0 {
        output.push_str(&format!("\n[Exit code: {exit_code}]"));
    }
    if output.is_empty() {
        output = format!("[Command completed with exit code {exit_code}]");
    }
    output
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn format_background_task_started(task_id: &str, command: &str, output_path: &Path) -> String {
    format!(
        "<background-task-started><task-id>{}</task-id><command>{}</command><output>{}</output></background-task-started>",
        task_id,
        escape_xml_text(command),
        output_path.display()
    )
}

struct ForegroundCommandGuard {
    kill: cc_agent::shell::ShellAbortHandle,
    handoff: Arc<cc_agent::shell::ShellHandoff>,
}

impl Drop for ForegroundCommandGuard {
    fn drop(&mut self) {
        if self.handoff.settle_foreground() {
            self.kill.abort();
        }
    }
}

enum ShellWaitResult {
    Completed(
        Result<anyhow::Result<cc_agent::shell::ShellCommandOutput>, oneshot::error::RecvError>,
    ),
    Backgrounded,
    TimedOut,
}

async fn wait_for_shell_result(
    mut result_rx: oneshot::Receiver<anyhow::Result<cc_agent::shell::ShellCommandOutput>>,
    handoff: &cc_agent::shell::ShellHandoff,
    timeout_ms: u64,
) -> ShellWaitResult {
    let timeout_sleep = tokio::time::sleep(Duration::from_millis(timeout_ms));
    tokio::pin!(timeout_sleep);

    tokio::select! {
        biased;
        result = &mut result_rx => {
            // UI 移交与前台结果竞争同一把归属锁，不能同时承诺两种结果。
            if handoff.settle_foreground() || !handoff.is_backgrounded() {
                ShellWaitResult::Completed(result)
            } else {
                ShellWaitResult::Backgrounded
            }
        }
        _ = handoff.wait_for_background() => ShellWaitResult::Backgrounded,
        _ = &mut timeout_sleep => {
            if handoff.background() || handoff.is_backgrounded() {
                ShellWaitResult::Backgrounded
            } else {
                ShellWaitResult::TimedOut
            }
        }
    }
}

#[async_trait::async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        BASH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "REQUIRED. Bash command to execute. Quote paths and arguments for Bash; use && for commands that depend on earlier success"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds (default 120000, max 600000). In the TUI host, foreground commands that exceed this continue in the background; in hosts without background support they are killed and a timeout error is returned"
                },
                "execution_timeout": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 600000,
                    "description": "Hard execution limit in milliseconds (default 600000, max 600000). Starts when the process starts and never resets after backgrounding. The process is terminated at this deadline"
                },
                "description": {
                    "type": "string",
                    "description": "A clear, concise description of what this command does in active voice. Never use words like 'complex' or 'risk' in the description — just describe what it does"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later"
                }
            },
            "required": ["command"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing command parameter")?;

        let timeout_ms = input["timeout"]
            .as_u64()
            .unwrap_or(120_000)
            .clamp(1, 600_000);
        let _description = input["description"].as_str();
        let execution_timeout_ms = match input.get("execution_timeout") {
            None => 600_000,
            Some(value) => value
                .as_u64()
                .filter(|ms| (1..=600_000).contains(ms))
                .ok_or("execution_timeout must be an integer between 1 and 600000 milliseconds")?,
        };
        let run_in_background = input["run_in_background"].as_bool().unwrap_or(false);

        let user_command = command.to_string();
        // 轨一：尝试使用 RTK 重写命令（对齐 Claude Code / Codex 代理模式）
        let (command, is_rtk_rewritten) =
            if let Some(rewritten) = crate::process::rtk_rewrite_command(command).await {
                (rewritten, true)
            } else {
                (command.to_string(), false)
            };
        // 委托给 ShellExecutor：cc-tui 注入的实现会把命令接入 shell 池，
        // 支持 Ctrl+B 后台化；默认 InlineShellExecutor 仅支持前台执行。
        let req = cc_agent::shell::ShellRequest {
            owner_session_id: None,
            invocation: cc_agent::tools::ToolInvocationContext::current(),
            command: command.to_string(),
            original_command: user_command.clone(),
            shell: cc_agent::shell::ShellDialect::Bash,
            cwd: self.cwd.clone(),
            timeout_ms,
            execution_timeout_ms,
            run_in_background,
        };
        let handle = self.executor.execute(req).await?;

        // run_in_background=true：立即转后台，返回 task_id 占位串。
        // 真实输出靠后续 <background-task-completed> 通知注入下一轮对话。
        if run_in_background {
            #[allow(clippy::needless_borrow)]
            return Ok(format_background_task_started(
                &handle.task_id,
                &user_command,
                &handle.output_path,
            ));
        }

        // 前台执行：等待进程退出、用户手动后台化或宿主支持的自动后台化超时。
        let kill = handle.kill;
        // 工具 future 因 Agent 取消而被丢弃时，也要停止尚未移交后台的命令。
        let _foreground = ForegroundCommandGuard {
            kill: kill.clone(),
            handoff: handle.handoff.clone(),
        };
        let task_id = handle.task_id;
        let output_path = handle.output_path;
        let result = wait_for_shell_result(handle.result_rx, &handle.handoff, timeout_ms).await;

        match result {
            #[allow(clippy::needless_borrow)]
            ShellWaitResult::Backgrounded => Ok(format_background_task_started(
                &task_id,
                &user_command,
                &output_path,
            )),
            ShellWaitResult::TimedOut => {
                // 无后台宿主，guard 负责终止仍归前台的命令。
                Err(format!(
                    "Error: Command timed out after {} seconds.\nCommand: {command}",
                    timeout_ms as f64 / 1000.0
                )
                .into())
            }
            // oneshot channel 关闭（executor task 异常退出未 send）
            ShellWaitResult::Completed(Err(_)) => {
                Err("Error: command executor closed unexpectedly".into())
            }
            // executor 返回错误（spawn 失败等）
            ShellWaitResult::Completed(Ok(Err(e))) => {
                Err(format!("Error executing command: {e}").into())
            }
            ShellWaitResult::Completed(Ok(Ok(output))) => {
                let stdout = output.stdout;
                let stderr = crate::tools::output_filter::clean_rtk_stderr_noise(&output.stderr);
                let exit_code = output.exit_code;

                let output = format_command_output(&stdout, &stderr, exit_code);
                // RTK 重写后：仅对 git status 做针对性噪音剔除（filter_git_status
                // 只删噪音行+折叠空行，对 RTK 已压缩输出安全），不做通用折叠
                // 避免对 RTK 已压缩内容二次折叠导致信息丢失。（issue #207）
                // 未重写路径：走 Peri 内置完整语义压缩。
                let output = if is_rtk_rewritten {
                    let cmd_lower = user_command.to_lowercase();
                    if cmd_lower.contains("git status") {
                        crate::tools::output_filter::filter_git_status(&output)
                    } else {
                        output
                    }
                } else {
                    crate::tools::output_filter::filter_command_output(
                        &user_command,
                        &output,
                        exit_code,
                    )
                };
                Ok(truncate_output(&output))
            }
        }
    }
}

/// TerminalMiddleware - 与 TypeScript TerminalMiddleware 对齐
pub struct TerminalMiddleware {
    /// Shell 执行器，注入到 BashTool 以支持 Ctrl+B 后台化。
    /// None 时使用默认 [`InlineShellExecutor`]（仅前台执行）。
    executor: Option<Arc<dyn ShellExecutor>>,
}

impl TerminalMiddleware {
    pub fn new() -> Self {
        Self { executor: None }
    }

    /// 创建注入自定义 [`ShellExecutor`] 的 middleware。
    pub fn with_executor(executor: Arc<dyn ShellExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }

    /// 构造 BashTool。executor 为 None 时使用默认 InlineShellExecutor。
    pub fn build_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        let executor = self
            .executor
            .clone()
            .unwrap_or_else(|| Arc::new(InlineShellExecutor));
        vec![Box::new(BashTool::with_executor(cwd, executor))]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["Bash"]
    }
}

impl Default for TerminalMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for TerminalMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        self.build_tools(cwd)
    }

    fn name(&self) -> &str {
        "TerminalMiddleware"
    }
}

#[path = "terminal_inline_executor.rs"]
mod terminal_inline_executor;
pub use terminal_inline_executor::InlineShellExecutor;

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
