use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cc_agent::{
    agent::react::{ReactLLM, Reasoning},
    interaction::{
        ApprovalDecision, InteractionContext, InteractionResponse, UserInteractionBroker,
    },
    llm::{
        types::{LlmRequest, LlmResponse, StopReason, StreamingContext},
        BaseModel,
    },
    messages::BaseMessage,
    tools::BaseTool,
};
use serde_json::{json, Value};

use super::*;
use crate::hitl::{
    default_requires_approval,
    jev::{
        config::JevConfig,
        rules::{empty_slot, JevRuleLoader},
        JevGate,
    },
    AutoClassifier, Classification, PermissionMode, SharedPermissionMode,
};

struct MockBroker {
    calls: AtomicUsize,
}

#[async_trait]
impl UserInteractionBroker for MockBroker {
    async fn request(&self, _context: InteractionContext) -> InteractionResponse {
        self.calls.fetch_add(1, Ordering::Relaxed);
        InteractionResponse::Decisions(vec![ApprovalDecision::Approve { source: None }])
    }
}

struct MockClassifier(Classification);

#[async_trait]
impl AutoClassifier for MockClassifier {
    async fn classify(&self, _tool: &str, _input: &Value) -> Classification {
        self.0
    }
}

struct MockTargetClassifier(parking_lot::Mutex<Vec<(String, Value)>>);

#[async_trait]
impl AutoClassifier for MockTargetClassifier {
    async fn classify(&self, tool: &str, input: &Value) -> Classification {
        self.0.lock().push((tool.to_string(), input.clone()));
        Classification::Allow
    }
}

fn make_call(name: &str, input: Value) -> ToolCall {
    ToolCall::new("permission-test", name, input)
}

fn make_parent(
    classification: Option<Classification>,
    gate: Option<Arc<JevGate>>,
) -> (
    HumanInTheLoopMiddleware,
    Arc<MockBroker>,
    Arc<SharedPermissionMode>,
) {
    let broker = Arc::new(MockBroker {
        calls: AtomicUsize::new(0),
    });
    let mode = SharedPermissionMode::new(PermissionMode::AutoMode);
    let parent = HumanInTheLoopMiddleware::with_shared_mode(
        broker.clone(),
        default_requires_approval,
        mode.clone(),
        classification.map(|value| Arc::new(MockClassifier(value)) as Arc<dyn AutoClassifier>),
        gate,
    );
    (parent, broker, mode)
}

fn make_permission(parent: &HumanInTheLoopMiddleware) -> SubAgentPermissionMiddleware {
    SubAgentPermissionMiddleware::new(
        Arc::new(parent.for_subagent()),
        Vec::new(),
        Vec::new(),
        vec![
            "Bash".to_string(),
            "Write".to_string(),
            "Read".to_string(),
            "Agent".to_string(),
            "ExecuteExtraTool".to_string(),
            "mcp__service__write".to_string(),
            "TodoWrite".to_string(),
        ],
        "permission-test".to_string(),
    )
}

#[tokio::test]
async fn test_child_denies_unsure_without_requesting_parent_broker() {
    let (parent, broker, _) = make_parent(Some(Classification::Unsure), None);
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    let call = make_call("Write", json!({"file_path": "file.txt"}));
    assert!(
        child.before_tool(&mut state, &call).await.is_err(),
        "子 Agent 不确定必须拒绝"
    );
    assert_eq!(
        broker.calls.load(Ordering::Relaxed),
        0,
        "子 Agent 不应请求父审批"
    );
    assert!(
        parent.before_tool(&mut state, &call).await.is_ok(),
        "父 Agent 保留人工审批"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_child_preserves_explicit_classifier_allow_and_deny() {
    let call = make_call("mcp__service__write", json!({}));
    let mut state = AgentState::new("permission-test");
    for (classification, allowed) in [(Classification::Allow, true), (Classification::Deny, false)]
    {
        let (parent, broker, _) = make_parent(Some(classification), None);
        assert_eq!(
            make_permission(&parent)
                .before_tool(&mut state, &call)
                .await
                .is_ok(),
            allowed
        );
        assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
    }
}

#[tokio::test]
async fn test_child_observes_parent_mode_changes_without_rebuilding() {
    let (parent, broker, mode) = make_parent(Some(Classification::Unsure), None);
    mode.store(PermissionMode::Bypass);
    let child = make_permission(&parent);
    let call = make_call("Write", json!({"file_path": "file.txt"}));
    let mut state = AgentState::new("permission-test");
    assert!(child.before_tool(&mut state, &call).await.is_ok());
    mode.store(PermissionMode::AutoMode);
    assert!(
        child.before_tool(&mut state, &call).await.is_err(),
        "缓存子工具必须实时恢复 Auto"
    );
    mode.store(PermissionMode::Bypass);
    assert!(child.before_tool(&mut state, &call).await.is_ok());
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_child_applies_parent_command_allow_and_deny_rules() {
    let gate = JevGate::new(JevConfig {
        api_key_env: format!("JEV_PERMISSION_UNSET_{}", uuid::Uuid::new_v4()),
        allowed_commands: vec!["cargo build*".to_string()],
        disallowed_commands: vec!["kubectl delete*".to_string()],
        ..Default::default()
    })
    .unwrap();
    let (parent, _, _) = make_parent(Some(Classification::Allow), Some(gate));
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    assert!(child
        .before_tool(
            &mut state,
            &make_call("Bash", json!({"command":"cargo build"}))
        )
        .await
        .is_ok());
    assert!(child
        .before_tool(
            &mut state,
            &make_call("Bash", json!({"command":"kubectl delete pod x"}))
        )
        .await
        .is_err());
}

#[tokio::test]
async fn test_child_default_auto_denies_sensitive_tools_without_configuration() {
    let child = SubAgentPermissionMiddleware::new(
        Arc::new(HumanInTheLoopMiddleware::subagent_default()),
        vec![],
        vec![],
        [
            "Bash",
            "Write",
            "Edit",
            "mcp__service__write",
            "WebFetch",
            "Read",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect(),
        "permission-test".to_string(),
    );
    let mut state = AgentState::new("permission-test");
    for name in ["Bash", "Write", "Edit", "mcp__service__write", "WebFetch"] {
        assert!(
            child
                .before_tool(&mut state, &make_call(name, json!({})))
                .await
                .is_err(),
            "未配置子权限必须拒绝 {name}"
        );
    }
    assert!(child
        .before_tool(&mut state, &make_call("Read", json!({})))
        .await
        .is_ok());
}

#[tokio::test]
async fn test_child_explicit_disabled_parent_preserves_bypass() {
    let child = make_permission(&HumanInTheLoopMiddleware::disabled());
    let mut state = AgentState::new("permission-test");
    assert!(child
        .before_tool(&mut state, &make_call("Write", json!({})))
        .await
        .is_ok());
}

#[tokio::test]
async fn test_child_deferred_lowercase_target_uses_actual_sensitive_name() {
    let (parent, _, _) = make_parent(Some(Classification::Unsure), None);
    let mut state = AgentState::new("permission-test");
    assert!(
        make_permission(&parent)
            .before_tool(
                &mut state,
                &make_call(
                    "ExecuteExtraTool",
                    json!({"tool_name":"bash","params":{"command":"cargo build"}})
                )
            )
            .await
            .is_err(),
        "小写代理名不能跳过 Bash 判定"
    );
    let (allow_parent, _, _) = make_parent(Some(Classification::Allow), None);
    let result = make_permission(&allow_parent)
        .before_tool(
            &mut state,
            &make_call("ExecuteExtraTool", json!({"tool_name":"bash","params":{}})),
        )
        .await;
    assert!(result.is_ok());
    assert_eq!(result.expect("已断言调用允许").input["tool_name"], "Bash");
}

#[tokio::test]
async fn test_child_deferred_classifier_receives_actual_target_and_parameters() {
    let classifier = Arc::new(MockTargetClassifier(parking_lot::Mutex::new(Vec::new())));
    let broker = Arc::new(MockBroker {
        calls: AtomicUsize::new(0),
    });
    let parent = HumanInTheLoopMiddleware::with_shared_mode(
        broker.clone(),
        default_requires_approval,
        SharedPermissionMode::new(PermissionMode::AutoMode),
        Some(classifier.clone()),
        None,
    );
    let mut state = AgentState::new("permission-test");
    assert!(make_permission(&parent)
        .before_tool(
            &mut state,
            &make_call(
                "ExecuteExtraTool",
                json!({"tool_name":"write","params":{"file_path":"file.txt"}})
            )
        )
        .await
        .is_ok());
    assert_eq!(
        *classifier.0.lock(),
        vec![("Write".to_string(), json!({"file_path":"file.txt"}))],
        "分类器应判真实目标与参数"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_child_deferred_targets_obey_definition_allow_and_deny_lists() {
    let (parent, _, _) = make_parent(Some(Classification::Allow), None);
    let mut child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    child.disallowed = vec!["Write".to_string()];
    assert!(child
        .before_tool(
            &mut state,
            &make_call("ExecuteExtraTool", json!({"tool_name":"write","params":{}}))
        )
        .await
        .is_err());
    child.disallowed.clear();
    child.allowed = vec!["Read".to_string(), "ExecuteExtraTool".to_string()];
    assert!(child
        .before_tool(
            &mut state,
            &make_call("ExecuteExtraTool", json!({"tool_name":"Write","params":{}}))
        )
        .await
        .is_err());
    assert!(child
        .before_tool(
            &mut state,
            &make_call("ExecuteExtraTool", json!({"tool_name":"Read","params":{}}))
        )
        .await
        .is_ok());
    assert!(
        child
            .before_tool(&mut state, &make_call("TodoWrite", json!({})))
            .await
            .is_err(),
        "中间件注入工具也应遵守允许集合"
    );
}

#[tokio::test]
async fn test_child_rejects_uninherited_fuzzy_recursive_and_nested_targets() {
    let (parent, _, mode) = make_parent(Some(Classification::Allow), None);
    mode.store(PermissionMode::Bypass);
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    for target in [
        "mcp",
        "bash_alias",
        "Agent",
        "ExecuteExtraTool",
        "UninheritedTool",
    ] {
        assert!(
            child
                .before_tool(
                    &mut state,
                    &make_call(
                        "ExecuteExtraTool",
                        json!({"tool_name":target,"params":{"tool_name":"Write","params":{}}})
                    )
                )
                .await
                .is_err(),
            "权限模式不能扩大继承的工具集合: {target}"
        );
    }
    assert!(child
        .before_tool(&mut state, &make_call("Agent", json!({})))
        .await
        .is_err());
}

#[tokio::test]
async fn test_child_direct_tool_names_cannot_bypass_with_case_or_aliases() {
    let (parent, broker, _) = make_parent(Some(Classification::Unsure), None);
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    for name in ["bash", "BASH", "write", "shell", "task"] {
        assert!(
            child
                .before_tool(
                    &mut state,
                    &make_call(name, json!({"command":"cargo build"}))
                )
                .await
                .is_err(),
            "大小写或别名不得绕过权限: {name}"
        );
    }
    assert!(child
        .before_tool(&mut state, &make_call("read", json!({})))
        .await
        .is_ok());
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_child_batch_never_requests_parent_broker() {
    let (parent, broker, _) = make_parent(Some(Classification::Unsure), None);
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    let results = child
        .before_tools_batch(
            &mut state,
            &[
                make_call("Write", json!({})),
                make_call("Read", json!({})),
                make_call("mcp__service__write", json!({})),
            ],
        )
        .await;
    assert_eq!(
        results.iter().map(Result::is_ok).collect::<Vec<_>>(),
        vec![false, true, false]
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

struct MockRulesModel;

#[async_trait]
impl BaseModel for MockRulesModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        Ok(LlmResponse {
            message: BaseMessage::ai("invalid-rule-json"),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_id(&self) -> &str {
        "mock-rule-failure"
    }
}

struct MockBudgetRulesModel;

#[async_trait]
impl BaseModel for MockBudgetRulesModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        Ok(LlmResponse {
            message: BaseMessage::ai(r#"{"rules":[{"text":"禁止修改主分支","source":"project"}]}"#),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }
    fn provider_name(&self) -> &str {
        "mock"
    }
    fn model_id(&self) -> &str {
        "mock-rule-budget"
    }
}

#[tokio::test]
async fn test_child_rule_extraction_failure_denies_but_explicit_allow_survives() {
    let loader = Arc::new(JevRuleLoader::new(
        format!("permission-rule-{}", uuid::Uuid::new_v4()),
        Arc::new(MockRulesModel),
        Duration::from_secs(1),
        1000,
        1,
        empty_slot(),
    ));
    let gate = JevGate::with_loader(
        JevConfig {
            api_key_env: format!("JEV_PERMISSION_UNSET_{}", uuid::Uuid::new_v4()),
            allowed_commands: vec!["cargo build*".to_string()],
            ..Default::default()
        },
        Some(loader),
    )
    .unwrap();
    let (parent, broker, _) = make_parent(Some(Classification::Allow), Some(gate));
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    assert!(
        child
            .before_tool(
                &mut state,
                &make_call("Bash", json!({"command":"git status"}))
            )
            .await
            .is_err(),
        "规则提炼失败不能自动执行"
    );
    assert!(
        child
            .before_tool(
                &mut state,
                &make_call("Bash", json!({"command":"cargo build"}))
            )
            .await
            .is_ok(),
        "人显式授权仍生效"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_child_incomplete_rules_deny_parent_readonly_allow_but_keep_explicit_allow() {
    let source = format!(
        "first-source-{}\n\nsecond-source-{}",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4()
    );
    let loader = Arc::new(JevRuleLoader::new(
        source,
        Arc::new(MockBudgetRulesModel),
        Duration::from_secs(1),
        80,
        1,
        empty_slot(),
    ));
    let gate = JevGate::with_loader(
        JevConfig {
            api_key_env: format!("JEV_PERMISSION_UNSET_{}", uuid::Uuid::new_v4()),
            allowed_commands: vec!["cargo build*".to_string()],
            ..Default::default()
        },
        Some(loader.clone()),
    )
    .unwrap();
    let (parent, broker, _) = make_parent(Some(Classification::Allow), Some(gate));
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    let call = make_call("Bash", json!({"command":"git status"}));
    assert!(
        parent.before_tool(&mut state, &call).await.is_ok(),
        "父保留部分提炼结果的旧容错"
    );
    assert!(
        loader.rules_incomplete(),
        "真实Mock提炼必须确认来源因预算丢失"
    );
    assert!(!loader.rules_unavailable(), "整体失败与部分失败应区分");
    assert!(
        child.before_tool(&mut state, &call).await.is_err(),
        "来源不完整时子不能只靠只读捷径允许"
    );
    assert!(
        child
            .before_tool(
                &mut state,
                &make_call("Bash", json!({"command":"cargo build"}))
            )
            .await
            .is_ok(),
        "人明确授权仍生效"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

/// 本地 HTTP 桩，仅返回固定判定数据，不调用真实模型，也不修改全局配置文件。
struct MockJevEndpoint {
    api_key_env: String,
    endpoint: String,
    task: tokio::task::JoinHandle<()>,
    requests: Arc<parking_lot::Mutex<Vec<Value>>>,
}

impl Drop for MockJevEndpoint {
    fn drop(&mut self) {
        std::env::remove_var(&self.api_key_env);
        self.task.abort();
    }
}

async fn make_judge_endpoint(status: &str, body: Value) -> MockJevEndpoint {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let api_key_env = format!("JEV_PERMISSION_TEST_{}", uuid::Uuid::new_v4());
    std::env::set_var(&api_key_env, "local-mock-key");
    let body = body.to_string();
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured_requests = requests.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = [0_u8; 4096];
            let mut request = Vec::new();
            loop {
                match stream.read(&mut bytes).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => request.extend_from_slice(&bytes[..count]),
                }
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        if let Ok(body) =
                            serde_json::from_slice(&request[end + 4..end + 4 + length])
                        {
                            captured_requests.lock().push(body);
                        }
                        let _ = stream.write_all(response.as_bytes()).await;
                        break;
                    }
                }
            }
        }
    });
    MockJevEndpoint {
        api_key_env,
        endpoint,
        task,
        requests,
    }
}

#[tokio::test]
async fn test_child_uncertain_deny_overrides_parent_uncertain_allow() {
    let mut answers = serde_json::Map::new();
    for condition in crate::hitl::jev::conditions::CONDITIONS {
        answers.insert(
            condition.id.to_string(),
            json!({"noul": if condition.id == "local_scope" { 0.0 } else { 1.0 }}),
        );
    }
    let endpoint =
        make_judge_endpoint("200 OK", json!({"answers": answers, "model":"local-mock"})).await;
    let gate = JevGate::new(JevConfig {
        endpoint: endpoint.endpoint.clone(),
        api_key_env: endpoint.api_key_env.clone(),
        uncertain: crate::hitl::jev::config::UncertainPolicy::Allow,
        ..Default::default()
    })
    .unwrap();
    let (parent, broker, _) = make_parent(None, Some(gate));
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    let call = make_call("Bash", json!({"command":"cargo build"}));
    assert!(
        parent.before_tool(&mut state, &call).await.is_ok(),
        "父保留 uncertain=allow 配置"
    );
    assert!(
        child.before_tool(&mut state, &call).await.is_err(),
        "子判定不确定必须拒绝"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_child_judge_failure_denies_while_parent_requests_approval() {
    let endpoint =
        make_judge_endpoint("400 Bad Request", json!({"error":"local mock failure"})).await;
    let gate = JevGate::new(JevConfig {
        endpoint: endpoint.endpoint.clone(),
        api_key_env: endpoint.api_key_env.clone(),
        ..Default::default()
    })
    .unwrap();
    let (parent, broker, _) = make_parent(None, Some(gate));
    let child = make_permission(&parent);
    let mut state = AgentState::new("permission-test");
    let call = make_call("Bash", json!({"command":"cargo build"}));
    assert!(
        child.before_tool(&mut state, &call).await.is_err(),
        "接口失败时子 Agent 不应执行"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
    assert!(
        parent.before_tool(&mut state, &call).await.is_ok(),
        "父仍可请求人工确认"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_child_judge_uses_inherited_execution_directory_and_current_mode() {
    let parent_dir = tempfile::tempdir().unwrap();
    let child_dir = tempfile::tempdir().unwrap();
    for (dir, branch) in [
        (parent_dir.path(), "main"),
        (child_dir.path(), "feature/child"),
    ] {
        std::fs::create_dir(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    }
    let answers = crate::hitl::jev::conditions::CONDITIONS
        .iter()
        .map(|condition| (condition.id.to_string(), json!({"noul":1.0})))
        .collect::<serde_json::Map<String, Value>>();
    let endpoint = make_judge_endpoint("200 OK", json!({"answers":answers})).await;
    let gate = JevGate::new(JevConfig {
        endpoint: endpoint.endpoint.clone(),
        api_key_env: endpoint.api_key_env.clone(),
        ..Default::default()
    })
    .unwrap();
    let (parent, broker, mode) = make_parent(None, Some(gate));
    let mut child = make_permission(&parent);
    child.execution_cwd = parent_dir.path().to_string_lossy().into_owned();
    let mut state = AgentState::new(child_dir.path().to_string_lossy().into_owned());
    let call = make_call("Bash", json!({"command":"git commit -m permission-test"}));
    assert!(child.before_tool(&mut state, &call).await.is_ok());
    assert_eq!(endpoint.requests.lock().len(), 1);
    assert_eq!(
        endpoint.requests.lock()[0]["state"]["context"]["repository"]["branch"],
        "main",
        "子指引目录不能替换工具实际执行分支"
    );
    assert_eq!(
        endpoint.requests.lock()[0]["state"]["context"]["repository"]["cwd"],
        parent_dir.path().to_string_lossy().as_ref()
    );
    assert_eq!(
        cc_agent::agent::state::State::cwd(&state),
        child_dir.path().to_string_lossy().as_ref(),
        "子状态仍保留指引目录"
    );
    mode.store(PermissionMode::Bypass);
    assert!(child.before_tool(&mut state, &call).await.is_ok());
    assert_eq!(endpoint.requests.lock().len(), 1, "Bypass 仍按配置免判定");
    mode.store(PermissionMode::AutoMode);
    assert!(child.before_tool(&mut state, &call).await.is_ok());
    assert_eq!(
        endpoint.requests.lock().len(),
        2,
        "共享模式切回 Auto 后应重新判定"
    );
    assert_eq!(
        endpoint.requests.lock()[1]["state"]["context"]["repository"]["branch"],
        "main"
    );
    assert_eq!(broker.calls.load(Ordering::Relaxed), 0);
}

struct MockCountingTool(Arc<AtomicUsize>);

#[async_trait]
impl BaseTool for MockCountingTool {
    fn name(&self) -> &str {
        "Write"
    }
    fn description(&self) -> &str {
        "Mock tool"
    }
    fn parameters(&self) -> Value {
        json!({"type":"object"})
    }
    async fn invoke(
        &self,
        _input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok("executed".to_string())
    }
}

struct MockToolCallingModel(AtomicUsize);

#[async_trait]
impl ReactLLM for MockToolCallingModel {
    async fn generate_reasoning(
        &self,
        _messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        if self.0.fetch_add(1, Ordering::Relaxed) == 0 {
            Ok(Reasoning::with_tools(
                "",
                vec![make_call(
                    "Write",
                    json!({"file_path":"permission-test.txt"}),
                )],
            ))
        } else {
            Ok(Reasoning::with_answer("", "finished"))
        }
    }
}

async fn make_execution_count(
    fork: bool,
    background: bool,
    permissions: Option<Arc<HumanInTheLoopMiddleware>>,
) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut tool = super::super::SubAgentTool::new(
        Arc::new(vec![
            Arc::new(MockCountingTool(calls.clone())) as Arc<dyn BaseTool>
        ]),
        None,
        Arc::new(|_| {
            Box::new(MockToolCallingModel(AtomicUsize::new(0))) as Box<dyn ReactLLM + Send + Sync>
        }),
        cwd.to_string(),
    )
    .with_parent_messages(Arc::new(parking_lot::RwLock::new(vec![])))
    .with_background_registry(Arc::new(
        crate::subagent::background::BackgroundTaskRegistry::new(sender),
    ))
    .with_inherited_instructions(Some(crate::subagent::InheritedInstructions {
        cwd: Arc::from(cwd),
        rendered: Arc::from(""),
    }));
    if let Some(permissions) = permissions {
        tool = tool.with_permissions(permissions);
    }
    let result = tool.invoke(json!({"subagent_type":"general-purpose","prompt":"test permissions","fork":fork,"run_in_background":background})).await;
    assert!(
        result.is_ok(),
        "路径构造不应失败: fork={fork}, background={background}: {result:?}"
    );
    if background {
        assert!(
            matches!(
                tokio::time::timeout(Duration::from_secs(3), receiver.recv()).await,
                Ok(Some(_))
            ),
            "后台应完成且不等待审批"
        );
    }
    calls.load(Ordering::Relaxed)
}

#[tokio::test]
async fn test_all_child_execution_paths_enforce_default_auto_permissions() {
    for (fork, background) in [(false, false), (true, false), (false, true), (true, true)] {
        assert_eq!(
            make_execution_count(fork, background, None).await,
            0,
            "默认未授权工具不应执行: fork={fork}, background={background}"
        );
    }
}

#[tokio::test]
async fn test_all_child_execution_paths_preserve_parent_auto_allow() {
    let (parent, broker, _) = make_parent(Some(Classification::Allow), None);
    let permissions = Arc::new(parent.for_subagent());
    for (fork, background) in [(false, false), (true, false), (false, true), (true, true)] {
        assert_eq!(
            make_execution_count(fork, background, Some(permissions.clone())).await,
            1,
            "父明确允许应传递到真实执行: fork={fork}, background={background}"
        );
    }
    assert_eq!(
        broker.calls.load(Ordering::Relaxed),
        0,
        "四种子执行路径均不应请求审批"
    );
}
