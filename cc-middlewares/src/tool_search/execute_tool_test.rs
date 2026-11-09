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
            json!({"type": "object", "properties": {}, "additionalProperties": true})
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

    struct SchemaCheckedTool {
        name: &'static str,
        calls: Arc<std::sync::atomic::AtomicUsize>,
        fail_execution: bool,
        schema: Value,
    }

    #[async_trait]
    impl BaseTool for SchemaCheckedTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "A deferred tool with required and nested parameters"
        }

        fn parameters(&self) -> Value {
            self.schema.clone()
        }

        async fn invoke(
            &self,
            _input: Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.fail_execution {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "target permission denied",
                )
                .into());
            }
            Ok("target executed".to_string())
        }
    }

    fn schema_checked_registry(
        name: &'static str,
        fail_execution: bool,
    ) -> (
        Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        schema_checked_registry_with_schema(
            name,
            fail_execution,
            json!({
                "type": "object",
                "properties": {
                    "file_path": {"type": "string"},
                    "options": {
                        "type": "object",
                        "properties": {"limit": {"type": "integer"}},
                        "required": ["limit"]
                    }
                },
                "required": ["file_path"],
                "additionalProperties": false
            }),
        )
    }

    fn schema_checked_registry_with_schema(
        name: &'static str,
        fail_execution: bool,
        schema: Value,
    ) -> (
        Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let target: Arc<dyn BaseTool> = Arc::new(SchemaCheckedTool {
            name,
            calls: Arc::clone(&calls),
            fail_execution,
            schema,
        });
        let registry = Arc::new(RwLock::new(HashMap::from([(name.to_string(), target)])));
        (registry, calls)
    }

    #[tokio::test]
    async fn test_invalid_target_arguments_never_execute_and_preserve_schema_error() {
        let (registry, calls) = schema_checked_registry("StrictDeferredTool", false);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        for (params, expected_error) in [
            (json!({}), "required parameter 'file_path' is missing"),
            (
                json!({"filePath": "a.rs"}),
                "Unexpected parameter 'filePath'",
            ),
            (
                json!({"file_path": 17}),
                "parameter 'file_path' type is expected as string",
            ),
            (
                json!({"file_path": "a.rs", "options": {}}),
                "Invalid nested arguments at 'options'",
            ),
            (
                json!({"file_path": "a.rs", "options": {"limit": "many"}}),
                "parameter 'limit' type is expected as integer",
            ),
            (json!("a.rs"), "expected an object for arguments"),
        ] {
            // 模糊解析成功后，诊断与发现示例必须使用目标真实名称。
            let error = tool
                .invoke(json!({"tool_name": "strictdeferredtool", "params": params}))
                .await
                .expect_err("目标参数不合法时应在调用前返回错误");
            let AgentError::ToolSchemaValidationFailed { tool, reason } = error
                .downcast_ref::<AgentError>()
                .expect("必须保留结构化参数错误")
            else {
                panic!("预期目标 schema 错误，实际: {error}");
            };
            assert_eq!(tool, "StrictDeferredTool");
            assert!(reason.contains(expected_error), "{reason}");
            assert!(reason.contains("Received keys:"), "{reason}");
            assert!(reason.contains("If you have not obtained the target schema"));
            assert!(reason.contains("select:StrictDeferredTool"));
            assert!(reason.contains("ExecuteExtraTool("));
            assert!(!reason.contains("camelCase"));
            assert_eq!(
                calls.load(std::sync::atomic::Ordering::SeqCst),
                0,
                "参数错误不得触发目标工具副作用"
            );
        }
    }

    #[tokio::test]
    async fn test_valid_target_arguments_execute_without_discovery() {
        let (registry, calls) = schema_checked_registry("StrictDeferredTool", false);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let output = tool
            .invoke(json!({
                "tool_name": "StrictDeferredTool",
                "params": {"file_path": "a.rs", "options": {"limit": 3}}
            }))
            .await
            .expect("正确参数不应被强制搜索拦截");
        assert_eq!(output, "target executed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_non_search_kernels_do_not_receive_unavailable_discovery_hints() {
        let (registry, calls) = schema_checked_registry("StrictDeferredTool", false);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        for id in ["dsh", "pi"] {
            let invocation = ToolInvocationContext {
                tool_call_id: format!("invalid-{id}"),
                source_agent_id: None,
            };
            let error = invocation
                .scope_with_kernel(
                    cc_agent::kernel::Kernel::try_from_id(id).unwrap(),
                    tool.invoke(json!({"tool_name": "StrictDeferredTool", "params": {}})),
                )
                .await
                .expect_err("非法目标参数仍应拒绝执行");
            let text = error.to_string();
            assert!(text.contains("file_path"), "{text}");
            assert!(!text.contains("SearchExtraTools"), "{id}: {text}");
            assert!(!text.contains("ExecuteExtraTool"), "{id}: {text}");
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_non_discoverable_target_errors_do_not_suggest_deferred_discovery() {
        for name in ["Read", "AgentResult"] {
            let (registry, calls) = schema_checked_registry(name, false);
            let tool = ExecuteExtraTool::new(Arc::clone(&registry));
            let error = tool
                .invoke(json!({"tool_name": name, "params": {}}))
                .await
                .expect_err("不可发现的目标仍应校验参数");
            assert!(!error.to_string().contains("SearchExtraTools"), "{name}: {error}");
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn test_deferred_schema_allows_dynamic_keys_without_additional_properties() {
        let (registry, calls) = schema_checked_registry_with_schema(
            "DynamicDeferredTool",
            false,
            json!({
                "type": "object",
                "properties": {"file_path": {"type": "string"}},
                "required": ["file_path"]
            }),
        );
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let output = tool
            .invoke(json!({
                "tool_name": "DynamicDeferredTool",
                "params": {"file_path": "a.rs", "plugin_option": {"enabled": true}}
            }))
            .await
            .expect("外部 schema 未禁止的动态参数必须允许执行");
        assert_eq!(output, "target executed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_deferred_schema_accepts_required_nullable_anyof_value() {
        let (registry, calls) = schema_checked_registry_with_schema(
            "NullableDeferredTool",
            false,
            json!({
                "type": "object",
                "properties": {
                    "file_path": {"anyOf": [{"type": "string"}, {"type": "null"}]}
                },
                "required": ["file_path"]
            }),
        );
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let output = tool
            .invoke(json!({
                "tool_name": "NullableDeferredTool",
                "params": {"file_path": null}
            }))
            .await
            .expect("必填但允许 null 的外部参数不能被误拒绝");
        assert_eq!(output, "target executed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_target_execution_error_preserves_type_without_schema_guidance() {
        let (registry, calls) = schema_checked_registry("StrictDeferredTool", true);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let error = tool
            .invoke(json!({
                "tool_name": "StrictDeferredTool",
                "params": {"file_path": "a.rs"}
            }))
            .await
            .expect_err("目标执行错误仍应原样返回");
        assert_eq!(error.to_string(), "target permission denied");
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_discovery_examples_escape_target_names_as_json() {
        let name = "mcp__server__quoted\"name";
        let (registry, _) = schema_checked_registry(name, false);
        let tool = ExecuteExtraTool::new(Arc::clone(&registry));
        let error = tool
            .invoke(json!({"tool_name": name, "params": {}}))
            .await
            .expect_err("缺失目标参数应返回发现指引");
        let text = error.to_string();
        let search = text
            .lines()
            .find_map(|line| line.strip_prefix("1. SearchExtraTools("))
            .unwrap()
            .strip_suffix(')')
            .unwrap();
        let search: Value = serde_json::from_str(search).expect("搜索示例必须是合法 JSON");
        assert_eq!(search["query"], format!("select:{name}"));
        let execute = text
            .lines()
            .find_map(|line| line.strip_prefix("2. ExecuteExtraTool("))
            .unwrap()
            .split_once("), replacing params")
            .unwrap()
            .0;
        let execute: Value = serde_json::from_str(execute).expect("执行示例必须是合法 JSON");
        assert_eq!(execute["tool_name"], name);
    }
