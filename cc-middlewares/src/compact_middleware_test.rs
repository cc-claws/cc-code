use super::*;
use async_trait::async_trait;
use cc_agent::{
    agent::{state::AgentState, token::ContextBudget},
    llm::{LlmRequest, LlmResponse, StopReason},
    messages::{BaseMessage, ContentBlock},
};
use std::sync::Arc;

struct MockCompactModel;

#[async_trait]
impl BaseModel for MockCompactModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        Ok(LlmResponse {
            message: BaseMessage::ai("<summary>保留当前任务</summary>"),
            stop_reason: StopReason::EndTurn,
            usage: None,
            request_id: None,
        })
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-compact"
    }
}

struct MockFailedCompactModel;

#[async_trait]
impl BaseModel for MockFailedCompactModel {
    async fn invoke(&self, _request: LlmRequest) -> AgentResult<LlmResponse> {
        Err(cc_agent::error::AgentError::LlmError(
            "模拟摘要失败".to_string(),
        ))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-failed-compact"
    }
}

struct MockCancellingCompactModel {
    cancel: AgentCancellationToken,
}

#[async_trait]
impl BaseModel for MockCancellingCompactModel {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        self.cancel.cancel();
        MockCompactModel.invoke(request).await
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-cancelling-compact"
    }
}

fn make_state() -> AgentState {
    AgentState::new("/tmp/test")
}

fn make_config() -> CompactConfig {
    CompactConfig::default()
}

fn make_budget(context_window: u32) -> ContextBudget {
    ContextBudget::new(context_window)
}

fn make_event_tx() -> Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>> {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    Arc::new(Mutex::new(Some(tx)))
}

fn make_middleware() -> CompactMiddleware {
    CompactMiddleware {
        model: None,
        config: make_config(),
        budget: make_budget(200_000),
        cwd: "/tmp/test".to_string(),
        event_tx: make_event_tx(),
        cancel: AgentCancellationToken::default(),
        hooks: vec![],
        session_id: "test-session".to_string(),
        provider_name: "test-model".to_string(),
        micro_compact_done: AtomicBool::new(false),
        invoked_skills: InvokedSkillRegistry::default(),
    }
}

#[tokio::test]
async fn test_name_returns_compact_middleware() {
    let mw = make_middleware();
    assert_eq!(
        <CompactMiddleware as Middleware<AgentState>>::name(&mw),
        "CompactMiddleware"
    );
}

#[tokio::test]
async fn test_before_model_noop_when_disabled_by_env() {
    // 使用 config.auto_compact_enabled=false 模拟 disable（避免 env var 并行测试污染）
    let mw = CompactMiddleware {
        config: {
            let mut c = make_config();
            c.auto_compact_enabled = false;
            c
        },
        ..make_middleware()
    };
    let mut state = make_state();
    mw.before_model(&mut state).await.unwrap();
}

#[tokio::test]
async fn test_before_model_noop_when_config_disabled() {
    let mw = CompactMiddleware {
        config: {
            let mut c = make_config();
            c.auto_compact_enabled = false;
            c
        },
        ..make_middleware()
    };
    let mut state = make_state();
    mw.before_model(&mut state).await.unwrap();
}

#[tokio::test]
async fn test_before_model_noop_when_below_threshold() {
    // tracker 用量低，不触发任何 compact
    let mw = make_middleware();
    let mut state = make_state();
    mw.before_model(&mut state).await.unwrap();
}

#[tokio::test]
async fn test_before_model_with_low_budget_triggers_full_or_micro() {
    // budget 为 1000 token 且 tracker 已累积 → 应触发 compact
    let mut state = make_state();
    // 向 state 添加大量消息
    state.add_message(BaseMessage::human(vec![ContentBlock::text(
        "hello ".repeat(100),
    )]));

    let mw = CompactMiddleware {
        budget: ContextBudget::new(100), // 极小窗口
        model: None,                     // 无 model，full compact 会跳过
        ..make_middleware()
    };

    let result = mw.before_model(&mut state).await;
    // 无 model 时 full compact 返回 Ok 但跳过
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_compact_without_model_skips_full() {
    // 验证无 model 时 full compact 被跳过
    let mut state = make_state();
    state.add_message(BaseMessage::human(vec![ContentBlock::text("test message")]));

    let mw = CompactMiddleware {
        budget: ContextBudget::new(100),
        model: None,
        ..make_middleware()
    };

    let result = mw.before_model(&mut state).await;
    assert!(result.is_ok());
    // 无 model 时不应该 panic
}

#[tokio::test]
async fn test_borrow_safety_then_mut() {
    // 验证先读 tracker 后改 messages 的借用模式
    let mut state = make_state();
    state.add_message(BaseMessage::human(vec![ContentBlock::text("test")]));

    // 即使有低 budget，借用模式也不应 panic
    let mw = CompactMiddleware {
        budget: ContextBudget::new(1_000_000), // 大窗口，不触发
        ..make_middleware()
    };

    let result = mw.before_model(&mut state).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_disabled_detects_config() {
    let mw = make_middleware();
    // 默认情况 auto_compact_enabled=true，不应 disabled
    assert!(!mw.is_disabled());

    let mw = CompactMiddleware {
        config: {
            let mut c = make_config();
            c.auto_compact_enabled = false;
            c
        },
        ..make_middleware()
    };
    assert!(mw.is_disabled());
}

#[tokio::test]
async fn test_micro_compact_once_per_prompt() {
    // 验证 micro compact 在同一个 middleware 实例中只触发一次
    let mut state = make_state();
    // 添加足够的消息使 stale_steps 之外的工具有可压缩内容
    for i in 0..8 {
        state.add_message(BaseMessage::ai_with_tool_calls(
            cc_agent::messages::MessageContent::text("using tool"),
            vec![cc_agent::messages::ToolCallRequest::new(
                format!("tc{}", i),
                "Bash",
                serde_json::json!({}),
            )],
        ));
        state.add_message(BaseMessage::tool_result(
            format!("tc{}", i),
            "x".repeat(600),
        ));
    }

    // 设置 token tracker 使 should_warn() 返回 true
    // context_window=1000, input_tokens=800 → 80% > 70% (warning threshold)
    state
        .token_tracker_mut()
        .accumulate(&cc_agent::llm::types::TokenUsage {
            input_tokens: 800,
            output_tokens: 100,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            request_id: None,
        });

    // 极小 budget 使 should_warn() 返回 true（70% 阈值）
    let mut mw = CompactMiddleware {
        budget: ContextBudget::new(1000),
        config: {
            let mut c = make_config();
            c.micro_compact_stale_steps = 1;
            c
        },
        ..make_middleware()
    };

    // 第一次调用：micro compact 应该触发
    let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
    mw.event_tx = Arc::new(Mutex::new(Some(tx1)));

    mw.before_model(&mut state).await.unwrap();

    // 应收到 CompactCompleted 事件
    let event1 = rx1.try_recv();
    assert!(
        matches!(event1, Ok(ExecutorEvent::CompactCompleted { micro_cleared, .. }) if micro_cleared > 0),
        "第一次 micro compact 应触发并清理工具结果"
    );

    // 第二次调用：micro compact 不应再触发
    let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
    mw.event_tx = Arc::new(Mutex::new(Some(tx2)));

    mw.before_model(&mut state).await.unwrap();

    let event2 = rx2.try_recv();
    assert!(
        event2.is_err(),
        "第二次 micro compact 不应触发（once-per-prompt 守卫）"
    );

    // 确认标志已设置
    assert!(mw.micro_compact_done.load(Ordering::Relaxed));
}

fn make_memory_system_prefix() -> Vec<BaseMessage> {
    vec![
        BaseMessage::system("冻结系统规则\n# Memory\nMEMORY.md:\n中文记忆入口 MEMORY_SENTINEL"),
        BaseMessage::system("CLAUDE.md 项目指引\nPROJECT_SENTINEL"),
    ]
}

#[tokio::test]
async fn test_do_full_compact_preserves_system_prefix() {
    let prefix = make_memory_system_prefix();
    let mut messages = prefix.clone();
    messages.push(BaseMessage::human("继续当前任务"));
    let mut state = AgentState::with_messages("/tmp/test", messages);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok(), "生产压缩路径应执行成功");
    assert_eq!(
        state.messages().len(),
        3,
        "压缩后应有原始系统前缀和一条摘要"
    );
    for (actual, expected) in state.messages().iter().zip(&prefix) {
        assert_eq!(actual.id(), expected.id(), "系统前缀必须保留原始 ID");
        assert_eq!(
            actual.content(),
            expected.content(),
            "系统前缀的字节和顺序必须保持不变"
        );
    }
    assert!(matches!(state.messages()[2], BaseMessage::Human { .. }));
    assert!(state.messages()[2].content().contains("保留当前任务"));
}

#[tokio::test]
async fn test_do_full_compact_without_system_prefix_keeps_human_summary() {
    let mut state = AgentState::with_messages(
        "/tmp/test",
        vec![
            BaseMessage::human("继续当前任务"),
            BaseMessage::ai("此前进展"),
        ],
    );
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok(), "没有系统前缀时压缩仍应成功");
    assert_eq!(state.messages().len(), 1);
    assert!(matches!(state.messages()[0], BaseMessage::Human { .. }));
    assert!(state.messages()[0].content().contains("保留当前任务"));
}

#[tokio::test]
async fn test_do_full_compact_repeatedly_preserves_system_ids_without_duplicates() {
    let prefix = make_memory_system_prefix();
    let mut messages = prefix.clone();
    messages.push(BaseMessage::human("继续当前任务"));
    let mut state = AgentState::with_messages("/tmp/test", messages);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    for _ in 0..2 {
        let result = mw.do_full_compact(&mut state).await;
        assert!(result.is_ok(), "连续压缩应执行成功");
        assert_eq!(state.messages().len(), 3, "连续压缩不得累积系统前缀");
        for (actual, expected) in state.messages().iter().zip(&prefix) {
            assert_eq!(
                actual.id(),
                expected.id(),
                "连续压缩后 cleanup 仍须匹配原 ID"
            );
            assert_eq!(
                actual.content(),
                expected.content(),
                "连续压缩不得改变规则和索引"
            );
        }
        assert!(matches!(state.messages()[2], BaseMessage::Human { .. }));
    }
}

#[tokio::test]
async fn test_do_full_compact_preserves_system_prefix_across_ancestor_boundary() {
    let prefix = make_memory_system_prefix();
    let mut messages = prefix.clone();
    messages.push(BaseMessage::human("当前子代理任务"));
    let mut state = AgentState::with_messages("/tmp/test", messages).with_ancestor_len(1);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.ancestor_len(), 1, "祖先边界保持不变");
    assert_eq!(state.messages().len(), 3, "边界两侧的系统前缀各保留一次");
    for (actual, expected) in state.messages().iter().zip(&prefix) {
        assert_eq!(
            actual.id(),
            expected.id(),
            "前缀跨越祖先边界时不得重建或重复"
        );
        assert_eq!(actual.content(), expected.content());
    }
    assert!(matches!(state.messages()[2], BaseMessage::Human { .. }));
}

#[tokio::test]
async fn test_do_full_compact_does_not_restore_system_message_after_ancestor_history() {
    let prefix = BaseMessage::system("冻结系统规则 MEMORY_SENTINEL");
    let ancestor = BaseMessage::human("只读父代理历史");
    let historical = BaseMessage::system("旧文件快照 HISTORICAL_SENTINEL");
    let historical_id = historical.id();
    let ancestors = [prefix.clone(), ancestor.clone()];
    let mut state = AgentState::with_messages(
        "/tmp/test",
        vec![
            prefix,
            ancestor,
            historical,
            BaseMessage::human("当前子代理任务"),
        ],
    )
    .with_ancestor_len(2);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.ancestor_len(), 2);
    assert_eq!(state.messages().len(), 3);
    for (actual, expected) in state.messages().iter().zip(&ancestors) {
        assert_eq!(
            actual.id(),
            expected.id(),
            "祖先快照的 ID、内容和顺序必须保持不变"
        );
        assert_eq!(actual.content(), expected.content());
    }
    assert!(
        state
            .messages()
            .iter()
            .all(|message| message.id() != historical_id),
        "非全局前缀的历史 System 不得复活"
    );
    assert!(matches!(state.messages()[2], BaseMessage::Human { .. }));
}

#[tokio::test]
async fn test_do_full_compact_failure_restores_original_messages() {
    let mut messages = make_memory_system_prefix();
    messages.push(BaseMessage::human("当前任务与约束"));
    messages.push(BaseMessage::ai("尚未压缩的历史进展"));
    let original = serde_json::to_value(&messages).expect("应能保存原始消息快照");
    let mut state = AgentState::with_messages("/tmp/test", messages);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockFailedCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok(), "摘要失败后应恢复并允许原任务继续");
    assert_eq!(
        serde_json::to_value(state.messages()).expect("应能检查恢复后的消息"),
        original,
        "摘要失败后必须原样恢复系统前缀、消息 ID、顺序和历史正文"
    );
}

#[tokio::test]
async fn test_do_full_compact_cancellation_restores_original_messages() {
    let mut messages = make_memory_system_prefix();
    messages.push(BaseMessage::human("当前任务与约束"));
    let original = serde_json::to_value(&messages).expect("应能保存原始消息快照");
    let mut state = AgentState::with_messages("/tmp/test", messages);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    mw.cancel.cancel();
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(
        serde_json::to_value(state.messages()).expect("应能检查恢复后的消息"),
        original,
        "压缩取消后必须原样恢复系统前缀和历史，不得重复追加"
    );
}

#[tokio::test]
async fn test_do_full_compact_cancellation_after_summary_restores_original_messages() {
    let mut messages = make_memory_system_prefix();
    messages.push(BaseMessage::human("当前任务与约束"));
    let original = serde_json::to_value(&messages).expect("应能保存原始消息快照");
    let mut state = AgentState::with_messages("/tmp/test", messages);
    let cancel = AgentCancellationToken::default();
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCancellingCompactModel {
            cancel: cancel.clone(),
        })),
        cancel,
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(
        serde_json::to_value(state.messages()).expect("应能检查恢复后的消息"),
        original,
        "摘要已生成但恢复前被取消时，仍必须原样恢复全部消息"
    );
}

#[tokio::test]
async fn test_do_full_compact_event_excludes_system_prefix_and_keeps_recovered_file() {
    let directory = tempfile::tempdir().expect("应能创建隔离目录");
    let path = directory.path().join("context.txt");
    std::fs::write(&path, "恢复的文件正文 FILE_SENTINEL").expect("应能创建恢复文件");
    let mut messages = make_memory_system_prefix();
    messages.push(BaseMessage::human("读取文件后继续任务"));
    messages.push(BaseMessage::ai_with_tool_calls(
        cc_agent::messages::MessageContent::text("读取文件"),
        vec![cc_agent::messages::ToolCallRequest::new(
            "read-context",
            "Read",
            serde_json::json!({"path": path.to_string_lossy()}),
        )],
    ));
    messages.push(BaseMessage::tool_result("read-context", "先前文件正文"));
    let mut state = AgentState::with_messages(directory.path().to_string_lossy(), messages);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        cwd: directory.path().to_string_lossy().into_owned(),
        event_tx: Arc::new(Mutex::new(Some(tx))),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    let mut completed = None;
    while let Ok(event) = rx.try_recv() {
        if let ExecutorEvent::CompactCompleted { messages, .. } = event {
            completed = Some(messages);
        }
    }
    let completed = completed.expect("应产生压缩完成事件");
    assert_eq!(completed.len(), 2, "事件只包含摘要和恢复文件");
    assert!(matches!(completed[0], BaseMessage::Human { .. }));
    assert!(completed[1].is_system());
    assert!(completed[1].content().contains("FILE_SENTINEL"));
    assert!(
        completed.iter().all(|message| {
            !message.content().contains("MEMORY_SENTINEL")
                && !message.content().contains("PROJECT_SENTINEL")
        }),
        "临时系统规则和记忆入口不得泄漏至 UI 或持久化事件"
    );
    assert_eq!(
        state.messages().len(),
        4,
        "执行状态仍应保留两条原始系统前缀"
    );
    assert_eq!(
        serde_json::to_value(&state.messages()[2..]).expect("应能检查执行状态"),
        serde_json::to_value(&completed).expect("应能检查事件载荷"),
        "状态中的压缩历史应与事件一致，系统前缀独立保留"
    );
}

#[test]
fn test_compact_middleware_reset_micro_compact_flag() {
    let mw = make_middleware();
    // 设置标志为 true
    mw.micro_compact_done.store(true, Ordering::SeqCst);
    assert!(mw.micro_compact_done.load(Ordering::SeqCst));

    // reset 后应恢复为 false
    mw.reset();
    assert!(!mw.micro_compact_done.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_do_full_compact_keeps_registry_content_across_replaced_history() {
    use cc_agent::agent::compact::re_inject::skill_invocation_payload;
    let registry = InvokedSkillRegistry::default();
    let instruction = "完整规则\n".repeat(100);
    registry.record(&serde_json::json!({
        "success": true,
        "commandName": "review",
        "status": "inline",
        "instruction": instruction,
    }));
    let mut mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        config: CompactConfig {
            re_inject_max_tokens_per_file: 10,
            ..make_config()
        },
        ..make_middleware()
    }
    .with_invoked_skills(registry);
    let mut state = make_state();
    state.add_message(BaseMessage::human("执行审查"));
    let first = mw.do_full_compact(&mut state).await;
    assert!(first.is_ok());
    let first_text = state.messages()[1].content();
    let first_output: serde_json::Value = serde_json::from_str(
        skill_invocation_payload(&first_text).expect("compact 应恢复技能元消息"),
    )
    .expect("技能元消息应为合法 JSON");
    assert_eq!(
        first_output["instruction"]
            .as_str()
            .map(|text| text.chars().count()),
        Some(40)
    );
    mw.config.re_inject_max_tokens_per_file = 5000;
    let second = mw.do_full_compact(&mut state).await;
    assert!(second.is_ok());
    let second_text = state.messages()[1].content();
    let second_output: serde_json::Value = serde_json::from_str(
        skill_invocation_payload(&second_text).expect("再次 compact 应恢复技能元消息"),
    )
    .expect("技能元消息应为合法 JSON");
    assert_eq!(second_output["instruction"], serde_json::json!(instruction));
    assert!(state
        .messages()
        .iter()
        .all(|message| matches!(message, BaseMessage::Human { .. })));
}

#[tokio::test]
async fn test_do_full_compact_seeds_only_current_agent_messages() {
    use cc_agent::agent::compact::re_inject::skill_invocation_message;
    let parent = skill_invocation_message(&serde_json::json!({
        "success": true,
        "commandName": "parent",
        "status": "inline",
        "instruction": "父代理规则",
    }));
    let child = skill_invocation_message(&serde_json::json!({
        "success": true,
        "commandName": "child",
        "status": "inline",
        "instruction": "子代理规则",
    }));
    let parent_id = parent.id();
    let mut state =
        AgentState::with_messages("/tmp/test", vec![parent, child]).with_ancestor_len(1);
    let mw = CompactMiddleware {
        model: Some(Arc::new(MockCompactModel)),
        ..make_middleware()
    };
    let result = mw.do_full_compact(&mut state).await;
    assert!(result.is_ok());
    assert_eq!(state.messages()[0].id(), parent_id, "祖先快照应保持原样");
    assert_eq!(extract_skill_names(&state.messages()[1..]), vec!["child"]);
    let recovered = re_inject_with_registry(&[], &make_config(), "", &mw.invoked_skills).await;
    assert_eq!(
        extract_skill_names(&recovered.messages),
        vec!["child"],
        "registry 只应记录当前子代理调用的技能"
    );
}
