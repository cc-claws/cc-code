use cc_agent::agent::{events::AgentEvent, steering::SteeringQueue, AgentCancellationToken, BackgroundTaskResult};
use std::collections::HashMap;
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackgroundTaskStatus { Running, AwaitingApproval, Completed, Stopped, Failed }

pub struct BackgroundTask {
    pub id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub status: BackgroundTaskStatus,
    pub started_at: std::time::Instant,
    pub abort_handle: tokio::task::JoinHandle<()>,
}

/// 可跨 prompt 查询的任务快照；线程 ID 和工具返回的 task ID 均可用于管理。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackgroundTaskSnapshot {
    pub task_id: String,
    pub instance_id: String,
    pub agent_name: String,
    pub task_preview: String,
    pub status: BackgroundTaskStatus,
    pub elapsed_ms: u64,
    pub total_steps: usize,
    pub tokens_used: u64,
    pub current_activity: String,
    pub model: Option<String>,
    pub timeout_secs: Option<u64>,
    pub result: Option<String>,
}

struct BackgroundRuntime {
    snapshot: BackgroundTaskSnapshot,
    cancel: AgentCancellationToken,
    mailbox: SteeringQueue,
    started_at: std::time::Instant,
}

/// 会话级注册表。终态保留用于查询；只有运行项占用并发槽位。
pub struct BackgroundTaskRegistry {
    tasks: parking_lot::Mutex<HashMap<String, BackgroundTask>>,
    runtimes: parking_lot::Mutex<HashMap<String, BackgroundRuntime>>,
    notification_tx: parking_lot::Mutex<tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>>,
    max_concurrent: usize,
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        Self::new(tx)
    }
}

/// 终态 runtime 保留上限（会话级 registry 的跨轮累积边界）。
const TERMINAL_RUNTIME_KEEP: usize = 8;

impl BackgroundTaskRegistry {
    pub fn new(notification_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>) -> Self {
        Self {
            tasks: parking_lot::Mutex::new(HashMap::new()),
            runtimes: parking_lot::Mutex::new(HashMap::new()),
            notification_tx: parking_lot::Mutex::new(notification_tx),
            max_concurrent: 3,
        }
    }

    pub fn set_notification_sender(&self, sender: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>) {
        *self.notification_tx.lock() = sender;
    }

    pub fn active_count(&self) -> usize { self.tasks.lock().len() }

    pub fn register(&self, task: BackgroundTask) -> Result<(), String> {
        let mut tasks = self.tasks.lock();
        if tasks.len() >= self.max_concurrent {
            // 调用方以启动闸门保证未注册任务尚未执行任何操作。
            task.abort_handle.abort();
            return Err(format!("Maximum {} concurrent background tasks reached", self.max_concurrent));
        }
        tasks.insert(task.id.clone(), task);
        Ok(())
    }

    pub(crate) fn attach_runtime(&self, snapshot: BackgroundTaskSnapshot, cancel: AgentCancellationToken, mailbox: SteeringQueue) {
        let mut runtimes = self.runtimes.lock();
        runtimes.insert(snapshot.task_id.clone(), BackgroundRuntime {
            snapshot, cancel, mailbox, started_at: std::time::Instant::now(),
        });
        Self::prune_terminal_runtimes(&mut runtimes, TERMINAL_RUNTIME_KEEP);
    }

    /// 终态 runtime 只保留最近 `keep` 条。
    ///
    /// registry 自本 PR 起是**会话级共享**（不再是每轮重建），而 `complete()` 只改快照、
    /// 不删条目，且每条都持有完整 `result` 文本——不设界的话长会话会累积几十份完整输出。
    /// 活跃任务（Running/AwaitingApproval）永不淘汰，保证在途完成通知仍能落到 runtime 上。
    fn prune_terminal_runtimes(runtimes: &mut HashMap<String, BackgroundRuntime>, keep: usize) {
        let is_terminal = |r: &BackgroundRuntime| {
            !matches!(r.snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval)
        };
        let terminal_count = runtimes.values().filter(|r| is_terminal(r)).count();
        if terminal_count <= keep {
            return;
        }
        let mut finished: Vec<(std::time::Instant, String)> = runtimes
            .iter()
            .filter(|(_, r)| is_terminal(r))
            .map(|(id, r)| (r.started_at, id.clone()))
            .collect();
        finished.sort_by_key(|(started_at, _)| *started_at);
        for (_, id) in finished.into_iter().take(terminal_count - keep) {
            runtimes.remove(&id);
        }
    }

    pub fn status(&self, id: &str) -> Result<BackgroundTaskSnapshot, String> {
        let runtimes = self.runtimes.lock();
        let runtime = runtimes.values().find(|r| r.snapshot.task_id == id || r.snapshot.instance_id == id)
            .ok_or_else(|| format!("Task {id} not found"))?;
        let mut snapshot = runtime.snapshot.clone();
        if matches!(snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval) {
            snapshot.elapsed_ms = runtime.started_at.elapsed().as_millis() as u64;
        }
        Ok(snapshot)
    }

    pub(crate) fn record_event(&self, id: &str, event: &AgentEvent) {
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(id) else { return; };
        match event {
            AgentEvent::ToolStart { name, .. } => {
                runtime.snapshot.total_steps += 1;
                runtime.snapshot.current_activity = name.clone();
            }
            AgentEvent::LlmCallEnd { model, usage, .. } => {
                runtime.snapshot.model = Some(model.clone());
                if let Some(usage) = usage {
                    runtime.snapshot.tokens_used += u64::from(usage.input_tokens) + u64::from(usage.output_tokens);
                }
            }
            _ => {}
        }
    }

    pub fn complete(&self, task_id: &str, result: BackgroundTaskResult) {
        if let Some(runtime) = self.runtimes.lock().get_mut(task_id) {
            runtime.snapshot.status = if runtime.cancel.is_cancelled() && !result.output.contains("timed out") {
                BackgroundTaskStatus::Stopped
            } else if result.success { BackgroundTaskStatus::Completed } else { BackgroundTaskStatus::Failed };
            runtime.snapshot.elapsed_ms = result.duration_ms;
            runtime.snapshot.result = Some(result.output.clone());
            runtime.mailbox.close();
        }
        self.tasks.lock().remove(task_id);
        if self.notification_tx.lock().send(result).is_err() {
            warn!(task_id, "background task complete: notification channel closed");
        }
    }

    pub fn list_tasks(&self) -> Vec<(String, BackgroundTaskStatus, String)> {
        self.tasks.lock().values().map(|t| (t.id.clone(), t.status.clone(), t.prompt_summary.clone())).collect()
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        {
            let runtimes = self.runtimes.lock();
            if let Some(runtime) = runtimes.values().find(|r| r.snapshot.task_id == id || r.snapshot.instance_id == id) {
                if !matches!(runtime.snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval) {
                    return Err(format!("Task {id} ended (read-only)"));
                }
                // 不 abort 任务：让执行器取消 future 并正常发出终态、完成持久化和清理。
                runtime.cancel.cancel();
                return Ok(());
            }
        }
        if let Some(task) = self.tasks.lock().remove(id) {
            task.abort_handle.abort();
            Ok(())
        } else { Err(format!("Task {id} not found")) }
    }

    pub fn cancel_all(&self) {
        for runtime in self.runtimes.lock().values() { runtime.cancel.cancel(); }
    }

    /// 入队立即返回；执行器在工具结束后的安全边界接收，不打断正在运行的工具。
    pub fn send_message(&self, id: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() { return Err("Message must not be empty".into()); }
        let runtimes = self.runtimes.lock();
        let runtime = runtimes.values().find(|r| r.snapshot.task_id == id || r.snapshot.instance_id == id)
            .ok_or_else(|| format!("Task {id} not found"))?;
        if runtime.cancel.is_cancelled() || !matches!(runtime.snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval) {
            return Err(format!("Task {id} ended (read-only)"));
        }
        runtime.mailbox.enqueue(cc_agent::messages::MessageContent::text(text))
            .map(|_| ()).ok_or_else(|| format!("Task {id} ended (read-only)"))
    }

    pub fn cleanup_completed(&self) {
        self.runtimes.lock().retain(|_, r| matches!(r.snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("background_test.rs");
}
