//! 显式运行的真实模型验收；默认跳过，不在 CI 中读取凭据或产生模型费用。

use super::*;
use cc_agent::{
    agent::{
        compact::CompactConfig, events::AgentEventHandler, react::ToolCall, state::State,
        token::ContextBudget,
    },
    error::{AgentError, AgentResult},
    llm::{types::TokenUsage, BaseModel, LlmRequest, LlmResponse},
    middleware::r#trait::Middleware,
};
use std::{path::PathBuf, time::Instant};

#[derive(Default)]
struct LiveEvidence {
    events: Mutex<Vec<AgentEvent>>,
    compact_usage: Mutex<Vec<Option<TokenUsage>>>,
}

impl AgentEventHandler for LiveEvidence {
    fn on_event(&self, event: AgentEvent) {
        self.events.lock().push(event);
    }

    fn wants_llm_call_payload(&self) -> bool {
        true
    }
}

struct LiveCompactModel {
    model: Box<dyn BaseModel>,
    evidence: Arc<LiveEvidence>,
}

#[async_trait]
impl BaseModel for LiveCompactModel {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let response = self.model.invoke(request).await?;
        self.evidence
            .compact_usage
            .lock()
            .push(response.usage.clone());
        Ok(response)
    }

    fn provider_name(&self) -> &str {
        self.model.provider_name()
    }

    fn model_id(&self) -> &str {
        self.model.model_id()
    }
}

struct LiveFixtureGuard {
    directory: PathBuf,
}

#[async_trait]
impl Middleware<AgentState> for LiveFixtureGuard {
    fn name(&self) -> &str {
        "LiveFixtureGuard"
    }

    async fn before_tool(&self, _state: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        let path = call.input["file_path"]
            .as_str()
            .map(PathBuf::from)
            .filter(|_| matches!(call.name.as_str(), "Read" | "Write" | "Edit"))
            .ok_or_else(|| AgentError::Other(anyhow::anyhow!("真实测试仅允许文件工具")))?;
        let path = if path.is_absolute() {
            path
        } else {
            self.directory.join(path)
        };
        let resolved = if path.exists() {
            path.canonicalize().map_err(anyhow::Error::from)?
        } else {
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("测试文件必须有父目录"))?
                .canonicalize()
                .map_err(anyhow::Error::from)?
                .join(
                    path.file_name()
                        .ok_or_else(|| anyhow::anyhow!("缺少文件名"))?,
                )
        };
        if !resolved.starts_with(&self.directory) {
            return Err(AgentError::Other(anyhow::anyhow!(
                "真实测试禁止访问隔离目录外的文件"
            )));
        }
        Ok(call.clone())
    }
}

fn make_live_tool_filter(name: &str) -> bool {
    !matches!(name, "Read" | "Write" | "Edit")
}

fn make_live_fixture_contents(directory: &Path) -> HashMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(directory)
        .expect("应能枚举隔离文件")
        .map(|entry| {
            let path = entry.expect("应能读取目录项").path();
            let bytes = std::fs::read(&path).expect("应能读取测试文件");
            (path, bytes)
        })
        .collect()
}

#[tokio::test]
#[ignore = "使用本机配置的真实模型并产生费用；仅显式 --ignored 运行"]
async fn test_memory_retrieval_and_explicit_save_with_live_configured_agent() {
    let started = Instant::now();
    let settings_path = std::env::var_os("PERI_LIVE_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or_else(crate::provider::config_path);
    let settings_before = std::fs::read(&settings_path).expect("本机应已配置 Agent");
    let mut settings = crate::provider::load_from(&settings_path).expect("应能加载本机配置");
    // 只修改测试进程中的推理和输出预算，模型、协议与凭据沿用本机配置。
    settings.config.thinking = Some(crate::provider::ThinkingConfig {
        enabled: true,
        budget_tokens: 1024,
        effort: "low".to_string(),
        max_tokens: 2048,
    });
    let provider = LlmProvider::from_config(&settings).expect("本机应有可用 Provider");
    let directory = tempfile::tempdir().expect("应能创建隔离目录");
    let cwd = directory.path().canonicalize().expect("应能解析隔离目录");
    let memory_dir = cwd.join("memory");
    std::fs::create_dir(&memory_dir).expect("应能创建隔离记忆目录");
    let alpha = format!("alpha_{}", uuid::Uuid::new_v4().simple());
    let beta = format!("beta_{}", uuid::Uuid::new_v4().simple());
    let gamma = format!("gamma_{}", uuid::Uuid::new_v4().simple());
    let index = "LIVE_MEMORY_INDEX_732\n- alpha.md: Alpha 项目的发布标记。\n- beta.md: Beta 项目的验收标记。\n";
    std::fs::write(memory_dir.join("MEMORY.md"), index).expect("应能创建记忆索引");
    std::fs::write(
        memory_dir.join("alpha.md"),
        format!("Alpha 发布标记：{alpha}\n"),
    )
    .expect("应能创建 Alpha 记忆");
    std::fs::write(
        memory_dir.join("beta.md"),
        format!("Beta 验收标记：{beta}\n"),
    )
    .expect("应能创建 Beta 记忆");
    let original_files = make_live_fixture_contents(&memory_dir);
    let snapshot = crate::prompt::AutoMemorySnapshot {
        directory: memory_dir.clone(),
        display_directory: memory_dir.to_string_lossy().replace('\\', "/"),
        content: cc_middlewares::memory::read_and_truncate_entrypoint(
            &memory_dir.join("MEMORY.md"),
        )
        .expect("应通过生产读取逻辑捕获索引")
        .content,
    };
    let mut features = crate::prompt::PromptFeatures::none();
    features.memory_enabled = true;
    let system_prompt = crate::prompt::build_system_prompt_with_memory(
        None,
        &cwd.to_string_lossy(),
        features,
        &[],
        Some("2026-10-27"),
        Some("zh-CN"),
        Some(&snapshot),
    );
    let evidence = Arc::new(LiveEvidence::default());
    let pool = Arc::new(Mutex::new(AgentPool::new()));
    let mut state = AgentState::new(cwd.to_string_lossy());
    let mut answers = Vec::new();
    let prompts = [
        "Alpha 项目的发布标记是什么？请从已有记忆确认，只回答标记。".to_string(),
        "Beta 项目的验收标记是什么？请从已有记忆确认，只回答标记。".to_string(),
        "我偏好简洁回答，这轮只是临时聊天。请只回复收到。".to_string(),
        format!("请长期记住 Gamma 项目的发布标记是 {gamma}。请保存到记忆目录中的 gamma.md，并在 MEMORY.md 添加短索引，保留原有索引项。"),
    ];
    let mut compact_events = Vec::new();
    let mut beta_request_start = 0;
    let mut beta_request_end = 0;
    let mut unsolicited_events_end = 0;
    for (phase, prompt) in prompts.into_iter().enumerate() {
        let mut config = make_agent_config(
            &cwd,
            provider.clone(),
            system_prompt.clone(),
            Some(snapshot.rendered_entrypoint()),
        );
        config.event_handler = evidence.clone();
        config.peri_config = Arc::new(settings.clone());
        config.frozen_instructions =
            Some("LIVE_PROJECT_RULE_732: Only access the isolated test directory.".to_string());
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        if phase == 1 {
            assert!(!state
                .messages()
                .iter()
                .any(|message| message.content().contains(&beta)));
            assert!(state.token_tracker().estimated_context_tokens().is_some());
            beta_request_start = evidence.events.lock().len();
            // 用真实上一轮 usage 和缩小的测试预算触发生产自动压缩，无伪造响应。
            config.compact_config = Some(CompactConfig {
                summary_max_tokens: 2048,
                ..CompactConfig::default()
            });
            config.compact_budget = Some(ContextBudget::new(1));
            config.compact_model = Some(Arc::new(LiveCompactModel {
                model: provider.clone().into_model(),
                evidence: evidence.clone(),
            }));
            config.compact_event_tx = Some(Arc::new(std::sync::Mutex::new(Some(event_tx))));
        }
        let cancel = config.cancel.clone();
        let (mut agent, _) = build_agent_with_embedded_memory(config, None, &pool);
        agent.executor = agent
            .executor
            .max_iterations(6)
            .with_tool_filter(make_live_tool_filter)
            .add_middleware(Box::new(LiveFixtureGuard {
                directory: cwd.clone(),
            }));
        let output = tokio::time::timeout(
            Duration::from_secs(180),
            agent
                .executor
                .execute(AgentInput::text(prompt), &mut state, Some(cancel)),
        )
        .await
        .expect("真实 Agent 应在预算内完成")
        .expect("真实 Agent 应执行成功");
        assert!(output.stop_reason.is_none(), "不应耗尽循环次数");
        answers.push(output.text);
        if phase == 1 {
            beta_request_end = evidence.events.lock().len();
        }
        if let Some(path) = std::env::var_os("PERI_LIVE_REPORT") {
            let progress = json!({"completed_phase": phase + 1, "answers": answers,
                "elapsed_seconds": started.elapsed().as_secs_f64()});
            std::fs::write(
                PathBuf::from(path).with_extension("progress.json"),
                progress.to_string(),
            )
            .expect("应能保存脱敏进度");
        }
        compact_events.extend(std::iter::from_fn(|| event_rx.try_recv().ok()));
        if phase < 3 {
            assert_eq!(
                make_live_fixture_contents(&memory_dir),
                original_files,
                "普通问答不得自动写记忆"
            );
            unsolicited_events_end = evidence.events.lock().len();
        }
        assert!(
            !state
                .messages()
                .iter()
                .any(|message| message.is_system() && message.content().starts_with(&system_prompt)),
            "临时冻结提示词不能残留到历史，正常读取索引的工具历史应保留"
        );
    }
    assert!(answers[0].contains(&alpha), "压缩前应读到随机记忆标记");
    assert!(
        answers[1].contains(&beta),
        "自动压缩后应读到此前未出现的记忆标记"
    );
    assert!(answers[2].contains("收到"));
    assert!(std::fs::read_to_string(memory_dir.join("gamma.md"))
        .expect("显式请求应保存主题文件")
        .contains(&gamma));
    let saved_index =
        std::fs::read_to_string(memory_dir.join("MEMORY.md")).expect("应能读取更新索引");
    assert!(
        saved_index.contains("gamma.md")
            && saved_index.contains("alpha.md")
            && saved_index.contains("beta.md"),
        "显式保存应更新索引并保留已有主题"
    );
    for name in ["alpha.md", "beta.md"] {
        let path = memory_dir.join(name);
        assert_eq!(
            std::fs::read(&path).expect("原有主题应保留"),
            original_files[&path],
            "显式保存不能覆盖原有主题"
        );
    }
    assert!(compact_events
        .iter()
        .any(|event| matches!(event, AgentEvent::CompactStarted { manual: false })));
    assert!(compact_events.iter().any(
        |event| matches!(event, AgentEvent::CompactCompleted { summary, .. } if !summary.is_empty())
    ));
    assert!(!compact_events
        .iter()
        .any(|event| matches!(event, AgentEvent::CompactError { .. })));
    let events = evidence.events.lock();
    let requests: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::LlmCallStart { messages, .. } => Some(messages),
            _ => None,
        })
        .collect();
    assert!(!requests.is_empty());
    let expected_system = requests[0]
        .iter()
        .find(|message| message.content().contains("LIVE_MEMORY_INDEX_732"))
        .expect("应注入记忆索引")
        .content();
    for messages in &requests {
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.is_system()
                    && message.content().contains("LIVE_MEMORY_INDEX_732"))
                .count(),
            1,
            "每次请求只注入一份索引"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.is_system() && message.content() == expected_system),
            "压缩和跨轮次均不得修改冻结提示词"
        );
    }
    let beta_events = &events[beta_request_start..beta_request_end];
    let beta_read = beta_events.iter().position(|event| matches!(event, AgentEvent::ToolStart { name, input, .. } if name == "Read" && input["file_path"].as_str().is_some_and(|path| path.ends_with("beta.md")))).expect("压缩后必须真正调用 Read 读取 Beta 主题");
    let AgentEvent::ToolStart {
        tool_call_id: beta_call_id,
        ..
    } = &beta_events[beta_read]
    else {
        unreachable!()
    };
    assert!(beta_events.iter().any(|event| matches!(event, AgentEvent::ToolEnd { tool_call_id, output, is_error: false, .. } if tool_call_id == beta_call_id && output.contains(&beta))), "Beta 主题必须实际读取成功");
    assert!(beta_events[..beta_read].iter().any(|event| matches!(event, AgentEvent::LlmCallStart { messages, .. } if messages.iter().any(|message| message.content().contains("This session continues an earlier conversation")))), "读取新主题前应已进入压缩后的上下文");
    let tools: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolStart { name, input, .. } => {
                Some(json!({"name": name, "file": input["file_path"]}))
            }
            _ => None,
        })
        .collect();
    assert!(
        events[..unsolicited_events_end]
            .iter()
            .all(|event| !matches!(event, AgentEvent::ToolStart { name, .. } if name != "Read")),
        "普通问答不能触发 Write/Edit"
    );
    let usage: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::LlmCallEnd {
                usage: Some(usage), ..
            } => Some(usage.clone()),
            _ => None,
        })
        .collect();
    let compact_usage = evidence.compact_usage.lock().clone();
    let report = json!({
        "model": provider.model_name(), "protocol": provider.display_name(),
        "elapsed_seconds": started.elapsed().as_secs_f64(),
        "main_calls": requests.len(), "compact_calls": compact_usage.len(),
        "main_usage": usage, "compact_usage": compact_usage,
        "tools": tools, "answers": answers,
        "checks": {"before_compact_read": true, "unseen_topic_after_auto_compact": true,
            "frozen_prompt_stable": true, "index_once_per_request": true,
            "no_unsolicited_memory_write": true, "temporary_prefix_cleaned": true,
            "explicit_save_updates_topic_and_index": true},
    });
    assert_eq!(
        std::fs::read(&settings_path).expect("应能复查本机配置"),
        settings_before,
        "测试不得修改用户配置"
    );
    if let Some(path) = std::env::var_os("PERI_LIVE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&report).expect("报告应能序列化"),
        )
        .expect("应能保存脱敏报告");
    }
    tracing::info!(report = %report, "真实模型记忆验收完成");
}
