//! Monitor 工具：以监控语义启动一个长期后台命令（流式输出、日志跟踪、轮询接口）。
//!
//! 对齐 CC 的 MonitorTool（`packages/builtin-tools/src/tools/MonitorTool`）。
//!
//! 与 `Bash(run_in_background: true)` 的关系：底层走同一个 [`ShellExecutor`]
//! 后台通道，差别在语义与提示——
//! - Bash 的后台语义是「一次性命令，跑完通知我」；
//! - Monitor 的语义是「持续产出事件流，我要随时 Read 输出文件」。
//!
//! 独立工具的存在意义是把这两种意图分开，让模型在系统提示层面就知道
//! 「盯日志 / 轮询接口」该用 Monitor，而不是自己拼一个 `while true; do ...; sleep 5; done`
//! 塞进 Bash 前台。
//!
//! 注意：Monitor 自己发起的命令**不做**阻塞 sleep 拦截——`while true; do
//! check; sleep 5; done` 正是它的典型用法，且命令本身在后台运行，不占用前台轮次。

use async_trait::async_trait;
use cc_agent::shell::{ShellDialect, ShellExecutor, ShellRequest};
use cc_agent::tools::BaseTool;
use serde_json::Value;
use std::sync::Arc;

const MONITOR_DESCRIPTION: &str = r#"Start a long-running background monitor that streams output.

The command runs in the background and you receive a notification when it exits. Use the Read tool with the output file path to check its output at any time.

Usage:
- Required: command and description. Example: Monitor({"command":"tail -f /var/log/app.log","description":"Watch app log for errors"}).
- Use Monitor for commands that produce ongoing streaming output: tail -f, log watchers, file watchers, API polling loops, watch commands.
- Do NOT use Monitor for one-shot commands that finish quickly — use Bash for those.
- Do NOT use Monitor for commands that need interactive input — they will hang.
- The description should clearly explain what is being monitored; it is used as the label in the background task UI.
- You get a task notification when the monitor process exits (stream ends, script fails, or is killed).
- To check output at any time, use Read on the output file path returned by this tool.
- Do not poll a Monitor by rerunning the command. Read its output file, or wait for the completion notification."#;

/// 监控命令的执行硬上限：默认 10 分钟，避免遗忘的 monitor 长期驻留。
const MONITOR_EXECUTION_TIMEOUT_MS: u64 = 600_000;

pub struct MonitorTool {
    cwd: String,
    /// 会话 cwd 句柄：与 Bash 一致，同一轮内的 worktree 切换要能跟随。
    session_cwd: Option<cc_agent::session_cwd::SessionCwd>,
    executor: Arc<dyn ShellExecutor>,
}

impl MonitorTool {
    pub fn with_executor(cwd: impl Into<String>, executor: Arc<dyn ShellExecutor>) -> Self {
        Self {
            cwd: cwd.into(),
            session_cwd: None,
            executor,
        }
    }

    pub fn with_session_cwd(mut self, cwd: cc_agent::session_cwd::SessionCwd) -> Self {
        self.session_cwd = Some(cwd);
        self
    }
}

#[async_trait]
impl BaseTool for MonitorTool {
    fn name(&self) -> &str {
        "Monitor"
    }

    fn description(&self) -> &str {
        MONITOR_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to run as a long-running monitor. Should produce streaming output (e.g., tail -f, watch, polling loops)"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this monitor watches. Used as the label in the background tasks UI"
                }
            },
            "required": ["command", "description"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .ok_or("Monitor command cannot be empty.")?;
        let description = input["description"]
            .as_str()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .ok_or("Monitor description cannot be empty.")?;

        let req = ShellRequest {
            owner_session_id: None,
            invocation: cc_agent::tools::ToolInvocationContext::current(),
            command: command.to_string(),
            original_command: command.to_string(),
            description: Some(description.to_string()),
            shell: ShellDialect::Bash,
            // 与 Bash 一样不能复用构建时的快照：同轮 worktree 切换也要跟随。
            cwd: cc_agent::session_cwd::current_or(self.session_cwd.as_ref(), &self.cwd),
            // 监控进程自身很快返回 handle；真正的长期运行在后台通道里。
            timeout_ms: MONITOR_EXECUTION_TIMEOUT_MS,
            execution_timeout_ms: MONITOR_EXECUTION_TIMEOUT_MS,
            run_in_background: true,
        };

        let handle = self.executor.execute(req).await?;

        Ok(format!(
            "<monitor-started><task-id>{}</task-id><command>{}</command><output>{}</output></monitor-started>\n\
             Monitor started (task {}). Output file: {}. \
             You will be notified when it exits; use Read on the output file to check progress.",
            handle.task_id,
            command,
            handle.output_path.display(),
            handle.task_id,
            handle.output_path.display()
        ))
    }
}
