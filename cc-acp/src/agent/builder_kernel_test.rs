use super::*;
use cc_agent::kernel::Kernel;

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
