use rust_create_agent::agent::BackgroundTaskResult;
use std::collections::HashMap;

/// 后台任务状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
}

/// 后台任务信息（注册表条目）
pub struct BackgroundTask {
    pub id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub status: BackgroundTaskStatus,
    pub started_at: std::time::Instant,
    pub abort_handle: tokio::task::JoinHandle<()>,
}

/// 后台任务注册中心
pub struct BackgroundTaskRegistry {
    tasks: parking_lot::Mutex<HashMap<String, BackgroundTask>>,
    notification_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>,
    max_concurrent: usize,
}

impl BackgroundTaskRegistry {
    pub fn new(notification_tx: tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>) -> Self {
        Self {
            tasks: parking_lot::Mutex::new(HashMap::new()),
            notification_tx,
            max_concurrent: 3,
        }
    }

    /// 当前运行中的任务数
    pub fn active_count(&self) -> usize {
        self.tasks
            .lock()
            .values()
            .filter(|t| matches!(t.status, BackgroundTaskStatus::Running))
            .count()
    }

    /// 注册新任务，超出上限返回 Err
    pub fn register(&self, task: BackgroundTask) -> Result<(), String> {
        if self.active_count() >= self.max_concurrent {
            return Err(format!(
                "Maximum {} concurrent background tasks reached",
                self.max_concurrent
            ));
        }
        self.tasks.lock().insert(task.id.clone(), task);
        Ok(())
    }

    /// 任务完成时调用：更新状态 + 推送通知
    pub fn complete(&self, task_id: &str, result: BackgroundTaskResult) {
        if let Some(task) = self.tasks.lock().get_mut(task_id) {
            task.status = if result.success {
                BackgroundTaskStatus::Completed
            } else {
                BackgroundTaskStatus::Failed
            };
        }
        let _ = self.notification_tx.send(result);
    }

    /// 获取所有任务状态（UI 使用）
    pub fn list_tasks(&self) -> Vec<(String, BackgroundTaskStatus, String)> {
        self.tasks
            .lock()
            .values()
            .map(|t| (t.id.clone(), t.status.clone(), t.prompt_summary.clone()))
            .collect()
    }

    /// 取消指定任务
    pub fn cancel(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.lock();
        if let Some(task) = tasks.remove(task_id) {
            task.abort_handle.abort();
            Ok(())
        } else {
            Err(format!("Task {} not found", task_id))
        }
    }

    /// 清理已完成的任务
    pub fn cleanup_completed(&self) {
        self.tasks
            .lock()
            .retain(|_, t| matches!(t.status, BackgroundTaskStatus::Running));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> (
        BackgroundTaskRegistry,
        tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (BackgroundTaskRegistry::new(tx), rx)
    }

    fn make_task(id: &str) -> BackgroundTask {
        BackgroundTask {
            id: id.to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "test task".to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: std::time::Instant::now(),
            abort_handle: tokio::runtime::Handle::current().spawn(async {}),
        }
    }

    #[tokio::test]
    async fn test_register_and_active_count() {
        let (registry, _rx) = make_registry();
        assert_eq!(registry.active_count(), 0);

        registry.register(make_task("bg-1")).unwrap();
        assert_eq!(registry.active_count(), 1);
    }

    #[tokio::test]
    async fn test_max_concurrent_limit() {
        let (registry, _rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        registry.register(make_task("bg-2")).unwrap();
        registry.register(make_task("bg-3")).unwrap();

        let result = registry.register(make_task("bg-4"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum 3"));
    }

    #[tokio::test]
    async fn test_complete_sends_notification() {
        let (registry, mut rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        assert_eq!(registry.active_count(), 1);

        let result = BackgroundTaskResult {
            task_id: "bg-1".to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "test".to_string(),
            success: true,
            output: "done".to_string(),
            tool_calls_count: 2,
            duration_ms: 100,
        };

        registry.complete("bg-1", result);

        // 任务状态应变为 Completed
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert!(matches!(tasks[0].1, BackgroundTaskStatus::Completed));
        assert_eq!(registry.active_count(), 0);

        // 通知应已发送
        let received = rx.try_recv().unwrap();
        assert_eq!(received.task_id, "bg-1");
        assert!(received.success);
    }

    #[tokio::test]
    async fn test_cancel_removes_task() {
        let (registry, _rx) = make_registry();

        registry.register(make_task("bg-1")).unwrap();
        registry.register(make_task("bg-2")).unwrap();

        registry.cancel("bg-1").unwrap();
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0, "bg-2");

        // 取消不存在的任务返回 Err
        let result = registry.cancel("nonexistent");
        assert!(result.is_err());
    }
}
