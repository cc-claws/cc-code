use super::*;
use cc_agent::kernel::Kernel;

#[tokio::test]
async fn forked_skills_receive_only_real_subagent_capabilities() {
    let directory = tempfile::tempdir().unwrap();
    let skill_directory = directory.path().join(".claude/skills/kernel-worker");
    let agent_directory = directory.path().join(".claude/agents");
    std::fs::create_dir_all(&skill_directory).unwrap();
    std::fs::create_dir_all(&agent_directory).unwrap();
    std::fs::write(skill_directory.join("SKILL.md"),
        "---\nname: kernel-worker\ndescription: Inspect the local fixture\ncontext: fork\nagent: kernel-worker\n---\nRead the assigned fixture and report its content.\n").unwrap();
    let fixture = directory.path().join("fixture.txt");
    std::fs::write(&fixture, "CHILD_READ_RESULT").unwrap();
    for id in ["cc", "dsh", "pi"] {
        for readonly in [true, false] {
            let tools_setting = if readonly { "tools: Read\n" } else { "" };
            std::fs::write(agent_directory.join("kernel-worker.md"), format!(
                "---\nname: kernel-worker\ndescription: Local worker\n{tools_setting}---\nInspect the assigned fixture.\n"
            )).unwrap();
            let kernel = Kernel::try_from_id(id).unwrap();
            let read_input = if id == "pi" {
                json!({"path":fixture})
            } else {
                json!({"file_path":fixture})
            };
            let responses = [
                (kernel.echo_name("Skill"), json!({"skill":"kernel-worker"})),
                (kernel.echo_name("Read"), read_input),
            ].into_iter().enumerate().map(|(index, (name, input))| make_http_stream_response(&format!(
                "data: {}\n\ndata: [DONE]\n\n", json!({"id":"child-audit","choices":[{"index":0,"delta":{
                    "role":"assistant","tool_calls":[{"index":0,"id":format!("child-call-{index}"),"type":"function",
                    "function":{"name":name,"arguments":input.to_string()}}]},"finish_reason":"tool_calls"}]})
            ))).collect();
            let endpoint = make_scripted_model_endpoint(MockProvider::OpenAi, responses).await;
            let mut config = make_agent_config(
                directory.path(),
                make_provider(MockProvider::OpenAi, &endpoint.base_url),
                "PARENT_AGENT_BASE".to_string(),
                None,
            );
            config.kernel = kernel;
            let pool = Arc::new(Mutex::new(AgentPool::new()));
            let (agent, _) = build_agent_with_embedded_memory(config, None, &pool);
            let mut state = AgentState::new(directory.path().to_string_lossy());
            let output = tokio::time::timeout(
                Duration::from_secs(15),
                agent.executor.execute(
                    AgentInput::text("Run the kernel-worker skill"),
                    &mut state,
                    None,
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(output.tool_calls.len(), 1, "{id}/{readonly}");
            assert!(
                !output.tool_calls[0].1.is_error,
                "{id}/{readonly}: {:?}",
                output.tool_calls
            );
            let requests = endpoint.requests.lock();
            assert_eq!(requests.len(), 4, "{id}/{readonly}");
            for request in &requests[1..3] {
                let messages = request.body["messages"].to_string();
                assert!(messages.contains(&format!("# Agent Kernel: {id}")));
                assert!(!messages.contains("SubAgent Delegation"));
                assert!(!messages.contains("SearchExtraTools"));
                assert!(!messages.contains("ExecuteExtraTool"));
                assert!(!messages.contains("Scheduled Tasks (Cron)"));
                let names: Vec<&str> = request.body["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|tool| tool["function"]["name"].as_str().unwrap())
                    .collect();
                assert!(names.contains(&kernel.echo_name("Read")));
                for unavailable in ["Agent", "WebFetch", "AskUserQuestion", "CronList"] {
                    assert!(
                        !names.contains(&kernel.echo_name(unavailable)),
                        "{id}/{readonly}: {names:?}"
                    );
                }
                if readonly {
                    assert_eq!(names, vec![kernel.echo_name("Read")]);
                    assert!(!messages.contains("BLOCKING REQUIREMENT"));
                } else {
                    assert!(names.contains(&kernel.echo_name("Skill")));
                    assert!(messages.contains("nested delegation is unavailable"));
                }
            }
            assert!(requests[2].body["messages"]
                .to_string()
                .contains("CHILD_READ_RESULT"));
        }
    }
}

#[derive(Clone)]
struct DeferredAuditTool {
    calls: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl cc_agent::tools::BaseTool for DeferredAuditTool {
    fn name(&self) -> &str {
        "mcp__shared__lookup"
    }

    fn description(&self) -> &str {
        "Look up shared user data"
    }

    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"file_path":{"type":"string"}},"required":["file_path"]})
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.calls.lock().push(input);
        Ok("SHARED_DEFERRED_RESULT".to_string())
    }
}

#[tokio::test]
async fn root_agent_keeps_cc_tools_deferred_and_other_kernels_direct() {
    let directory = tempfile::tempdir().unwrap();
    for id in ["cc", "dsh", "pi"] {
        let kernel = Kernel::try_from_id(id).unwrap();
        let arguments = json!({"file_path":"Read/Skill/file_path"});
        let mut calls = Vec::new();
        if kernel.exposes_tool_search() {
            calls.push((
                "SearchExtraTools",
                json!({"query":"select:mcp__shared__lookup"}),
            ));
            calls.push((
                "ExecuteExtraTool",
                json!({"tool_name":"mcp__shared__lookup","params":arguments}),
            ));
        } else {
            calls.push(("mcp__shared__lookup", arguments.clone()));
        }
        let responses = calls.iter().enumerate().map(|(index, (name, input))| {
            make_http_stream_response(&format!(
                "data: {}\n\ndata: [DONE]\n\n",
                json!({"id":"deferred-audit", "choices":[{"index":0,"delta":{
                    "role":"assistant", "tool_calls":[{"index":0,"id":format!("deferred-{index}"),"type":"function",
                    "function":{"name":name,"arguments":input.to_string()}}]
                },"finish_reason":"tool_calls"}]})
            ))
        }).collect();
        let endpoint = make_scripted_model_endpoint(MockProvider::OpenAi, responses).await;
        let mut config = make_agent_config(
            directory.path(),
            make_provider(MockProvider::OpenAi, &endpoint.base_url),
            "Use the configured tools".to_string(),
            None,
        );
        config.kernel = kernel;
        let tool = DeferredAuditTool {
            calls: Arc::default(),
        };
        config
            .shared_tools
            .write()
            .insert("mcp__shared__lookup".to_string(), Arc::new(tool.clone()));
        let pool = Arc::new(Mutex::new(AgentPool::new()));
        let (agent, _) = build_agent_with_embedded_memory(config, None, &pool);
        let executor = agent.executor.register_tool(Box::new(tool.clone()));
        let mut state = AgentState::new(directory.path().to_string_lossy());
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            executor.execute(AgentInput::text("Look up shared data"), &mut state, None),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(output.tool_calls.len(), calls.len(), "{id}");
        assert!(
            output.tool_calls.iter().all(|(_, result)| !result.is_error),
            "{id}: {:?}",
            output.tool_calls
        );
        assert_eq!(*tool.calls.lock(), vec![arguments], "{id}");
        let requests = endpoint.requests.lock();
        assert_eq!(requests.len(), calls.len() + 1, "{id}");
        for request in requests.iter() {
            let tools = request.body["tools"].as_array().unwrap();
            let visible = |name: &str| tools.iter().any(|tool| tool["function"]["name"] == name);
            assert_eq!(
                visible("mcp__shared__lookup"),
                !kernel.exposes_tool_search(),
                "{id}: deferred schemas must stay out of cc requests"
            );
            assert_eq!(visible("CronList"), !kernel.exposes_tool_search(), "{id}");
            assert_eq!(
                visible("SearchExtraTools"),
                kernel.exposes_tool_search(),
                "{id}"
            );
            assert_eq!(
                visible("ExecuteExtraTool"),
                kernel.exposes_tool_search(),
                "{id}"
            );
        }
    }
}

#[tokio::test]
async fn configured_skills_and_memory_work_through_every_kernel() {
    let directory = tempfile::tempdir().unwrap();
    let skill_directory = directory.path().join(".claude/skills/shared-audit");
    std::fs::create_dir_all(&skill_directory).unwrap();
    let description = "USER_DESCRIPTION Read Skill file_path mcp__shared__lookup";
    std::fs::write(skill_directory.join("SKILL.md"), format!(
        "---\nname: shared-audit\ndescription: {description}\n---\n\nSHARED_SKILL_BODY Read Skill file_path\n"
    )).unwrap();
    let memory = crate::prompt::AutoMemorySnapshot {
        directory: directory.path().join("memory"),
        display_directory: "Read/Skill/file_path/shared-memory".to_string(),
        content: "SHARED_MEMORY_BODY Read Skill file_path {{cwd}} {{date}}".to_string(),
    };
    for id in ["cc", "dsh", "pi"] {
        let kernel = Kernel::try_from_id(id).unwrap();
        let response = make_http_stream_response(&format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"id":"shared-skill", "choices":[{"index":0,"delta":{
                "role":"assistant", "tool_calls":[{"index":0,"id":"shared-skill-call","type":"function",
                "function":{"name":kernel.echo_name("Skill"),"arguments":json!({"skill":"shared-audit"}).to_string()}}]
            },"finish_reason":"tool_calls"}]})
        ));
        let endpoint = make_model_endpoint(MockProvider::OpenAi, Some(response)).await;
        let prompt = crate::prompt::build_system_prompt_with_memory_for_kernel(
            None,
            directory.path().to_str().unwrap(),
            crate::prompt::PromptFeatures::detect(),
            &[],
            Some("2026-11-05"),
            Some("zh-CN"),
            Some(&memory),
            kernel,
        );
        let mut config = make_agent_config(
            directory.path(),
            make_provider(MockProvider::OpenAi, &endpoint.base_url),
            prompt,
            None,
        );
        config.kernel = kernel;
        config.frozen_skill_summary = cc_middlewares::SkillsMiddleware::build_frozen_summary(
            directory.path().to_str().unwrap(),
            &[],
        );
        let cancel = config.cancel.clone();
        let pool = Arc::new(Mutex::new(AgentPool::new()));
        let (agent, _) = build_agent_with_embedded_memory(config, None, &pool);
        let mut state = AgentState::new(directory.path().to_string_lossy());
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            agent.executor.execute(
                AgentInput::text("Use the configured shared-audit skill"),
                &mut state,
                Some(cancel),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(output.tool_calls.len(), 1, "{id}");
        assert!(
            !output.tool_calls[0].1.is_error,
            "{id}: {:?}",
            output.tool_calls[0].1.output
        );
        assert!(
            output.tool_calls[0]
                .1
                .output
                .contains("SHARED_SKILL_BODY Read Skill file_path"),
            "{id}"
        );
        let requests = endpoint.requests.lock();
        assert_eq!(requests.len(), 2, "{id}: skill call and continuation");
        for request in requests.iter() {
            let messages = request.body["messages"].to_string();
            assert!(
                messages.contains(&memory.content),
                "{id}: shared memory must be preserved"
            );
            assert!(
                messages.contains(&memory.display_directory),
                "{id}: shared memory path must be preserved"
            );
            assert!(
                messages.contains(description),
                "{id}: user descriptions must be preserved"
            );
            let tools = request.body["tools"].as_array().unwrap();
            assert!(
                tools
                    .iter()
                    .any(|tool| tool["function"]["name"] == kernel.echo_name("Skill")),
                "{id}"
            );
            if !kernel.exposes_tool_search() {
                assert!(
                    !tools
                        .iter()
                        .any(|tool| tool["function"]["name"] == "SearchExtraTools"),
                    "{id}"
                );
                assert!(!messages.contains("invoke SearchExtraTools"), "{id}");
            }
        }
    }
}
