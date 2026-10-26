use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use cc_agent::{
    agent::{
        events::{AgentEvent, AgentEventHandler},
        react::{ReactLLM, Reasoning},
        AgentCancellationToken,
    },
    error::{AgentError, AgentResult},
    llm::types::StreamingContext,
    messages::{BaseMessage, ContentBlock},
    tools::BaseTool,
};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::{
    skills::invocation::SkillError,
    subagent::{get_built_in_agent, InheritedInstructions},
};

use super::{SkillEventHandler, SubAgentTool};

const CHILD_INSTRUCTION: &str = "FORK_INSTRUCTION_ONLY: run the complete isolated workflow.";
const CHILD_RESULT: &str = "The isolated skill finished with its final result.";
const CHILD_SYSTEM: &str = "Frozen child system prompt.";
const FROZEN_GUIDANCE: &str = "Frozen inherited project guidance.";
const AGENT_PERSONA: &str = "LOCAL_SKILL_WORKER_PERSONA";

struct MockSkillFixture {
    directory: tempfile::TempDir,
    agent_name: String,
    skill_name: String,
    skills_root: PathBuf,
}

impl MockSkillFixture {
    fn cwd(&self) -> &str {
        self.directory.path().to_str().expect("临时目录路径有效")
    }
}

fn make_skill_fixture() -> MockSkillFixture {
    let directory = tempfile::tempdir().unwrap();
    let suffix = uuid::Uuid::new_v4();
    let agent_name = format!("offline-skill-worker-{suffix}");
    let skill_name = format!("offline-fork-skill-{suffix}");
    let agent_directory = directory.path().join(".claude").join("agents");
    let skills_root = directory.path().join(".claude").join("skills");
    let skill_directory = skills_root.join(&skill_name);
    std::fs::create_dir_all(&agent_directory).unwrap();
    std::fs::create_dir_all(&skill_directory).unwrap();
    std::fs::write(
        skill_directory.join("SKILL.md"),
        format!("---\nname: {skill_name}\ndescription: Offline fork fixture\n---\nDO_NOT_PRELOAD_THE_SAME_SKILL_AGAIN\n"),
    )
    .unwrap();
    std::fs::write(
        agent_directory.join(format!("{agent_name}.md")),
        format!("---\nname: {agent_name}\ndescription: Offline skill worker\ntools: Read\nmodel: definition-model\nskills: [{skill_name}]\nmaxTurns: 2\n---\n{AGENT_PERSONA}\n"),
    )
    .unwrap();
    MockSkillFixture {
        directory,
        agent_name,
        skill_name,
        skills_root,
    }
}

#[derive(Clone)]
struct MockModelRequest {
    messages: Vec<BaseMessage>,
    tool_names: Vec<String>,
}

#[derive(Default)]
struct MockObservations {
    requests: Mutex<Vec<MockModelRequest>>,
    model_aliases: Mutex<Vec<Option<String>>>,
    personas: Mutex<Vec<Option<String>>>,
}

#[derive(Clone)]
enum MockResponse {
    Complete,
    Wait {
        entered: Arc<Notify>,
        release: Arc<Notify>,
        dropped: Arc<AtomicBool>,
    },
    Fail,
}

struct MockDropProbe(Arc<AtomicBool>);

impl Drop for MockDropProbe {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct MockSkillModel {
    observations: Arc<MockObservations>,
    response: MockResponse,
}

#[async_trait]
impl ReactLLM for MockSkillModel {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        self.observations.requests.lock().push(MockModelRequest {
            messages: messages.to_vec(),
            tool_names: tools.iter().map(|tool| tool.name().to_string()).collect(),
        });
        match &self.response {
            MockResponse::Wait {
                entered,
                release,
                dropped,
            } => {
                let _drop_probe = MockDropProbe(Arc::clone(dropped));
                entered.notify_one();
                release.notified().await;
            }
            MockResponse::Fail => {
                return Err(AgentError::Other(anyhow::anyhow!("offline-model-failure")));
            }
            MockResponse::Complete => {}
        }
        let mut reasoning = Reasoning::with_answer("The isolated task is complete.", CHILD_RESULT);
        reasoning.source_message = Some(BaseMessage::ai_from_blocks(vec![
            ContentBlock::reasoning_with_signature(
                "The isolated task is complete.",
                "local-child-signature",
            ),
            ContentBlock::text(CHILD_RESULT),
        ]));
        Ok(reasoning)
    }
}

struct MockNamedTool(&'static str);

#[async_trait]
impl BaseTool for MockNamedTool {
    fn name(&self) -> &str {
        self.0
    }

    fn description(&self) -> &str {
        "Local inherited tool fixture."
    }

    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    async fn invoke(&self, _input: Value) -> Result<String, SkillError> {
        panic!("隔离执行测试不应调用无关工具")
    }
}

#[derive(Default)]
struct MockEventRecorder {
    events: Mutex<Vec<AgentEvent>>,
}

impl AgentEventHandler for MockEventRecorder {
    fn on_event(&self, event: AgentEvent) {
        self.events.lock().push(event);
    }

    fn wants_llm_call_payload(&self) -> bool {
        true
    }
}

fn make_runner(
    fixture: &MockSkillFixture,
    observations: Arc<MockObservations>,
    response: MockResponse,
    handler: Option<Arc<dyn AgentEventHandler>>,
) -> SubAgentTool {
    let model_observations = Arc::clone(&observations);
    SubAgentTool::new(
        Arc::new(vec![
            Arc::new(MockNamedTool("Read")) as Arc<dyn BaseTool>,
            Arc::new(MockNamedTool("Write")) as Arc<dyn BaseTool>,
            Arc::new(MockNamedTool("Agent")) as Arc<dyn BaseTool>,
        ]),
        handler,
        Arc::new(move |model| {
            model_observations
                .model_aliases
                .lock()
                .push(model.map(str::to_string));
            Box::new(MockSkillModel {
                observations: Arc::clone(&model_observations),
                response: response.clone(),
            }) as Box<dyn ReactLLM + Send + Sync>
        }),
        fixture.cwd().to_string(),
    )
    .with_extra_skill_dirs(vec![fixture.skills_root.clone()])
    .with_inherited_instructions(Some(InheritedInstructions {
        cwd: Arc::from(fixture.cwd()),
        rendered: Arc::from(FROZEN_GUIDANCE),
    }))
    .with_system_builder(Arc::new(move |overrides, _cwd| {
        observations
            .personas
            .lock()
            .push(overrides.and_then(|overrides| overrides.persona.clone()));
        CHILD_SYSTEM.to_string()
    }))
}

#[derive(Default)]
struct MockRuntimeCallbacks {
    registrations: Mutex<Vec<(String, AgentCancellationToken, String)>>,
    deregistrations: Mutex<Vec<String>>,
}

fn make_runtime_tracked_runner(
    runner: SubAgentTool,
    callbacks: Arc<MockRuntimeCallbacks>,
) -> SubAgentTool {
    let registration_callbacks = Arc::clone(&callbacks);
    runner
        .with_register_runtime(Arc::new(move |agent_id, cancel, policy| {
            registration_callbacks
                .registrations
                .lock()
                .push((agent_id, cancel, policy));
        }))
        .with_deregister_runtime(Arc::new(move |agent_id| {
            callbacks.deregistrations.lock().push(agent_id.to_string());
        }))
}

#[tokio::test]
async fn test_invoke_skill_waits_for_final_result_and_isolates_parent_history() {
    let fixture = make_skill_fixture();
    let observations = Arc::new(MockObservations::default());
    let callbacks = Arc::new(MockRuntimeCallbacks::default());
    let parent_events = Arc::new(MockEventRecorder::default());
    let child_events = Arc::new(MockEventRecorder::default());
    let factory_ids = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let parent_cancel = AgentCancellationToken::new();
    let parent_messages = Arc::new(RwLock::new(vec![
        BaseMessage::system("PARENT_SYSTEM_SECRET"),
        BaseMessage::human("PARENT_HISTORY_SECRET"),
        BaseMessage::ai("PARENT_REPLY_SECRET"),
    ]));
    let initial_parent_messages =
        serde_json::to_value(&*parent_messages.read()).expect("父消息可序列化");
    let runner = make_runner(
        &fixture,
        Arc::clone(&observations),
        MockResponse::Wait {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
            dropped: Arc::clone(&dropped),
        },
        Some(parent_events.clone()),
    )
    .with_parent_messages(Arc::clone(&parent_messages))
    .with_cancel(parent_cancel.clone())
    .with_child_handler_factory(Arc::new({
        let child_events = Arc::clone(&child_events);
        let factory_ids = Arc::clone(&factory_ids);
        move |agent_id| {
            factory_ids.lock().push(agent_id);
            child_events.clone() as Arc<dyn AgentEventHandler>
        }
    }));
    let runner = make_runtime_tracked_runner(runner, Arc::clone(&callbacks));
    let skill_name = fixture.skill_name.clone();
    let agent_name = fixture.agent_name.clone();
    let mut execution = tokio::spawn(async move {
        runner
            .invoke_skill(
                &skill_name,
                CHILD_INSTRUCTION,
                Some(&agent_name),
                Some("explicit-model"),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("子模型进入执行");
    assert!(
        !execution.is_finished(),
        "同步 Skill 必须等待最终结果，不提前返回后台启动串"
    );
    assert!(
        callbacks.deregistrations.lock().is_empty(),
        "执行中不能提前注销"
    );
    {
        let registrations = callbacks.registrations.lock();
        assert_eq!(registrations.len(), 1, "实际子执行只注册一次");
        assert_eq!(registrations[0].2, "cascade", "同步 fork 使用级联取消策略");
        assert!(!registrations[0].1.is_cancelled(), "子执行运行时令牌有效");
    }
    release.notify_one();
    let (agent_id, result) = tokio::time::timeout(Duration::from_secs(5), &mut execution)
        .await
        .expect("释放模型后执行完成")
        .expect("执行任务正常退出")
        .expect("Skill 返回最终结果");
    assert_eq!(result, CHILD_RESULT);
    assert!(
        !result.starts_with("Background task"),
        "不能复用后台任务启动回执"
    );
    assert!(dropped.load(Ordering::Relaxed), "已释放模型 future");
    assert!(!parent_cancel.is_cancelled(), "子执行完成不取消父执行");
    assert_eq!(
        &*callbacks.deregistrations.lock(),
        &[agent_id.clone()],
        "完成后只注销一次"
    );
    assert_eq!(callbacks.registrations.lock()[0].0, agent_id);
    assert!(
        callbacks.registrations.lock()[0].1.is_cancelled(),
        "返回后清理子取消令牌"
    );
    assert_eq!(
        &*factory_ids.lock(),
        &[agent_id.clone()],
        "工厂使用真实子身份"
    );
    assert!(
        parent_events.events.lock().is_empty(),
        "子处理器工厂优先，不能回落到父事件出口"
    );
    assert_eq!(
        serde_json::to_value(&*parent_messages.read()).expect("父消息可序列化"),
        initial_parent_messages,
        "主历史和主系统正文均保持原样"
    );
    assert_eq!(
        &*observations.model_aliases.lock(),
        &[Some("explicit-model".to_string())]
    );
    assert!(
        observations.personas.lock()[0]
            .as_deref()
            .is_some_and(|persona| persona.contains(AGENT_PERSONA)),
        "使用选定 agent 的 persona"
    );
    let requests = observations.requests.lock();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let human_messages: Vec<_> = request
        .messages
        .iter()
        .filter(|message| matches!(message, BaseMessage::Human { .. }))
        .collect();
    assert_eq!(
        human_messages.len(),
        1,
        "独立子上下文只接收一次技能全文，同名 preload 已移除"
    );
    assert_eq!(human_messages[0].content(), CHILD_INSTRUCTION);
    assert!(
        request
            .messages
            .iter()
            .all(|message| !message.content().contains("PARENT_")),
        "不继承父会话任何历史"
    );
    assert!(
        request
            .messages
            .iter()
            .filter(|message| message.is_system())
            .all(|message| !message.content().contains(CHILD_INSTRUCTION)),
        "技能全文只能作为 Human 输入"
    );
    assert_eq!(request.messages[0].content(), CHILD_SYSTEM);
    assert!(request
        .messages
        .iter()
        .any(|message| message.is_system() && message.content() == FROZEN_GUIDANCE));
    assert!(request.tool_names.iter().any(|name| name == "Read"));
    assert!(
        !request
            .tool_names
            .iter()
            .any(|name| name == "Write" || name == "Agent"),
        "遵守选定 agent 的工具约束"
    );
    let events = child_events.events.lock();
    assert!(!events.is_empty(), "真实子执行产生过程事件");
    assert!(events.iter().all(|event| matches!(event, AgentEvent::SubagentEvent { source_agent_id, .. } if source_agent_id == &agent_id)), "所有子事件都有来源信封，不能以主会话事件发出");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::SubagentEvent { event, .. } if matches!(event.as_ref(), AgentEvent::StateSnapshot(_)))), "子快照也必须保留来源");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::SubagentEvent { event, .. } if matches!(event.as_ref(), AgentEvent::TextChunk { chunk, .. } if chunk == CHILD_RESULT))), "子最终文本仅经来源信封输出");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::SubagentEvent { event, .. } if matches!(event.as_ref(), AgentEvent::LlmCallStart { messages, .. } if messages.iter().any(|message| message.content() == CHILD_INSTRUCTION)))), "信封处理器透传完整模型载荷需求");
}

#[tokio::test]
async fn test_invoke_skill_resolves_agent_model_defaults_and_explicit_override() {
    let fixture = make_skill_fixture();
    let observations = Arc::new(MockObservations::default());
    let runner = make_runner(
        &fixture,
        Arc::clone(&observations),
        MockResponse::Complete,
        None,
    );
    runner
        .invoke_skill(
            &fixture.skill_name,
            CHILD_INSTRUCTION,
            Some(&fixture.agent_name),
            None,
        )
        .await
        .expect("使用 agent 默认模型");
    runner
        .invoke_skill(
            &fixture.skill_name,
            CHILD_INSTRUCTION,
            Some(&fixture.agent_name),
            Some("override-model"),
        )
        .await
        .expect("技能覆盖 agent 默认模型");
    runner
        .invoke_skill(&fixture.skill_name, CHILD_INSTRUCTION, None, None)
        .await
        .expect("未指定 agent 时使用 general-purpose");
    assert_eq!(
        &*observations.model_aliases.lock(),
        &[
            Some("definition-model".to_string()),
            Some("override-model".to_string()),
            None
        ],
        "显式模型优先，其次 agent 配置，否则继承主模型"
    );
    let personas = observations.personas.lock();
    assert!(personas[0]
        .as_deref()
        .is_some_and(|persona| persona.contains(AGENT_PERSONA)));
    assert_eq!(personas[0], personas[1], "模型覆盖不改变选定 agent");
    let built_in = get_built_in_agent("general-purpose").expect("内置通用 agent 存在");
    let definition =
        crate::claude_agent_parser::parse_agent_file(built_in.content).expect("内置 agent 可解析");
    assert_eq!(
        personas[2].as_deref(),
        Some(definition.system_prompt.trim()),
        "默认选择通用 agent 的 persona"
    );
    let requests = observations.requests.lock();
    assert!(!requests[0].tool_names.iter().any(|name| name == "Write"));
    assert!(
        requests[2].tool_names.iter().any(|name| name == "Write"),
        "默认通用 agent 继承父工具"
    );
}

#[tokio::test]
async fn test_invoke_skill_cascades_cancellation_and_deregisters_running_child() {
    let fixture = make_skill_fixture();
    let observations = Arc::new(MockObservations::default());
    let callbacks = Arc::new(MockRuntimeCallbacks::default());
    let entered = Arc::new(Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let parent_cancel = AgentCancellationToken::new();
    let runner = make_runtime_tracked_runner(
        make_runner(
            &fixture,
            observations,
            MockResponse::Wait {
                entered: Arc::clone(&entered),
                release: Arc::new(Notify::new()),
                dropped: Arc::clone(&dropped),
            },
            None,
        )
        .with_cancel(parent_cancel.clone()),
        Arc::clone(&callbacks),
    );
    let skill_name = fixture.skill_name.clone();
    let agent_name = fixture.agent_name.clone();
    let mut execution = tokio::spawn(async move {
        runner
            .invoke_skill(&skill_name, CHILD_INSTRUCTION, Some(&agent_name), None)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("先确认子模型正在执行");
    assert!(!execution.is_finished(), "取消前仍等待子结果");
    parent_cancel.cancel();
    let error = tokio::time::timeout(Duration::from_secs(5), &mut execution)
        .await
        .expect("父取消应中断同步 Skill")
        .expect("执行任务正常退出")
        .expect_err("取消必须返回错误");
    assert!(
        matches!(
            error.downcast_ref::<AgentError>(),
            Some(AgentError::Interrupted)
        ),
        "保留真实中断类型"
    );
    assert!(
        dropped.load(Ordering::Relaxed),
        "取消时释放未完成模型 future"
    );
    let registrations = callbacks.registrations.lock();
    assert_eq!(registrations.len(), 1);
    assert_eq!(registrations[0].2, "cascade");
    assert!(registrations[0].1.is_cancelled(), "父取消传递到真实子令牌");
    assert_eq!(
        &*callbacks.deregistrations.lock(),
        &[registrations[0].0.clone()],
        "中断路径也只注销一次"
    );
}

#[tokio::test]
async fn test_invoke_skill_deregisters_child_after_model_failure() {
    let fixture = make_skill_fixture();
    let callbacks = Arc::new(MockRuntimeCallbacks::default());
    let runner = make_runtime_tracked_runner(
        make_runner(
            &fixture,
            Arc::new(MockObservations::default()),
            MockResponse::Fail,
            None,
        ),
        Arc::clone(&callbacks),
    );
    let error = runner
        .invoke_skill(
            &fixture.skill_name,
            CHILD_INSTRUCTION,
            Some(&fixture.agent_name),
            None,
        )
        .await
        .expect_err("模型失败不能返回成功回执");
    assert!(
        error.to_string().contains("offline-model-failure"),
        "保留真实错误原因"
    );
    let registrations = callbacks.registrations.lock();
    assert_eq!(registrations.len(), 1);
    assert_eq!(
        &*callbacks.deregistrations.lock(),
        &[registrations[0].0.clone()],
        "错误路径清理运行时注册"
    );
    assert!(registrations[0].1.is_cancelled(), "错误路径清理子令牌");
}

#[test]
fn test_skill_event_handler_wraps_all_payloads_and_preserves_source() {
    let recorder = Arc::new(MockEventRecorder::default());
    let handler = SkillEventHandler {
        agent_id: "isolated-child".to_string(),
        inner: recorder.clone(),
    };
    let events = vec![
        AgentEvent::MessageAdded(BaseMessage::human(CHILD_INSTRUCTION)),
        AgentEvent::StateSnapshot(vec![BaseMessage::ai(CHILD_RESULT)]),
        AgentEvent::TextChunk {
            message_id: cc_agent::messages::MessageId::new(),
            chunk: CHILD_RESULT.to_string(),
            source_agent_id: None,
        },
        AgentEvent::AgentExecutionFailed {
            message: "child-error".to_string(),
        },
    ];
    let expected: Vec<_> = events
        .iter()
        .map(|event| serde_json::to_value(event).expect("事件可序列化"))
        .collect();
    assert!(
        handler.wants_llm_call_payload(),
        "包装器保留内部处理器的载荷偏好"
    );
    for event in events {
        handler.on_event(event);
    }
    let emitted = recorder.events.lock();
    assert_eq!(emitted.len(), expected.len());
    for (event, expected) in emitted.iter().zip(expected) {
        let AgentEvent::SubagentEvent {
            source_agent_id,
            event,
        } = event
        else {
            panic!("子事件不能以裸主事件泄漏到主会话");
        };
        assert_eq!(source_agent_id, "isolated-child");
        assert_eq!(
            serde_json::to_value(event).expect("事件可序列化"),
            expected,
            "信封内原始载荷保持完整"
        );
    }
}
