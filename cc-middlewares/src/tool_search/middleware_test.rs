    struct MockTool {
        name_str: String,
        desc_str: String,
    }

    #[tokio::test]
    async fn test_before_agent_excludes_history_marker_from_discovery() {
        let (index, shared) = build_test_components();
        shared.write().insert(
            "AgentResult".into(),
            Arc::new(crate::subagent::AgentResultTool::new()),
        );
        let middleware = ToolSearchMiddleware::new(Arc::clone(&index), Arc::clone(&shared));
        let mut state = cc_agent::agent::state::AgentState::new("/tmp");
        middleware
            .before_agent(&mut state)
            .await
            .expect("生成工具发现提示词");
        let prompt = state.messages()[0].content();
        assert!(
            !prompt.contains("AgentResult"),
            "历史标记不得被推荐为可调用工具"
        );
        assert!(prompt.contains("CronRegister"), "正常延迟工具仍须可见");
        assert_eq!(index.total_count(), 2, "内部标记不计入可发现工具数");
        let search = SearchExtraTools::new(Arc::clone(&index));
        for query in ["select:AgentResult", "AgentResult", "background results"] {
            let output = search
                .invoke(serde_json::json!({"query": query}))
                .await
                .expect("搜索成功");
            let output: serde_json::Value =
                serde_json::from_str(&output).expect("搜索返回合法 JSON");
            assert!(
                output["results"]
                    .as_array()
                    .expect("结果数组")
                    .iter()
                    .all(|item| item["name"] != "AgentResult"),
                "精确与关键词搜索都不能发现内部标记"
            );
        }
        assert_eq!(
            index
                .search("select:CronRegister,mcp__slack__send", 5)
                .len(),
            2,
            "正常工具的精确查询不受影响"
        );
        let mut next_state = cc_agent::agent::state::AgentState::new("/tmp");
        middleware
            .before_agent(&mut next_state)
            .await
            .expect("下一轮复用提示词");
        assert_eq!(
            next_state.messages()[0].content(),
            prompt,
            "跨轮提示词必须稳定"
        );
        assert!(
            shared.read().contains_key("AgentResult"),
            "兼容标记仍在内部注册表中"
        );
        assert!(
            super::super::core_tools::is_deferred_tool("AgentResult"),
            "不能因移出发现索引而直接暴露给模型"
        );
    }

    #[tokio::test]
    async fn test_before_agent_with_only_history_marker_has_no_deferred_prompt() {
        let index = Arc::new(ToolSearchIndex::new());
        let shared: HashMap<String, Arc<dyn BaseTool>> = HashMap::from([(
            "AgentResult".into(),
            Arc::new(crate::subagent::AgentResultTool::new()) as Arc<dyn BaseTool>,
        )]);
        let middleware =
            ToolSearchMiddleware::new(Arc::clone(&index), Arc::new(RwLock::new(shared)));
        for _ in 0..2 {
            let mut state = cc_agent::agent::state::AgentState::new("/tmp");
            middleware
                .before_agent(&mut state)
                .await
                .expect("构建空工具索引");
            assert!(
                state.messages().is_empty(),
                "不能为内部标记注入可调用能力段落"
            );
            assert_eq!(index.total_count(), 0);
            assert!(index.cached_prompt().is_none());
        }
    }

    #[tokio::test]
    async fn test_history_marker_indirect_invocation_still_returns_error() {
        let (index, shared) = build_test_components();
        shared.write().insert(
            "AgentResult".into(),
            Arc::new(crate::subagent::AgentResultTool::new()),
        );
        let middleware = ToolSearchMiddleware::new(index, Arc::clone(&shared));
        let mut state = cc_agent::agent::state::AgentState::new("/tmp");
        middleware
            .before_agent(&mut state)
            .await
            .expect("生成工具发现提示词");
        let executor = ExecuteExtraTool::new(shared);
        let error = executor
            .invoke(serde_json::json!({
                "tool_name": "AgentResult",
                "params": {"task_id": "legacy-task"}
            }))
            .await
            .expect_err("旧会话误调用仍须明确报错");
        assert!(error.to_string().contains("Do not retry AgentResult."));
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .expect("保留原始错误类型")
                .kind(),
            std::io::ErrorKind::Unsupported
        );
    }

    impl MockTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name_str: name.to_string(),
                desc_str: desc.to_string(),
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
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("mock".to_string())
        }
    }

    fn build_test_components() -> (
        Arc<ToolSearchIndex>,
        Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>>,
    ) {
        let index = Arc::new(ToolSearchIndex::new());
        index.build(vec![
            Arc::new(MockTool::new("CronRegister", "Register a cron task")),
            Arc::new(MockTool::new("mcp__slack__send", "Send Slack message")),
        ]);

        let mut shared = HashMap::new();
        shared.insert(
            "CronRegister".to_string(),
            Arc::new(MockTool::new("CronRegister", "Register a cron task")) as Arc<dyn BaseTool>,
        );
        shared.insert(
            "mcp__slack__send".to_string(),
            Arc::new(MockTool::new("mcp__slack__send", "Send Slack message")) as Arc<dyn BaseTool>,
        );

        (index, Arc::new(RwLock::new(shared)))
    }

    #[test]
    fn test_collect_tools_returns_meta_tools() {
        let (index, shared) = build_test_components();
        let mw = ToolSearchMiddleware::new(index, shared);
        let tools = <ToolSearchMiddleware as Middleware<
            cc_agent::agent::state::AgentState,
        >>::collect_tools(&mw, "/tmp");

        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"SearchExtraTools"));
        assert!(names.contains(&"ExecuteExtraTool"));
    }

    #[test]
    fn test_registry_releases_after_middleware_drop() {
        let (index, shared) = build_test_components();
        let registry_weak = Arc::downgrade(&shared);
        let middleware = ToolSearchMiddleware::new(index, Arc::clone(&shared));
        let executor: Arc<dyn BaseTool> = Arc::new(ExecuteExtraTool::new(Arc::clone(&shared)));
        shared.write().insert(executor.name().to_string(), executor);
        drop(shared);
        assert!(
            registry_weak.upgrade().is_some(),
            "中间件存活时应持有注册表"
        );
        drop(middleware);
        assert!(
            registry_weak.upgrade().is_none(),
            "中间件销毁后元工具不得循环持有注册表"
        );
    }

    #[tokio::test]
    async fn test_execute_releases_registered_meta_tools_after_agent_drop() {
        let (index, shared) = build_test_components();
        let registry_weak = Arc::downgrade(&shared);
        let index_weak = Arc::downgrade(&index);
        let agent = cc_agent::agent::ReActAgent::new(cc_agent::llm::MockLLM::always_answer("ok"))
            .with_shared_tools(Arc::clone(&shared))
            .add_middleware(Box::new(ToolSearchMiddleware::new(
                index,
                Arc::clone(&shared),
            )));
        let mut state = cc_agent::agent::state::AgentState::new(".");
        agent
            .execute(cc_agent::agent::react::AgentInput::text("继续"), &mut state, None)
            .await
            .expect("通过真实执行器收集并注册元工具");
        let executor_weak = Arc::downgrade(
            shared.read().get("ExecuteExtraTool").expect("执行器已注册代理工具"),
        );
        drop(agent);
        drop(shared);
        assert!(registry_weak.upgrade().is_none(), "执行器销毁后应释放注册表");
        assert!(executor_weak.upgrade().is_none(), "代理工具包装及其内部对象都应释放");
        assert!(index_weak.upgrade().is_none(), "包装析构必须释放搜索工具内部持有的索引");
        assert!(!state.messages().is_empty(), "状态存活不应影响工具资源释放");
    }

    #[tokio::test]
    async fn test_before_agent_injects_system_prompt() {
        let (index, shared) = build_test_components();
        let mw = ToolSearchMiddleware::new(index, shared);

        let mut state = cc_agent::agent::state::AgentState::new("/tmp");
        mw.before_agent(&mut state).await.unwrap();

        let messages = state.messages();
        assert!(!messages.is_empty(), "before_agent 应注入 system 消息");
        let first = messages.first().unwrap();
        assert!(
            matches!(first, BaseMessage::System { .. }),
            "第一条消息应为 System"
        );
        assert!(
            first.content().contains("CronRegister"),
            "system 消息应包含延迟工具列表"
        );
    }

    #[tokio::test]
    async fn test_second_before_agent_injects_same_cached_prompt() {
        let (index, shared) = build_test_components();
        let mw = ToolSearchMiddleware::new(index, shared);

        let mut state1 = cc_agent::agent::state::AgentState::new("/tmp");
        mw.before_agent(&mut state1).await.unwrap();
        let first_content = state1.messages()[0].content().to_string();

        let mut state2 = cc_agent::agent::state::AgentState::new("/tmp");
        mw.before_agent(&mut state2).await.unwrap();
        assert_eq!(
            state2.messages().len(),
            1,
            "每轮都应注入 system 消息（System 消息被过滤后需重新注入）"
        );
        assert_eq!(
            state2.messages()[0].content(),
            first_content,
            "第二轮注入的内容应与首轮完全一致（缓存）"
        );
    }
