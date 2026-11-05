use super::*;
use async_trait::async_trait;
use cc_agent::{
    agent::react::{AgentInput, Reasoning},
    kernel::Kernel,
    messages::BaseMessage,
    tools::BaseTool,
};
use serde_json::{json, Value};

struct NamedTool(&'static str);

#[async_trait]
impl BaseTool for NamedTool {
    fn name(&self) -> &str {
        self.0
    }
    fn description(&self) -> &str {
        "Shared inherited tool"
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{}})
    }
    async fn invoke(
        &self,
        _input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        panic!("capability observation must not execute a tool");
    }
}

struct ToolSetObserver(Vec<String>);

#[async_trait]
impl ReactLLM for ToolSetObserver {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<cc_agent::llm::types::StreamingContext>,
    ) -> cc_agent::error::AgentResult<Reasoning> {
        let actual: Vec<String> = tools.iter().map(|tool| tool.name().to_string()).collect();
        assert_eq!(actual, self.0);
        let system = messages
            .iter()
            .find(|message| matches!(message, BaseMessage::System { .. }))
            .unwrap()
            .content();
        assert_eq!(serde_json::from_str::<Vec<String>>(&system).unwrap(), actual,
            "the prompt builder must receive exactly the callable tools, including shared MCP tools");
        Ok(Reasoning::with_answer("", "ok"))
    }
}

#[tokio::test]
async fn kernel_prompt_tool_list_matches_actual_subagent_tools() {
    let directory = tempfile::tempdir().unwrap();
    for id in ["cc", "dsh", "pi"] {
        let kernel = Kernel::try_from_id(id).unwrap();
        for (fork, readonly) in [(false, true), (false, false), (true, true)] {
            let tools_setting = if readonly { "tools: read\n" } else { "" };
            let definition = crate::claude_agent_parser::parse_agent_file(&format!(
                "---\nname: worker\ndescription: local worker\n{tools_setting}---\nWork directly."
            ))
            .unwrap();
            let mut expected = if readonly && !fork {
                vec!["Read".to_string()]
            } else {
                vec!["Read", "Write", "Skill", "mcp__shared__Read"]
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            };
            if !(readonly && !fork) && !kernel.should_hide("TodoWrite") {
                expected.push("TodoWrite".to_string());
            }
            expected.sort();
            let runner = crate::subagent::SubAgentTool::new(
                Arc::new(
                    ["Read", "Write", "Skill", "mcp__shared__Read", "Agent"]
                        .into_iter()
                        .map(|name| Arc::new(NamedTool(name)) as Arc<dyn BaseTool>)
                        .collect(),
                ),
                None,
                Arc::new(move |_| {
                    Box::new(ToolSetObserver(expected.clone())) as Box<dyn ReactLLM + Send + Sync>
                }),
                directory.path().to_string_lossy().to_string(),
            )
            .with_kernel(kernel)
            .with_system_builder(Arc::new(|_, _, tools| {
                serde_json::to_string(tools).unwrap()
            }));
            let build = runner
                .build_agent_from_def(
                    &definition,
                    "worker",
                    directory.path().to_str().unwrap(),
                    AgentBuildOptions {
                        fork,
                        snapshot_at_message_id: None,
                        cascade_cancel: true,
                    },
                )
                .await
                .unwrap();
            let mut state = build.state;
            build
                .builder
                .execute(AgentInput::text("Observe capabilities"), &mut state, None)
                .await
                .unwrap();
        }
    }
}
