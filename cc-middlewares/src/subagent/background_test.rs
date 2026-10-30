    fn make_registry() -> (
        BackgroundTaskRegistry,
        tokio::sync::mpsc::UnboundedReceiver<BackgroundTaskResult>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (BackgroundTaskRegistry::new(tx), rx)
    }

    fn make_snapshot(id: &str, status: BackgroundTaskStatus) -> BackgroundTaskSnapshot {
        BackgroundTaskSnapshot {
            task_id: id.to_string(),
            instance_id: format!("{id}-inst"),
            agent_name: "test-agent".to_string(),
            task_preview: "preview".to_string(),
            status,
            elapsed_ms: 0,
            total_steps: 0,
            tokens_used: 0,
            current_activity: String::new(),
            model: None,
            timeout_secs: None,
            result: None,
        }
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
            child_thread_id: None,
        };

        registry.complete("bg-1", result);

        // 已完成任务应被立即清理，list_tasks 不再返回
        let tasks = registry.list_tasks();
        assert_eq!(tasks.len(), 0, "completed tasks should be cleaned up immediately");
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

    /// 回归：会话级 registry 的终态 runtime 必须有界，长会话不得累积完整输出。
    #[tokio::test]
    async fn test_terminal_runtimes_are_pruned_and_active_kept() {
        let (registry, _rx) = make_registry();
        let active = "bg-active";

        // 先挂一个仍在运行的任务（模拟在途完成通知），再连续完成 40 个任务
        for index in 0..40 {
            let id = format!("bg-{index}");
            registry
                .attach_runtime(make_snapshot(&id, BackgroundTaskStatus::Running), AgentCancellationToken::new(), SteeringQueue::default());
            registry.complete(
                &id,
                BackgroundTaskResult {
                    task_id: id.clone(),
                    agent_name: "test-agent".into(),
                    prompt_summary: "preview".into(),
                    success: true,
                    output: "x".repeat(4096),
                    tool_calls_count: 0,
                    duration_ms: 1,
                    child_thread_id: None,
                },
            );
        }
        registry.attach_runtime(
            make_snapshot(active, BackgroundTaskStatus::Running),
            AgentCancellationToken::new(),
            SteeringQueue::default(),
        );

        let runtimes = registry.runtimes.lock();
        let terminal = runtimes
            .values()
            .filter(|r| !matches!(r.snapshot.status, BackgroundTaskStatus::Running | BackgroundTaskStatus::AwaitingApproval))
            .count();
        assert!(
            terminal <= 8,
            "终态 runtime 应被裁剪到上限（实际 {terminal}，共 {}）",
            runtimes.len()
        );
        assert!(
            runtimes.contains_key(active),
            "活跃任务的 runtime 不得被裁剪"
        );
    }

    /// 回归 #829：停止态（cancel 已触发）的任务 complete 后不应发送通知，
    /// 避免 drain_notifications 将内部文本以 Human 消息泄漏到主时间线。
    #[tokio::test]
    async fn test_stopped_task_complete_does_not_send_notification() {
        let (registry, mut rx) = make_registry();
        let task_id = "bg-stop";

        registry.register(make_task(task_id)).unwrap();
        let cancel = AgentCancellationToken::new();
        cancel.cancel(); // 模拟用户停止
        registry.attach_runtime(
            make_snapshot(task_id, BackgroundTaskStatus::Running),
            cancel,
            SteeringQueue::default(),
        );

        let result = BackgroundTaskResult {
            task_id: task_id.to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "stopped task".to_string(),
            success: false,
            output: "Interrupted by user".to_string(),
            tool_calls_count: 0,
            duration_ms: 50,
            child_thread_id: None,
        };
        registry.complete(task_id, result);

        // 快照应标记为 Stopped
        let snapshot = registry.status(task_id).unwrap();
        assert_eq!(snapshot.status, BackgroundTaskStatus::Stopped, "停止态快照应为 Stopped");

        // 通知通道应为空——停止态不发通知
        assert!(
            rx.try_recv().is_err(),
            "停止态的任务不应发送 notification（#829）"
        );
    }

    /// 对照组：正常完成的任务仍应发送通知。
    #[tokio::test]
    async fn test_completed_task_still_sends_notification() {
        let (registry, mut rx) = make_registry();
        let task_id = "bg-ok";

        registry.register(make_task(task_id)).unwrap();
        registry.attach_runtime(
            make_snapshot(task_id, BackgroundTaskStatus::Running),
            AgentCancellationToken::new(),
            SteeringQueue::default(),
        );

        let result = BackgroundTaskResult {
            task_id: task_id.to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "ok task".to_string(),
            success: true,
            output: "done".to_string(),
            tool_calls_count: 1,
            duration_ms: 100,
            child_thread_id: None,
        };
        registry.complete(task_id, result);

        let received = rx.try_recv().expect("正常完成的任务应发送 notification");
        assert!(received.success);
    }

    /// 对照组：超时任务（cancel 已触发但 output 含 "timed out"）不算停止，应发通知。
    #[tokio::test]
    async fn test_timed_out_task_sends_notification_even_when_cancelled() {
        let (registry, mut rx) = make_registry();
        let task_id = "bg-timeout";

        registry.register(make_task(task_id)).unwrap();
        let cancel = AgentCancellationToken::new();
        cancel.cancel();
        registry.attach_runtime(
            make_snapshot(task_id, BackgroundTaskStatus::Running),
            cancel,
            SteeringQueue::default(),
        );

        let result = BackgroundTaskResult {
            task_id: task_id.to_string(),
            agent_name: "test-agent".to_string(),
            prompt_summary: "timeout task".to_string(),
            success: false,
            output: "Agent timed out after 60s".to_string(),
            tool_calls_count: 3,
            duration_ms: 60000,
            child_thread_id: None,
        };
        registry.complete(task_id, result);

        // 超时不是停止，应标记为 Failed 且发送通知
        let snapshot = registry.status(task_id).unwrap();
        assert_eq!(snapshot.status, BackgroundTaskStatus::Failed, "超时应标记为 Failed 而非 Stopped");
        let received = rx.try_recv().expect("超时任务应发送 notification");
        assert!(!received.success);
    }
