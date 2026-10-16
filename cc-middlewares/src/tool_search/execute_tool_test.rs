    struct MockTool {
        name_str: String,
        desc_str: String,
        should_fail: bool,
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
                should_fail: false,
            }
        }

        fn new_failing(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl BaseTool for MockTool {
        fn name(&self) -> &str {
            &self.name_str
        }
        fn description(&self) -> &str {
            &self.desc_str
        }
        fn parameters(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }
        async fn invoke(
            &self,
            _input: Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            if self.should_fail {
                Err("mock tool error".into())
            } else {
                Ok(format!("{} executed", self.name_str))
            }
        }
    }

    fn make_test_registry() -> Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>> {
        let mut map = HashMap::new();
        map.insert(
            "CronRegister".to_string(),
            Arc::new(MockTool::new("CronRegister", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        map.insert(
            "mcp__slack__send_message".to_string(),
            Arc::new(MockTool::new(
                "mcp__slack__send_message",
                "Send Slack message",
            )) as Arc<dyn BaseTool>,
        );
        map.insert(
            "FailingTool".to_string(),
            Arc::new(MockTool::new_failing(
                "FailingTool",
                "A tool that always fails",
            )) as Arc<dyn BaseTool>,
        );
        Arc::new(RwLock::new(map))
    }

    #[test]
    fn test_tool_name_is_execute_extra_tool() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        assert_eq!(tool.name(), "ExecuteExtraTool");
    }

    #[test]
    fn test_parameters_schema() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["tool_name"].is_object());
        assert!(params["properties"]["params"].is_object());
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("tool_name")));
        assert!(required.contains(&json!("params")));
    }

    #[tokio::test]
    async fn test_invoke_executes_deferred_tool() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        let result = tool
            .invoke(json!({"tool_name": "CronRegister", "params": {"expression": "* * * * *", "prompt": "test"}}))
            .await
            .unwrap();
        assert_eq!(result, "CronRegister executed");
    }

    #[tokio::test]
    async fn test_invoke_content_preserves_text_result() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let result = tool
            .invoke_content(json!({"tool_name": "CronRegister", "params": {}}))
            .await
            .expect("注册表存活时应正常转发调用");
        assert_eq!(
            result.output, "CronRegister executed",
            "保留目标工具文本结果"
        );
        assert!(result.content.is_none(), "保留原有纯文本包装行为");
    }

    #[test]
    fn test_registry_releases_tools_with_registered_executor() {
        let registry = make_test_registry();
        let registry_weak = Arc::downgrade(&registry);
        let target_weak = Arc::downgrade(registry.read().get("CronRegister").expect("构造目标工具"));
        let executor: Arc<dyn BaseTool> = Arc::new(ExecuteExtraTool::new(Arc::clone(&registry)));
        let executor_weak = Arc::downgrade(&executor);
        registry
            .write()
            .insert(executor.name().to_string(), executor);
        drop(registry);
        assert!(
            registry_weak.upgrade().is_none(),
            "注册表不得被表内工具循环持有"
        );
        assert!(
            target_weak.upgrade().is_none(),
            "注册表销毁时应释放目标工具"
        );
        assert!(
            executor_weak.upgrade().is_none(),
            "注册表销毁时应释放代理工具"
        );
    }

    #[tokio::test]
    async fn test_expired_registry_returns_error() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        drop(registry);
        let error = tool
            .invoke(json!({"tool_name": "CronRegister", "params": {}}))
            .await
            .expect_err("注册表销毁后应明确报错");
        assert!(
            error
                .to_string()
                .contains("shared tool registry is no longer available"),
            "错误应说明注册表已销毁"
        );
    }

    #[tokio::test]
    async fn test_invoke_content_with_expired_registry_returns_error() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        drop(registry);
        let error = tool
            .invoke_content(json!({"tool_name": "CronRegister", "params": {}}))
            .await
            .expect_err("结构化调用也应传播注册表已销毁的错误");
        assert!(
            error
                .to_string()
                .contains("shared tool registry is no longer available"),
            "错误应说明注册表已销毁"
        );
    }

    struct MockPendingTool {
        started: Arc<tokio::sync::Notify>,
        resume: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl BaseTool for MockPendingTool {
        fn name(&self) -> &str {
            "PendingTool"
        }
        fn description(&self) -> &str {
            "等待测试继续执行"
        }
        fn parameters(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }
        async fn invoke(
            &self,
            _input: Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            self.started.notify_one();
            self.resume.notified().await;
            Ok("completed".to_string())
        }
    }

    #[tokio::test]
    async fn test_pending_invocation_releases_registry_and_keeps_target_alive() {
        let started = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let target: Arc<dyn BaseTool> = Arc::new(MockPendingTool {
            started: Arc::clone(&started),
            resume: Arc::clone(&resume),
        });
        let target_weak = Arc::downgrade(&target);
        let registry = Arc::new(RwLock::new(HashMap::from([(
            target.name().to_string(),
            target,
        )])));
        let registry_weak = Arc::downgrade(&registry);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let invocation = tokio::spawn(async move {
            tool.invoke(json!({"tool_name": "PendingTool", "params": {}}))
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
            .await
            .expect("目标工具应开始执行");
        assert!(
            registry.try_write().is_some(),
            "等待工具结果时不得持有注册表读锁"
        );
        drop(registry);
        assert!(
            registry_weak.upgrade().is_none(),
            "等待工具结果时不得延长注册表生命周期"
        );
        assert!(
            target_weak.upgrade().is_some(),
            "已开始的调用应保留目标工具"
        );
        resume.notify_one();
        let result = invocation
            .await
            .expect("等待调用任务完成")
            .expect("工具调用成功");
        assert_eq!(result, "completed", "注册表销毁后已开始的调用仍可完成");
        assert!(target_weak.upgrade().is_none(), "调用结束后应释放目标工具");
    }

    #[tokio::test]
    async fn test_tool_not_found_returns_error() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        let result = tool
            .invoke(json!({"tool_name": "UnknownTool", "params": {}}))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not found or not registered as a deferred tool"));
    }

    #[tokio::test]
    async fn test_missing_tool_name() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        let result = tool.invoke(json!({"params": {}})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing required 'tool_name' parameter"));
    }

    #[tokio::test]
    async fn test_missing_params() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        let result = tool.invoke(json!({"tool_name": "CronRegister"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing required 'params' parameter"));
    }

    #[tokio::test]
    async fn test_target_tool_error_propagates() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        let result = tool
            .invoke(json!({"tool_name": "FailingTool", "params": {}}))
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "mock tool error");
    }

    #[tokio::test]
    async fn test_case_insensitive_match() {
        let registry = make_test_registry();
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        // "cronregister" 大小写不敏感匹配 "CronRegister"
        let result = tool
            .invoke(json!({"tool_name": "cronregister", "params": {}}))
            .await
            .unwrap();
        assert_eq!(result, "CronRegister executed");
    }

    #[tokio::test]
    async fn test_camelcase_to_snake_case_match() {
        // 注册表用 snake_case，LLM 用 CamelCase
        let mut map = HashMap::new();
        map.insert(
            "cron_register".to_string(),
            Arc::new(MockTool::new("cron_register", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        let registry = Arc::new(RwLock::new(map));
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        // "CronRegister" 规范化为 "cron_register"，匹配注册表
        let result = tool
            .invoke(json!({"tool_name": "CronRegister", "params": {}}))
            .await
            .unwrap();
        assert_eq!(result, "cron_register executed");
    }

    #[tokio::test]
    async fn test_prefix_match_cron_create() {
        // 模拟实际场景：注册表有 cron_register，LLM 用 CronCreate
        let mut map = HashMap::new();
        map.insert(
            "cron_register".to_string(),
            Arc::new(MockTool::new("cron_register", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        let registry = Arc::new(RwLock::new(map));
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        // "CronCreate" → normalize → "cron_create" → 首词 "cron" → 匹配 "cron_register"
        let result = tool
            .invoke(json!({"tool_name": "CronCreate", "params": {}}))
            .await
            .unwrap();
        assert_eq!(result, "cron_register executed");
    }

    #[tokio::test]
    async fn test_prefix_match_ambiguous_returns_error() {
        // 同前缀多个工具时，不猜测
        let mut map = HashMap::new();
        map.insert(
            "cron_register".to_string(),
            Arc::new(MockTool::new("cron_register", "Register")) as Arc<dyn BaseTool>,
        );
        map.insert(
            "cron_remove".to_string(),
            Arc::new(MockTool::new("cron_remove", "Remove")) as Arc<dyn BaseTool>,
        );
        let registry = Arc::new(RwLock::new(map));
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));

        // "CronXxx" → 首词 "cron"，但 cron_register 和 cron_remove 都匹配 → 歧义
        let result = tool
            .invoke(json!({"tool_name": "CronXxx", "params": {}}))
            .await;
        assert!(result.is_err());
    }
