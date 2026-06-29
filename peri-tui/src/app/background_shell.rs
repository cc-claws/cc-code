//! 后台 Shell 任务状态模型
//!
//! 定义 [`BackgroundShell`] 状态机和 [`ShellStatus`]，支撑 Ctrl+B 后台 Shell
//! 机制。一个后台 Shell 任务的生命周期：
//! - `Running`：进程运行中，输出写入磁盘（[`BackgroundShell::output_path`]）
//! - `Completed`/`Failed`：进程退出（exit_code 0 / 非 0）
//! - `Killed`：用户在 BackgroundTasksPanel 按 `x` 中止
//!
//! stall watchdog 与完成通知在批次 C 追加。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

use super::AgentEvent;
use crate::shell_exec::CommandOutput;
use peri_agent::task_output::DiskOutput;

/// 后台 Shell 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellStatus {
    /// 进程运行中
    Running,
    /// 进程正常退出（exit_code == 0）
    Completed,
    /// 进程异常退出（exit_code != 0）
    Failed,
    /// 用户主动 kill
    Killed,
}

impl ShellStatus {
    /// badge 文本（供 UI 渲染，对齐效果图场景 3/4 的 running/completed/failed/killed）
    pub fn badge(&self) -> &'static str {
        match self {
            ShellStatus::Running => "running",
            ShellStatus::Completed => "completed",
            ShellStatus::Failed => "failed",
            ShellStatus::Killed => "killed",
        }
    }

    /// 进程是否已结束（非 Running）
    pub fn is_terminal(&self) -> bool {
        !matches!(self, ShellStatus::Running)
    }
}

/// 后台 Shell 任务：一个被后台化的 shell 命令的完整状态。
///
/// `result_rx` 是 oneshot::Receiver（进程退出时 resolve），不是 JoinHandle：
/// 进程执行由 [`crate::shell_exec::execute_shell_command_streaming`] 的内部
/// tokio task 持有，这里只接收退出结果。`abort_handle` 保留同一 task 的
/// AbortHandle，供 BackgroundTasksPanel 的 `x` 键 kill。
pub struct BackgroundShell {
    /// 任务 id（uuid7）
    pub id: String,
    /// 命令文本
    pub command: String,
    /// 工作目录
    pub cwd: PathBuf,
    /// 当前状态
    pub status: ShellStatus,
    /// 是否已后台化（true 表示输出写磁盘，false 表示还在前台输出到 UI）
    pub is_backgrounded: bool,
    /// 启动时间
    pub started_at: Instant,
    /// 结束时间（进程退出或 kill 时设置）
    pub ended_at: Option<Instant>,
    /// 退出码（进程退出时设置）
    pub exit_code: Option<i32>,
    /// 是否已通知 agent（避免重复通知）
    pub notified: bool,
    /// 磁盘输出文件路径
    pub output_path: PathBuf,
    /// 进程退出 result channel（进程退出时 resolve）
    pub result_rx: Option<oneshot::Receiver<Result<CommandOutput>>>,
    /// stall watchdog task handle（批次 C 启动）
    pub stall_watchdog: Option<tokio::task::JoinHandle<()>>,
    /// 进程 task 的 abort handle（kill 后台任务用，批次 D 的 `x` 键）
    pub abort_handle: Option<AbortHandle>,
}

impl BackgroundShell {
    /// 创建一个新的后台 Shell 任务（Running 状态，is_backgrounded = true）。
    pub fn new(
        id: String,
        command: String,
        cwd: PathBuf,
        output_path: PathBuf,
        result_rx: oneshot::Receiver<Result<CommandOutput>>,
        abort_handle: AbortHandle,
        started_at: Instant,
    ) -> Self {
        Self {
            id,
            command,
            cwd,
            status: ShellStatus::Running,
            is_backgrounded: true,
            started_at,
            ended_at: None,
            exit_code: None,
            notified: false,
            output_path,
            result_rx: Some(result_rx),
            stall_watchdog: None,
            abort_handle: Some(abort_handle),
        }
    }

    /// 已运行时长（结束时为 ended_at - started_at，运行中为 now - started_at）。
    pub fn elapsed(&self) -> std::time::Duration {
        match self.ended_at {
            Some(end) => end.saturating_duration_since(self.started_at),
            None => self.started_at.elapsed(),
        }
    }

    /// 标记任务结束。
    pub fn mark_ended(&mut self, status: ShellStatus, exit_code: Option<i32>) {
        self.status = status;
        self.exit_code = exit_code;
        self.ended_at = Some(Instant::now());
    }
}

// ── Stall Watchdog（Phase 4）─────────────────────────────────────────────────

/// Stall watchdog 检查间隔（生产 5 秒，测试 200ms 加速）
#[cfg(not(test))]
const STALL_CHECK_INTERVAL_MS: u64 = 5_000;
#[cfg(test)]
const STALL_CHECK_INTERVAL_MS: u64 = 200;

/// Stall 阈值：output 无增长超过此时长视为 stall（生产 45 秒，测试 500ms）
#[cfg(not(test))]
const STALL_THRESHOLD_MS: u64 = 45_000;
#[cfg(test)]
const STALL_THRESHOLD_MS: u64 = 500;

/// Stall 时读取输出末尾的字节数，用于 prompt pattern 匹配
const STALL_TAIL_BYTES: u64 = 1024;

/// Prompt 模式：末行包含任一则判定命令在等待用户输入。
///
/// 全小写英文 + 中文常见 prompt，匹配时对末行 to_lowercase（覆盖 "(Y/N)" 大写变体）。
/// 简化为字面量子串匹配（避免引入 regex 依赖）。
const PROMPT_PATTERNS: &[&str] = &[
    "(y/n)",
    "[y/n]",
    "(yes/no)",
    "do you",
    "would you",
    "shall i",
    "are you sure",
    "ready to",
    "press enter",
    "press any key",
    "continue?",
    "overwrite?",
    // 中文 prompt（确认/继续/是否/按键提示）
    "确认",
    "继续",
    "是否",
    "按回车",
    "按任意键",
];

/// 启动 stall watchdog task。
///
/// 每 [`STALL_CHECK_INTERVAL_MS`] 检查 `output_path` 文件大小：连续无增长达
/// [`STALL_THRESHOLD_MS`] 且末行匹配 [`PROMPT_PATTERNS`] 时，发送
/// [`AgentEvent::BackgroundShellStalled`] 通知（one-shot，触发后退出）。
///
/// 返回 JoinHandle，存入 [`BackgroundShell::stall_watchdog`]，任务结束时 abort。
pub fn spawn_stall_watchdog(
    task_id: String,
    command: String,
    output_path: PathBuf,
    bg_event_tx: tokio::sync::mpsc::Sender<AgentEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(STALL_CHECK_INTERVAL_MS));
        let mut last_size: u64 = 0;
        let mut stall_since: Option<Instant> = None;
        loop {
            interval.tick().await;
            let size = match tokio::fs::metadata(&output_path).await {
                Ok(m) => m.len(),
                Err(_) => continue,
            };
            if size > last_size {
                last_size = size;
                stall_since = None;
                continue;
            }
            // size 未增长
            let now = Instant::now();
            let Some(stalled_at) = stall_since else {
                stall_since = Some(now);
                continue;
            };
            if now.duration_since(stalled_at) < Duration::from_millis(STALL_THRESHOLD_MS) {
                continue;
            }
            // stall >= 阈值：tail + 匹配 prompt pattern
            let tail = match DiskOutput::read_tail(&output_path, STALL_TAIL_BYTES).await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let tail_str = String::from_utf8_lossy(&tail);
            let last_line = tail_str.lines().last().unwrap_or("");
            let last_line_lower = last_line.to_lowercase();
            if PROMPT_PATTERNS.iter().any(|p| last_line_lower.contains(p)) {
                let _ = bg_event_tx
                    .send(AgentEvent::BackgroundShellStalled {
                        task_id: task_id.clone(),
                        command: command.clone(),
                        last_output: last_line.to_string(),
                    })
                    .await;
                break; // one-shot：触发后退出
            }
        }
    })
}

// ── 完成通知生成（Phase 5）───────────────────────────────────────────────────

/// 生成后台 shell 完成通知消息文本（注入 agent 对话流，对齐效果图场景 5）。
///
/// agent 可通过 FileReadTool 读取 `output_path` 获取完整输出。
pub fn shell_completion_notification(
    id: &str,
    command: &str,
    exit_code: Option<i32>,
    output_path: &Path,
) -> String {
    let status_line = match exit_code {
        Some(0) => "completed (exit 0)".to_string(),
        Some(code) => format!("failed (exit {})", code),
        None => "terminated".to_string(),
    };
    // XML 结构化（对齐 TS <task-notification>）：agent 可精确提取 task-id/output 字段，
    // 避免 command 含换行时纯文本行解析错乱。<output> 是完整输出文件路径，
    // agent 用 Read 工具读取获取完整 stdout/stderr。
    format!(
        "<background-task-completed>\n<task-id>{}</task-id>\n<command>{}</command>\n<status>{}</status>\n<output>{}</output>\n</background-task-completed>",
        id,
        xml_escape(command),
        status_line,
        xml_escape(&output_path.display().to_string())
    )
}

/// 生成后台 shell stall（等待输入）警告通知文本（注入 agent 对话流，对齐效果图场景 6）。
///
/// watchdog 检测到命令无输出且末行匹配 prompt pattern 时调用。
pub fn shell_stalled_notification(task_id: &str, command: &str, last_output: &str) -> String {
    format!(
        "<background-task-waiting-for-input>\n<task-id>{}</task-id>\n<command>{}</command>\n<last-output>{}</last-output>\n<suggestion>provide input via stdin or kill the task</suggestion>\n</background-task-waiting-for-input>",
        task_id,
        xml_escape(command),
        xml_escape(last_output)
    )
}

/// 将后台 shell 控制通知转换为 TUI 可读的一行提示。
///
/// 原始 XML 仍会发送给 agent；这里仅用于聊天区展示，避免内部标签泄露给用户。
pub fn shell_notification_display_text(raw: &str) -> Option<String> {
    let notification = unwrap_system_reminder(raw.trim());
    if notification.starts_with("<background-task-completed>") {
        let command = extract_xml_tag(notification, "command")
            .map(xml_unescape)
            .unwrap_or_else(|| "shell command".to_string());
        let status = extract_xml_tag(notification, "status")
            .map(xml_unescape)
            .unwrap_or_else(|| "completed".to_string());
        let verb = if status.starts_with("failed") {
            "后台 shell 失败"
        } else if status == "terminated" {
            "后台 shell 已终止"
        } else {
            "后台 shell 已完成"
        };
        return Some(format!(
            "{}: {} ({})",
            verb,
            truncate_chars(&command, 80),
            status
        ));
    }

    if notification.starts_with("<background-task-waiting-for-input>") {
        let command = extract_xml_tag(notification, "command")
            .map(xml_unescape)
            .unwrap_or_else(|| "shell command".to_string());
        return Some(format!(
            "后台 shell 等待输入: {}",
            truncate_chars(&command, 80)
        ));
    }

    None
}

fn unwrap_system_reminder(raw: &str) -> &str {
    raw.strip_prefix("<system-reminder>")
        .and_then(|s| s.strip_suffix("</system-reminder>"))
        .map(str::trim)
        .unwrap_or(raw)
}

fn extract_xml_tag<'a>(raw: &'a str, tag: &str) -> Option<&'a str> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let start = raw.find(&start_tag)? + start_tag.len();
    let end = raw[start..].find(&end_tag)? + start;
    Some(&raw[start..end])
}

/// XML 转义（command/last_output 可能含 `<` `>` `&`，如 shell 重定向、错误信息）。
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut result: String = s.chars().take(max_chars).collect();
    result.push_str("...");
    result
}

#[cfg(test)]
#[path = "background_shell_test.rs"]
mod tests;
