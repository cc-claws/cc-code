//! Sleep 工具：按秒等待，不占用 shell 进程。
//!
//! 对齐 CC 的 SleepTool（`packages/builtin-tools/src/tools/SleepTool`）。
//!
//! 与 `Bash(command="sleep 30")` 的关键差别：
//! - 不起子进程，不占 shell 池里的槽位，等待期间其它 shell 操作照常进行；
//! - 用户中断由调度器统一处理——`tool_dispatch` 在 `select!` 里与
//!   `cancel.cancelled()` 竞争，本工具的 future 被整块 drop，无需自行清理；
//! - 返回结构化结果，模型能知道自己究竟睡了多久、是否被打断。
//!
//! 这也是 Bash 层「阻塞 sleep 拦截」文案里推荐的替代路径之一。

use async_trait::async_trait;
use cc_agent::tools::BaseTool;
use serde_json::Value;
use std::time::{Duration, Instant};

/// 单次睡眠上限（秒）。超过这个量级的「等待」几乎都是轮询，
/// 更应该用 `run_in_background` + 完成通知，而不是让一轮对话空转。
pub const MAX_SLEEP_SECONDS: f64 = 600.0;

const SLEEP_DESCRIPTION: &str = r#"Wait for a specified duration without holding a shell process.

Usage:
- Required: duration_seconds. Example: Sleep({"duration_seconds": 30}).
- Prefer this over Bash({"command":"sleep 30"}): it does not occupy the shell pool, so other commands stay responsive.
- The user can interrupt the sleep at any time; the result reports how long you actually slept and whether it was interrupted.
- Each wake-up costs an API call, and the prompt cache expires after 5 minutes of inactivity. Balance the wait duration accordingly.
- Use this only for a genuine wait. If you are waiting for a command to finish, run that command with run_in_background: true and await its completion notification instead of polling."#;

pub struct SleepTool;

impl SleepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SleepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseTool for SleepTool {
    fn name(&self) -> &str {
        "Sleep"
    }

    fn description(&self) -> &str {
        SLEEP_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration_seconds": {
                    "type": "number",
                    "exclusiveMinimum": 0,
                    "maximum": MAX_SLEEP_SECONDS,
                    "description": "How long to sleep in seconds. Can be interrupted by the user at any time"
                }
            },
            "required": ["duration_seconds"]
        })
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let duration_seconds = input["duration_seconds"]
            .as_f64()
            .ok_or("Missing duration_seconds parameter")?;
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
            return Err("duration_seconds must be a positive number".into());
        }
        if duration_seconds > MAX_SLEEP_SECONDS {
            return Err(format!(
                "duration_seconds must not exceed {MAX_SLEEP_SECONDS} seconds; \
                 for longer waits use run_in_background: true and await the completion notification"
            )
            .into());
        }

        let started = Instant::now();
        // 用户中断由调度器的 cancel.cancelled() 竞争处理，这里的 future 会被直接 drop。
        tokio::time::sleep(Duration::from_secs_f64(duration_seconds)).await;
        let elapsed = started.elapsed().as_secs_f64();

        Ok(format!(
            "<sleep-result><slept_seconds>{elapsed:.1}</slept_seconds><interrupted>false</interrupted></sleep-result>\nSlept {elapsed:.1}s."
        ))
    }
}
