use super::*;
use cc_agent::{
    agent::{AgentInput, AgentState, ReActAgent},
    error::AgentResult,
    kernel::Kernel,
    llm::{BaseModel, BaseModelReactLLM, LlmRequest, LlmResponse, StopReason},
    messages::{BaseMessage, ContentBlock},
};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct McpCallingModel(AtomicUsize);

#[async_trait]
impl BaseModel for McpCallingModel {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let tool = request
            .tools
            .iter()
            .find(|tool| tool.name == "mcp__shared__Read")
            .expect("the same configured MCP tool must reach every kernel");
        assert!(tool.description.contains("Read Skill file_path"));
        assert!(tool.parameters.as_json().unwrap()["properties"]
            .get("file_path")
            .is_some());
        let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
        Ok(LlmResponse {
            message: if first {
                BaseMessage::ai_from_blocks(vec![ContentBlock::ToolUse {
                    id: "shared-mcp-call".to_string(),
                    name: tool.name.clone(),
                    input: json!({"file_path":"Read/Skill/{{cwd}}/file_path"}),
                }])
            } else {
                BaseMessage::ai("ok")
            },
            stop_reason: if first {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            },
            usage: None,
            request_id: None,
        })
    }

    fn provider_name(&self) -> &str {
        "local-test"
    }
    fn model_id(&self) -> &str {
        "shared-mcp-test"
    }
}

#[tokio::test]
async fn one_connected_mcp_pool_is_callable_under_every_kernel_without_rewriting_user_arguments() {
    let (client_transport, server_transport) = tokio::io::duplex(16_384);
    let calls = Arc::new(parking_lot::Mutex::new(Vec::<Value>::new()));
    let received_calls = calls.clone();
    let server = tokio::spawn(async move {
        let (reader, mut writer) = tokio::io::split(server_transport);
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            let request: Value = serde_json::from_str(&line).unwrap();
            let Some(id) = request.get("id") else {
                continue;
            };
            let result = match request["method"].as_str().unwrap() {
                "initialize" => json!({"protocolVersion":request["params"]["protocolVersion"],
                    "capabilities":{"tools":{}},"serverInfo":{"name":"shared","version":"1"}}),
                "tools/call" => {
                    received_calls.lock().push(request["params"].clone());
                    json!({"content":[{"type":"text","text":"SHARED_MCP_RESULT Read Skill file_path"}],"isError":false})
                }
                method => panic!("unexpected local MCP method {method}"),
            };
            let response = json!({"jsonrpc":"2.0","id":id,"result":result});
            writer
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let client = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        ().serve(client_transport),
    )
    .await
    .unwrap()
    .unwrap();
    let pool = Arc::new(McpClientPool::new_pending());
    pool.clients.write().insert(
        "shared".to_string(),
        Arc::new(crate::mcp::McpClientHandle {
            name: "shared".to_string(),
            peer: Some(client.peer().clone()),
            tools: vec![serde_json::from_value(
                json!({"name":"Read","description":"Read Skill file_path",
            "inputSchema":{"type":"object","properties":{"file_path":{"type":"string"}},
                "required":["file_path"],"additionalProperties":false}}),
            )
            .unwrap()],
            resources: vec![],
            status: crate::mcp::ClientStatus::Connected,
            oauth_status: Default::default(),
            source: None,
            url: None,
            channel_capable: false,
        }),
    );
    for id in ["cc", "dsh", "pi"] {
        let kernel = Kernel::try_from_id(id).unwrap();
        let agent = ReActAgent::new(
            BaseModelReactLLM::new(Box::new(McpCallingModel(AtomicUsize::new(0))))
                .with_kernel(kernel),
        )
        .with_kernel(kernel)
        .max_iterations(3)
        .with_tool_filter(move |name| kernel.should_hide(name))
        .add_middleware(Box::new(McpMiddleware::new(pool.clone())));
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            agent.execute(
                AgentInput::text("Call the shared MCP tool"),
                &mut AgentState::new("."),
                None,
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
        assert_eq!(
            output.tool_calls[0].1.output, "SHARED_MCP_RESULT Read Skill file_path",
            "{id}"
        );
    }
    assert_eq!(calls.lock().len(), 3);
    for call in calls.lock().iter() {
        assert_eq!(call["name"], "Read");
        assert_eq!(
            call["arguments"],
            json!({"file_path":"Read/Skill/{{cwd}}/file_path"})
        );
    }
    client.cancel().await.unwrap();
    server.abort();
}
