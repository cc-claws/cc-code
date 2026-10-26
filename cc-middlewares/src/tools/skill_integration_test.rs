//! 使用真实 ReAct 执行器检查 Skill 的消息协议，全程使用本地 Mock 模型。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use cc_agent::{
    agent::{
        compact::re_inject::skill_invocation_payload,
        react::{AgentInput, ReactLLM, Reasoning, ToolCall},
        state::{AgentState, State},
        ReActAgent,
    },
    error::AgentResult,
    llm::types::StreamingContext,
    messages::{AnthropicAdapter, BaseMessage, ContentBlock, MessageAdapter, OpenAiAdapter},
    middleware::r#trait::Middleware,
    tools::BaseTool,
};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use crate::{
    skills::invocation::SkillError,
    subagent::{InheritedInstructions, SkillPreloadMiddleware, SubAgentTool},
    tool_search::{is_deferred_tool, ToolSearchIndex, ToolSearchMiddleware},
};

use super::SkillTool;

const TOOL_CALL_ID: &str = "local-skill-protocol-call";
const TOOL_THOUGHT: &str = "I will load the matching skill before continuing.";
const TOOL_SIGNATURE: &str = "local-tool-thinking-signature";
const FINAL_THOUGHT: &str = "The complete skill instruction is available.";
const FINAL_SIGNATURE: &str = "local-final-thinking-signature";
const FINAL_ANSWER: &str = "Local protocol regression complete.";
const FROZEN_SYSTEM: &str = "Stable system instructions for the offline protocol test.";
const DEFERRED_TOOL: &str = "mcp__protocol_fixture__deferred";

struct SkillFixture {
    directory: tempfile::TempDir,
    skill_root: PathBuf,
    body: String,
}

impl SkillFixture {
    fn cwd(&self) -> &str {
        self.directory
            .path()
            .to_str()
            .expect("临时目录使用合法路径")
    }

    fn make_tool(&self) -> SkillTool {
        SkillTool::new(vec![self.skill_root.clone()])
    }

    fn make_expected_body(&self, arguments: &str) -> String {
        self.body.replace("$ARGUMENTS", arguments)
    }
}

fn make_skill_fixture(name: &str) -> SkillFixture {
    let directory = tempfile::tempdir().unwrap();
    let skill_root = directory.path().join(".claude").join("skills");
    let skill_directory = skill_root.join(name);
    std::fs::create_dir_all(&skill_directory).unwrap();
    let lines = (0..120)
        .map(|index| {
            format!("Instruction {index:03}: 保留完整技能正文与参数，不得截断或模拟读取。")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "# Offline protocol fixture\nArguments: $ARGUMENTS\n{lines}\nEND_OF_COMPLETE_SKILL"
    );
    std::fs::write(
        skill_directory.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: Local protocol regression fixture\nallowed-tools: [Read, Bash]\nmodel: offline-fixture\n---\n{body}\n"
        ),
    )
    .unwrap();
    SkillFixture {
        directory,
        skill_root,
        body,
    }
}

#[derive(Clone)]
struct MockModelRequest {
    messages: Vec<BaseMessage>,
    tool_names: Vec<String>,
}

fn make_model_request(messages: &[BaseMessage], tools: &[&dyn BaseTool]) -> MockModelRequest {
    MockModelRequest {
        messages: messages.to_vec(),
        tool_names: tools.iter().map(|tool| tool.name().to_string()).collect(),
    }
}

fn make_skill_reasoning(skill_name: &str, arguments: &str) -> Reasoning {
    let input = json!({"skill": skill_name, "args": arguments});
    let mut reasoning = Reasoning::with_tools(
        TOOL_THOUGHT,
        vec![ToolCall::new(TOOL_CALL_ID, "Skill", input.clone())],
    );
    reasoning.source_message = Some(BaseMessage::ai_from_blocks(vec![
        ContentBlock::reasoning_with_signature(TOOL_THOUGHT, TOOL_SIGNATURE),
        ContentBlock::tool_use(TOOL_CALL_ID, "Skill", input),
    ]));
    reasoning
}

fn make_final_reasoning() -> Reasoning {
    let mut reasoning = Reasoning::with_answer(FINAL_THOUGHT, FINAL_ANSWER);
    reasoning.source_message = Some(BaseMessage::ai_from_blocks(vec![
        ContentBlock::reasoning_with_signature(FINAL_THOUGHT, FINAL_SIGNATURE),
        ContentBlock::text(FINAL_ANSWER),
    ]));
    reasoning
}

struct MockSkillCallingModel {
    requests: Arc<Mutex<Vec<MockModelRequest>>>,
    reasoning: Reasoning,
    expected_body: Option<String>,
    skill_name: String,
    arguments: String,
    calls: AtomicUsize,
}

impl MockSkillCallingModel {
    fn make_model(
        requests: Arc<Mutex<Vec<MockModelRequest>>>,
        skill_name: &str,
        arguments: &str,
        expected_body: Option<String>,
    ) -> Self {
        Self {
            requests,
            reasoning: make_skill_reasoning(skill_name, arguments),
            expected_body,
            skill_name: skill_name.to_string(),
            arguments: arguments.to_string(),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl ReactLLM for MockSkillCallingModel {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        self.requests
            .lock()
            .push(make_model_request(messages, tools));
        assert!(
            tools.iter().any(|tool| tool.name() == "Skill"),
            "模型必须直接看到一级 Skill 工具"
        );
        match self.calls.fetch_add(1, Ordering::Relaxed) {
            0 => {
                assert!(
                    messages.iter().all(|message| !matches!(
                        message,
                        BaseMessage::Ai { .. } | BaseMessage::Tool { .. }
                    )),
                    "首次模型请求不能包含伪造的 Ai 或 Tool 消息"
                );
                Ok(self.reasoning.clone())
            }
            1 => {
                let source = self
                    .reasoning
                    .source_message
                    .as_ref()
                    .expect("构造真实模型响应");
                let stored_source = messages
                    .iter()
                    .find(|message| message.id() == source.id())
                    .expect("第二次模型请求必须保留真实源消息身份");
                assert_eq!(
                    stored_source.content_blocks(),
                    source.content_blocks(),
                    "真实 Skill 调用的签名 thinking 与 tool_use 必须原样保留"
                );
                let calls: Vec<_> = messages.iter().flat_map(BaseMessage::tool_calls).collect();
                assert_eq!(calls.len(), 1, "只能有模型实际发出的工具调用");
                assert_eq!(calls[0].name, "Skill", "不能伪造 Read 代替 Skill");
                assert_eq!(calls[0].id, TOOL_CALL_ID, "工具身份必须保持配对");
                let results: Vec<_> = messages
                    .iter()
                    .filter_map(|message| match message {
                        BaseMessage::Tool {
                            tool_call_id,
                            content,
                            is_error,
                            ..
                        } => Some((tool_call_id, content, is_error)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(results.len(), 1, "必须且只能写入一个真实工具结果");
                let (tool_call_id, content, is_error) = results[0];
                assert_eq!(tool_call_id, TOOL_CALL_ID, "结果必须对应真实 Skill 调用");
                if let Some(expected_body) = &self.expected_body {
                    assert!(!*is_error, "技能加载必须成功");
                    let output: Value = serde_json::from_str(&content.text_content())
                        .expect("Skill 工具返回合法 JSON");
                    assert_eq!(output["success"], true, "成功标记正确");
                    assert_eq!(output["commandName"], self.skill_name, "返回真实技能名");
                    assert_eq!(output["status"], "inline", "当前会话加载状态正确");
                    assert_eq!(output["allowedTools"], json!(["Read", "Bash"]));
                    assert_eq!(output["model"], "offline-fixture");
                    assert_eq!(output["args"], self.arguments, "参数不应丢失或重写");
                    let instruction = output["instruction"].as_str().expect("包含技能指令");
                    assert!(
                        instruction.contains(expected_body),
                        "下一轮模型必须收到完整技能正文及插值后的原始参数"
                    );
                    assert!(
                        messages
                            .iter()
                            .filter(|message| message.is_system())
                            .all(|message| !message.content().contains("END_OF_COMPLETE_SKILL")),
                        "技能正文不能进入系统提示词"
                    );
                } else {
                    assert!(*is_error, "不存在的技能必须通过真实错误结果返回");
                    let output = serde_json::from_str::<Value>(&content.text_content()).ok();
                    assert!(
                        output
                            .as_ref()
                            .is_none_or(|output| output["success"] != true),
                        "错误结果不能被标为加载成功"
                    );
                }
                Ok(make_final_reasoning())
            }
            _ => panic!("单次 Skill 调用只需要两次模型请求"),
        }
    }
}

struct MockDeferredTool;

#[async_trait]
impl BaseTool for MockDeferredTool {
    fn name(&self) -> &str {
        DEFERRED_TOOL
    }

    fn description(&self) -> &str {
        "Local deferred tool used only to verify discovery filtering."
    }

    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn invoke(&self, _input: Value) -> Result<String, SkillError> {
        panic!("工具发现测试不得执行延迟工具")
    }
}

struct MockSkillToolsMiddleware(SkillTool);

#[async_trait]
impl Middleware<AgentState> for MockSkillToolsMiddleware {
    fn name(&self) -> &str {
        "MockSkillToolsMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(self.0.clone()), Box::new(MockDeferredTool)]
    }
}

#[tokio::test]
async fn test_skill_react_preserves_thinking_and_complete_instruction_with_tool_search() {
    let fixture = make_skill_fixture("autonomous-protocol");
    let arguments = "\"中文完整参数\" --target \"path with spaces\"";
    let expected_body = fixture.make_expected_body(arguments);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let model = MockSkillCallingModel::make_model(
        Arc::clone(&requests),
        "autonomous-protocol",
        arguments,
        Some(expected_body.clone()),
    );
    let shared_tools: Arc<RwLock<HashMap<String, Arc<dyn BaseTool>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let index = Arc::new(ToolSearchIndex::new());
    let agent = ReActAgent::new(model)
        .max_iterations(3)
        .add_middleware(Box::new(MockSkillToolsMiddleware(fixture.make_tool())))
        .add_middleware(Box::new(ToolSearchMiddleware::new(
            Arc::clone(&index),
            Arc::clone(&shared_tools),
        )))
        .with_shared_tools(shared_tools)
        .with_tool_filter(is_deferred_tool)
        .with_system_prompt(FROZEN_SYSTEM);
    let mut state = AgentState::new(fixture.cwd());
    let output = agent
        .execute(
            AgentInput::text("Run the matching local skill."),
            &mut state,
            None,
        )
        .await
        .expect("真实 ReAct 工具执行成功");
    assert_eq!(output.text, FINAL_ANSWER);
    assert_eq!(output.tool_calls.len(), 1, "没有伪造 Read 或二次加载");
    assert_eq!(output.tool_calls[0].0.name, "Skill");
    assert!(!output.tool_calls[0].1.is_error);
    assert_eq!(index.total_count(), 1, "只有测试延迟工具进入发现索引");
    assert!(
        index.get_tool("Skill").is_none(),
        "Skill 不是 deferred 工具"
    );
    assert!(index.get_tool(DEFERRED_TOOL).is_some());
    let requests = requests.lock();
    assert_eq!(requests.len(), 2, "加载后模型继续执行第二步");
    for request in requests.iter() {
        assert!(request.tool_names.iter().any(|name| name == "Skill"));
        assert!(request
            .tool_names
            .iter()
            .any(|name| name == "SearchExtraTools"));
        assert!(request
            .tool_names
            .iter()
            .any(|name| name == "ExecuteExtraTool"));
        assert!(!request.tool_names.iter().any(|name| name == DEFERRED_TOOL));
        assert_eq!(
            request.messages[0].content(),
            FROZEN_SYSTEM,
            "系统提示词保持稳定"
        );
    }
    assert!(
        !requests[0]
            .messages
            .iter()
            .any(|message| message.content().contains("END_OF_COMPLETE_SKILL")),
        "自主调用前不预先泄露正文"
    );
    let messages = &requests[1].messages;
    // 这里只验证公开消息适配器的离线协议结构，不代表远端 HTTP/provider 验收。
    let openai = OpenAiAdapter::from_base_messages(messages);
    let openai = openai.as_array().expect("OpenAI 消息数组");
    let assistant_index = openai
        .iter()
        .position(|message| message["tool_calls"][0]["id"] == TOOL_CALL_ID)
        .expect("OpenAI 保留真实函数调用");
    assert_eq!(openai[assistant_index]["reasoning_content"], TOOL_THOUGHT);
    assert_eq!(
        openai[assistant_index]["tool_calls"][0]["function"]["name"],
        "Skill"
    );
    assert_eq!(openai[assistant_index + 1]["role"], "tool");
    assert_eq!(openai[assistant_index + 1]["tool_call_id"], TOOL_CALL_ID);
    let openai_result: Value = serde_json::from_str(
        openai[assistant_index + 1]["content"]
            .as_str()
            .expect("工具正文使用文本"),
    )
    .expect("OpenAI 工具结果 JSON");
    assert!(openai_result["instruction"]
        .as_str()
        .expect("完整指令")
        .contains(&expected_body));
    let (anthropic, _) = AnthropicAdapter::to_anthropic_with_system(messages);
    let assistant_index = anthropic
        .iter()
        .position(|message| message["content"][1]["id"] == TOOL_CALL_ID)
        .expect("Anthropic 保留真实工具调用");
    assert_eq!(anthropic[assistant_index]["content"][0]["type"], "thinking");
    assert_eq!(
        anthropic[assistant_index]["content"][0]["thinking"],
        TOOL_THOUGHT
    );
    assert_eq!(
        anthropic[assistant_index]["content"][0]["signature"],
        TOOL_SIGNATURE
    );
    assert_eq!(anthropic[assistant_index]["content"][1]["name"], "Skill");
    assert_eq!(anthropic[assistant_index + 1]["role"], "user");
    assert_eq!(
        anthropic[assistant_index + 1]["content"][0]["type"],
        "tool_result"
    );
    assert_eq!(
        anthropic[assistant_index + 1]["content"][0]["tool_use_id"],
        TOOL_CALL_ID
    );
    assert_eq!(
        anthropic[assistant_index + 1]["content"][0]["is_error"],
        false
    );
    let anthropic_result: Value = serde_json::from_str(
        anthropic[assistant_index + 1]["content"][0]["content"]
            .as_str()
            .expect("Anthropic 工具正文使用文本"),
    )
    .expect("Anthropic 工具结果 JSON");
    assert_eq!(
        anthropic_result["instruction"], openai_result["instruction"],
        "两种消息适配器都必须保留完整指令"
    );
    assert!(
        state.messages().iter().all(|message| !message.is_system()),
        "临时注入的系统消息按原有执行器生命周期清理"
    );
}

struct MockPreloadedSkillModel {
    requests: Arc<Mutex<Vec<MockModelRequest>>>,
    expected_body: String,
    arguments: String,
}

#[async_trait]
impl ReactLLM for MockPreloadedSkillModel {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        self.requests
            .lock()
            .push(make_model_request(messages, tools));
        assert!(
            messages.iter().all(|message| !matches!(
                message,
                BaseMessage::Ai { .. } | BaseMessage::Tool { .. }
            )),
            "Slash 已加载指令不能伪造缺少 thinking 的助手或工具消息"
        );
        let human_messages: Vec<_> = messages
            .iter()
            .filter(|message| matches!(message, BaseMessage::Human { .. }))
            .collect();
        assert_eq!(human_messages.len(), 2, "保留用户输入并追加一条元指令");
        let marker = human_messages[1].content();
        let payload: Value = serde_json::from_str(
            skill_invocation_payload(&marker).expect("Slash 使用已加载技能标记"),
        )
        .expect("Slash 元指令为合法 JSON");
        assert_eq!(payload["success"], true);
        assert_eq!(payload["commandName"], "slash-protocol");
        assert_eq!(payload["display"], true, "首次 Slash 可投影工具卡片");
        assert_eq!(
            payload["args"], self.arguments,
            "引号、空白和中文参数完整保留"
        );
        assert!(payload["instruction"]
            .as_str()
            .expect("完整指令")
            .contains(&self.expected_body));
        let system_messages: Vec<_> = messages
            .iter()
            .filter(|message| message.is_system())
            .collect();
        assert_eq!(system_messages.len(), 1, "Slash 不追加系统消息");
        assert_eq!(
            system_messages[0].content(),
            FROZEN_SYSTEM,
            "系统提示词不受技能加载影响"
        );
        Ok(make_final_reasoning())
    }
}

#[tokio::test]
async fn test_skill_slash_react_loads_human_instruction_without_synthetic_tool_messages() {
    let fixture = make_skill_fixture("slash-protocol");
    let arguments = "\"含空格 中文参数\"  --target 'local path'";
    let requests = Arc::new(Mutex::new(Vec::new()));
    let model = MockPreloadedSkillModel {
        requests: Arc::clone(&requests),
        expected_body: fixture.make_expected_body(arguments),
        arguments: arguments.to_string(),
    };
    let tool = fixture.make_tool();
    let preload =
        SkillPreloadMiddleware::new(Vec::new(), fixture.cwd()).with_skill_tool(tool.clone());
    let agent = ReActAgent::new(model)
        .max_iterations(2)
        .register_tool(Box::new(tool))
        .add_middleware(Box::new(preload))
        .with_system_prompt(FROZEN_SYSTEM);
    let mut state = AgentState::new(fixture.cwd());
    let input = format!("/slash-protocol {arguments}");
    let output = agent
        .execute(AgentInput::text(&input), &mut state, None)
        .await
        .expect("Slash 元指令走真实 ReAct 执行成功");
    assert_eq!(output.text, FINAL_ANSWER);
    assert!(
        output.tool_calls.is_empty(),
        "显式 Slash 已加载，不需要再次调用 Skill"
    );
    assert_eq!(
        state.messages().len(),
        3,
        "仅用户输入、已加载指令和真实模型回答"
    );
    assert_eq!(state.messages()[0].content(), input);
    assert!(matches!(state.messages()[1], BaseMessage::Human { .. }));
    assert!(matches!(state.messages()[2], BaseMessage::Ai { .. }));
    assert_eq!(
        state.messages()[2].content_blocks()[0],
        ContentBlock::reasoning_with_signature(FINAL_THOUGHT, FINAL_SIGNATURE),
        "Slash 后的真实回答仍保留模型 thinking"
    );
    let requests = requests.lock();
    assert_eq!(requests.len(), 1, "模型一次完成，不需要伪造技能工具往返");
    let messages = &requests[0].messages;
    let openai = OpenAiAdapter::from_base_messages(messages);
    assert!(
        openai
            .as_array()
            .expect("OpenAI 消息数组")
            .iter()
            .all(|message| {
                matches!(message["role"].as_str(), Some("system" | "user"))
                    && message.get("tool_calls").is_none()
            }),
        "OpenAI 请求只包含 system/user，无无 thinking 的假 assistant"
    );
    let (anthropic, system) = AnthropicAdapter::to_anthropic_with_system(messages);
    assert_eq!(system.as_deref(), Some(FROZEN_SYSTEM));
    assert_eq!(anthropic.len(), 2);
    assert!(
        anthropic
            .iter()
            .all(|message| message["role"] == "user" && message["content"].is_string()),
        "Anthropic Slash 请求不含伪造 assistant/tool_use/tool_result"
    );
}

#[tokio::test]
async fn test_skill_child_agent_inherits_and_executes_parent_skill_tool() {
    let fixture = make_skill_fixture("child-protocol");
    let arguments = "\"子代理完整参数\"";
    let requests = Arc::new(Mutex::new(Vec::new()));
    let child_name = format!("offline-protocol-child-{}", uuid::Uuid::new_v4());
    let agent_directory = fixture.directory.path().join(".claude").join("agents");
    std::fs::create_dir_all(&agent_directory).unwrap();
    std::fs::write(
        agent_directory.join(format!("{child_name}.md")),
        format!("---\nname: {child_name}\ndescription: Local child fixture\ntools: '*'\nmaxTurns: 3\n---\nComplete the local protocol fixture.\n"),
    )
    .unwrap();
    let child_requests = Arc::clone(&requests);
    let expected_body = fixture.make_expected_body(arguments);
    let runner = SubAgentTool::new(
        Arc::new(vec![Arc::new(fixture.make_tool()) as Arc<dyn BaseTool>]),
        None,
        Arc::new(move |_| {
            Box::new(MockSkillCallingModel::make_model(
                Arc::clone(&child_requests),
                "child-protocol",
                arguments,
                Some(expected_body.clone()),
            )) as Box<dyn ReactLLM + Send + Sync>
        }),
        fixture.cwd().to_string(),
    )
    .with_inherited_instructions(Some(InheritedInstructions {
        cwd: Arc::from(fixture.cwd()),
        rendered: Arc::from(""),
    }));
    let (agent_id, result) = runner
        .invoke_skill(
            "delegating-protocol",
            "Load child-protocol using the inherited Skill tool.",
            Some(&child_name),
            None,
        )
        .await
        .expect("真实子执行器继承 Skill 并加载完整指令");
    assert!(!agent_id.is_empty(), "隔离执行有独立身份");
    assert_eq!(result, FINAL_ANSWER);
    let requests = requests.lock();
    assert_eq!(requests.len(), 2, "子 Agent 真正执行一次 Skill 工具往返");
    assert!(
        requests
            .iter()
            .all(|request| request.tool_names.iter().any(|name| name == "Skill")),
        "子 Agent 每次请求都继承父 Skill 工具"
    );
    assert!(
        requests
            .iter()
            .all(|request| request.tool_names.iter().all(|name| name != "Agent")),
        "原有禁止子 Agent 递归委派的规则保持有效"
    );
}

#[tokio::test]
async fn test_skill_react_failure_keeps_real_thinking_and_paired_error_result() {
    let fixture = make_skill_fixture("existing-protocol");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let model =
        MockSkillCallingModel::make_model(Arc::clone(&requests), "missing-protocol", "", None);
    let agent = ReActAgent::new(model)
        .max_iterations(3)
        .register_tool(Box::new(fixture.make_tool()))
        .with_system_prompt(FROZEN_SYSTEM);
    let mut state = AgentState::new(fixture.cwd());
    let output = agent
        .execute(
            AgentInput::text("Load a missing local skill."),
            &mut state,
            None,
        )
        .await
        .expect("工具错误配对后允许模型继续回答");
    assert_eq!(output.text, FINAL_ANSWER);
    assert_eq!(output.tool_calls.len(), 1);
    assert!(
        output.tool_calls[0].1.is_error,
        "真实工具失败必须保留错误标记"
    );
    let requests = requests.lock();
    assert_eq!(requests.len(), 2);
    let (anthropic, _) = AnthropicAdapter::to_anthropic_with_system(&requests[1].messages);
    let error_result = anthropic
        .iter()
        .flat_map(|message| message["content"].as_array().into_iter().flatten())
        .find(|block| block["type"] == "tool_result")
        .expect("Anthropic 错误仍有真实工具结果");
    assert_eq!(error_result["tool_use_id"], TOOL_CALL_ID);
    assert_eq!(error_result["is_error"], true, "错误不能序列化为加载成功");
}
