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

    fn build_test_registry() -> Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>> {
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
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);
        assert_eq!(tool.name(), "ExecuteExtraTool");
    }

    #[test]
    fn test_parameters_schema() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);
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
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool
            .invoke(json!({"tool_name": "CronRegister", "params": {"expression": "* * * * *", "prompt": "test"}}))
            .await
            .unwrap();
        assert_eq!(result, "CronRegister executed");
    }

    #[tokio::test]
    async fn test_tool_not_found_returns_error() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

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
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool.invoke(json!({"params": {}})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing required 'tool_name' parameter"));
    }

    #[tokio::test]
    async fn test_missing_params() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool.invoke(json!({"tool_name": "CronRegister"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing required 'params' parameter"));
    }

    #[tokio::test]
    async fn test_target_tool_error_propagates() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

        let result = tool
            .invoke(json!({"tool_name": "FailingTool", "params": {}}))
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "mock tool error");
    }

    #[tokio::test]
    async fn test_case_insensitive_match() {
        let registry = build_test_registry();
        let tool = ExecuteExtraTool::new(registry);

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
        let tool = ExecuteExtraTool::new(registry);

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
        let tool = ExecuteExtraTool::new(registry);

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
        let tool = ExecuteExtraTool::new(registry);

        // "CronXxx" → 首词 "cron"，但 cron_register 和 cron_remove 都匹配 → 歧义
        let result = tool
            .invoke(json!({"tool_name": "CronXxx", "params": {}}))
            .await;
        assert!(result.is_err());
    }
