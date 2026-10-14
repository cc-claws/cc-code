//! SubAgent state tracking — token usage updates + subagent start events.
//! Extracted from original agent_ops.rs (2026-05-20 split).

use super::super::*;

use crate::app::App;

/// 等待态计数行的识别标记（PRD §4.4.2）。用带前缀与后缀的固定子串而非整行匹配：
/// 数量会变化（`1`/`N`），但首尾稳定，清理时才能命中上一帧写入的行；同时收窄到
/// 本功能专属措辞，避免误伤其他含 "Waiting for" 的系统消息。
const BG_WAITING_MARKER: &str = "✻ Waiting for ";
const BG_WAITING_SUFFIX: &str = " to finish";

impl App {
    pub(super) fn handle_token_usage_update(
        &mut self,
        usage: cc_agent::llm::types::TokenUsage,
    ) -> (bool, bool, bool) {
        // SubAgent 的 TokenUsageUpdate 不应污染父 agent 的 tracker

        // 累积到会话追踪器
        self.session_mgr
            .current_mut()
            .agent
            .session_token_tracker
            .accumulate(&usage);

        // 缓存率检查：当次命中率低于 80% 时显示黄色提示
        // 首轮请求缓存尚未创建，cache_creation 有值但 cache_read=0，0% 是正常行为
        // 如果 provider 不支持缓存（累计 cache_creation + cache_read 始终为 0），跳过警告
        // 限制：CLI 启动 1800 秒（30分钟）后才允许提醒，之后每 60 秒最多一次，避免频繁打扰
        let tracker = &self.session_mgr.current().agent.session_token_tracker;
        let should_check = tracker.llm_call_count > 1;
        let rate = tracker.cache_hit_rate();
        // provider 是否实际报告过缓存数据（cache_creation > 0 或 cache_read > 0）
        let has_cache_data =
            tracker.total_cache_creation_tokens > 0 || tracker.total_cache_read_tokens > 0;
        let now = std::time::Instant::now();
        // CLI 启动 300 秒后才允许提醒
        let runtime_long_enough = self
            .session_mgr
            .current()
            .agent
            .session_start_time
            .map(|t| now.duration_since(t).as_secs() >= 1800)
            .unwrap_or(true);
        let should_warn = should_check
            && has_cache_data
            && rate > 0.0
            && rate < 0.8
            && runtime_long_enough
            && self
                .session_mgr
                .current()
                .agent
                .last_cache_warning_at
                .map(|t| now.duration_since(t).as_secs() >= 60)
                .unwrap_or(true);
        if should_warn {
            self.session_mgr.current_mut().agent.last_cache_warning_at = Some(now);
            let tracker = &self.session_mgr.current_mut().agent.session_token_tracker;
            tracing::warn!(
                input = tracker.total_input_tokens,
                cache_read = tracker.total_cache_read_tokens,
                rate_pct = rate * 100.0,
                "prompt cache hit rate below threshold"
            );
            // 状态栏已常驻显示 Cache 百分比（低于 80% 时显示为黄色警告色），
            // 不再向消息区注入冗余的系统气泡打扰用户。
        }
        // 更新 spinner 的 token 显示（仅当次调用的 token，不累计）
        let current_tokens = usage.input_tokens as usize + usage.output_tokens as usize;
        self.session_mgr
            .current_mut()
            .spinner_state
            .set_token_count(current_tokens);
        (true, false, false)
    }

    pub(super) fn handle_subagent_start(
        &mut self,
        agent_id: String,
        instance_id: String,
        task_preview: String,
        is_background: bool,
        prompt: Option<String>,
    ) -> (bool, bool, bool) {
        if !self.session_mgr.current().subagent_sessions.contains_key(&instance_id) {
            let mut sub = super::super::subagent_session::BackgroundAgentSession::new(
                instance_id.clone(), agent_id.clone(), task_preview.clone(), self.services.cwd.clone(),
            );
            if let Some(ref p) = prompt {
                sub.prompt = p.clone();
            }
            self.session_mgr.current_mut().subagent_sessions.insert(instance_id.clone(), sub);
        } else if let Some(ref p) = prompt {
            if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(&instance_id) {
                if sub.prompt.is_empty() {
                    sub.prompt = p.clone();
                }
            }
        }
        if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(&instance_id) {
            sub.is_background = is_background;
        }
        if is_background && !self.session_mgr.current().background_agents.iter().any(|a| a.instance_id == instance_id) {
            use super::super::chat_session::RunningBgAgent;
            self.session_mgr
                .current_mut()
                .background_agents
                .push(RunningBgAgent {
                    agent_name: agent_id.clone(),
                    instance_id: instance_id.clone(),
                    started_at: std::time::Instant::now(),
                    total_steps: 0,
                });
        }
        {
            let actions = self
                .session_mgr
                .current_mut()
                .messages
                .pipeline
                .handle_event(AgentEvent::SubAgentStart {
                    agent_id: agent_id.clone(),
                    instance_id: instance_id.clone(),
                    task_preview: task_preview.clone(),
                    is_background,
                    prompt,
                });
            for action in actions {
                self.apply_pipeline_action(action);
            }
        }
        self.update_subagent_launch_summary();
        self.request_rebuild();
        (true, false, false)
    }

    fn update_subagent_launch_summary(&mut self) {
        let session = self.session_mgr.current_mut();
        let agents: Vec<_> = session.background_agents.iter().filter_map(|agent| {
            session.subagent_sessions.get(&agent.instance_id)
        }).collect();
        if agents.is_empty() { return; }

        // 1. 普通模式：极简树形摘要
        let mut content = format!("• {} background agents launched (↓ to manage)", agents.len());
        for (index, agent) in agents.iter().enumerate() {
            // 树形行：缩进 2 列 → `├` 与标题文字同列；渲染层识别该形状后不再注入 `· ` 前缀。
            let branch = if index + 1 == agents.len() { "└──" } else { "├──" };
            content.push_str(&format!("\n  {branch} {}", agent.task_preview));
        }

        // 2. 详细模式（Ctrl+O）：方案 B 经典卡片（完美对齐参考图）
        let mut detail_content = String::new();
        for (index, agent) in agents.iter().enumerate() {
            if index > 0 {
                detail_content.push_str("\n\n");
            }
            detail_content.push_str(&format!(
                "● Agent({})\n  └ Backgrounded agent\n  └ Prompt:",
                agent.task_preview
            ));
            let prompt_text = if !agent.prompt.is_empty() {
                &agent.prompt
            } else {
                &agent.task_preview
            };
            for line in prompt_text.lines() {
                detail_content.push_str(&format!("\n      {line}"));
            }
        }

        // 摘要属于临时 UI 节点，规范历史仍由 Agent 工具调用及结果持久化。
        session.messages.ephemeral_notes.retain(|(_, vm)| {
            !matches!(vm, MessageViewModel::SystemNote { content, .. } if content.contains("background agents launched (↓ to manage)"))
        });
        session.messages.view_messages.retain(|vm| {
            !matches!(vm, MessageViewModel::SystemNote { content, .. } if content.contains("background agents launched (↓ to manage)"))
        });
        session.messages.push_system_note_with_detail(content, detail_content);
    }

    /// 刷新主视口的等待态计数行 `✻ Waiting for N background agents to finish`（PRD §4.4.2）。
    ///
    /// 与启动摘要同属临时 UI 节点（`ephemeral_notes`），不进入规范历史。活跃数归零时
    /// **必须移除该行**：残留的「Waiting for 1 …」会让用户以为还有任务在跑。
    pub(crate) fn refresh_bg_waiting_note(&mut self) {
        let session = self.session_mgr.current_mut();
        // 先清旧行，再按当前活跃数决定是否重建：数量变化时不会出现两行并存的中间态。
        session.messages.ephemeral_notes.retain(|(_, vm)| {
            !matches!(vm, MessageViewModel::SystemNote { content, .. } if content.starts_with(BG_WAITING_MARKER) && content.ends_with(BG_WAITING_SUFFIX))
        });
        session.messages.view_messages.retain(|vm| {
            !matches!(vm, MessageViewModel::SystemNote { content, .. } if content.starts_with(BG_WAITING_MARKER) && content.ends_with(BG_WAITING_SUFFIX))
        });
        let running = session
            .background_agents
            .iter()
            .filter(|agent| {
                session
                    .subagent_sessions
                    .get(&agent.instance_id)
                    .is_none_or(|sub| sub.status.is_active())
            })
            .count();
        if running == 0 {
            return;
        }
        let plural = if running > 1 { "s" } else { "" };
        session.messages.push_system_note(format!(
            "✻ Waiting for {running} background agent{plural} to finish"
        ));
    }

    pub(crate) fn handle_subagent_event(&mut self, id: &str, mut event: AgentEvent) -> (bool, bool, bool) {
        if let AgentEvent::SubagentEvent { source_agent_id, event } = event {
            return self.handle_subagent_event(&source_agent_id, *event);
        }
        // 到达子管线后去掉路由信封；它就是该管线的本地主线程。
        match &mut event {
            AgentEvent::ToolStart { source_agent_id, .. }
            | AgentEvent::ToolEnd { source_agent_id, .. }
            | AgentEvent::AssistantChunk { source_agent_id, .. } => *source_agent_id = None,
            _ => {}
        }
        // 前台同步 SubAgent 的事件需同时驱动主时间线 SubAgentGroup；在清空路由信封前
        // 先克隆一份（仅这三类事件需要，避免为整个 AgentEvent 实现 Clone）。
        let pipeline_clone = match &event {
            AgentEvent::ToolStart { tool_call_id, name, display, args, input, source_agent_id } => {
                Some(AgentEvent::ToolStart {
                    tool_call_id: tool_call_id.clone(), name: name.clone(), display: display.clone(),
                    args: args.clone(), input: input.clone(), source_agent_id: source_agent_id.clone(),
                })
            }
            AgentEvent::ToolEnd { tool_call_id, name, output, is_error, source_agent_id } => {
                Some(AgentEvent::ToolEnd {
                    tool_call_id: tool_call_id.clone(), name: name.clone(), output: output.clone(),
                    is_error: *is_error, source_agent_id: source_agent_id.clone(),
                })
            }
            AgentEvent::AssistantChunk { chunk, source_agent_id } => Some(AgentEvent::AssistantChunk {
                chunk: chunk.clone(), source_agent_id: source_agent_id.clone(),
            }),
            _ => None,
        };
        let cwd = self.services.cwd.clone();
        let (focused, actions) = {
            let session = self.session_mgr.current_mut();
            let sub = session.subagent_sessions.entry(id.to_string()).or_insert_with(|| {
                super::super::subagent_session::BackgroundAgentSession::new(id.into(), "agent".into(), String::new(), cwd)
            });
            let is_background = sub.is_background;
            sub.handle_event(event);
            // 前台同步 SubAgent：事件同时驱动主时间线 SubAgentGroup，保持与历史渲染一致。
            let actions = if !is_background {
                pipeline_clone
                    .map(|pipeline_event| session.messages.pipeline.handle_event(pipeline_event))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let focused = session.focused_instance_id.as_deref() == Some(id);
            (focused, actions)
        };
        for action in actions {
            self.apply_pipeline_action(action);
        }
        if focused {
            self.render_rebuild();
        }
        (true, false, false)
    }

    pub(crate) fn handle_subagent_lifecycle(&mut self, params: &serde_json::Value) -> (bool, bool, bool) {
        use super::super::subagent_session::SubagentRunStatus;
        let id = params["instanceId"].as_str().unwrap_or("");
        if id.is_empty() { return (false, false, false); }
        match params["event"].as_str().unwrap_or("") {
            "started" => {
                self.handle_subagent_start(
                    params["agentType"].as_str().unwrap_or("agent").into(), id.into(),
                    params["taskPreview"].as_str().unwrap_or("").into(),
                    // 默认 `true`：本分支已无前台同步模式，缺字段的载荷（旧服务端 /
                    // 非 ACP 客户端）若被当成前台，会重新打开「子过程二次投递进主消息区」。
                    params["isBackground"].as_bool().unwrap_or(true),
                    params["prompt"].as_str().map(String::from),
                );
                if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(id) {
                    sub.timeout_secs = params["timeoutSecs"].as_u64().filter(|value| *value > 0);
                    sub.model = params["model"].as_str().unwrap_or("").into();
                    sub.agent_type = params["agentType"].as_str().unwrap_or("agent").into();
                    sub.task_preview = params["taskPreview"].as_str().unwrap_or("").into();
                }
                self.update_subagent_launch_summary();
            }
            "progress" => {
                if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(id) {
                    if let Some(activity) = params["currentActivity"].as_str() {
                        sub.current_activity = activity.into();
                    }
                }
            }
            "stopped" => {
                self.cancel_subagent_approval(id);
                let result = params["result"].as_str().unwrap_or("").to_string();
                let is_error = params["isError"].as_bool().unwrap_or(false);
                if let Some(sub) = self.session_mgr.current_mut().subagent_sessions.get_mut(id) {
                    sub.elapsed = Some(std::time::Duration::from_millis(params["elapsedMs"].as_u64().unwrap_or(sub.started_at.elapsed().as_millis() as u64)));
                    sub.status = if params["status"].as_str() == Some("stopped") || sub.status == SubagentRunStatus::Stopped || result.to_lowercase().contains("cancelled") {
                        SubagentRunStatus::Stopped
                    } else if is_error { SubagentRunStatus::Failed } else { SubagentRunStatus::Completed };
                    sub.final_result = Some(result.clone());
                    sub.ended_at.get_or_insert_with(std::time::Instant::now);
                    sub.current_activity.clear();
                    sub.running_tool_times.clear();
                    sub.agent.active_tool = None;
                    // 结算子 Agent 的 spinner：完成态切到 Idle，触发耗时/动词/完成时刻
                    // 的捕获（对齐主会话 `set_loading(false)`），使子视口底部能渲染出
                    // `✻ <verb> for Xs · done HH:MM` 总结行。此前该调用遗漏，导致
                    // `last_summary_elapsed_ms` 恒为 0，切入子视口后总结行空白。
                    sub.spinner.set_mode(cc_widgets::SpinnerMode::Idle);
                    // 停止/失败属异常结束：清空总结行，避免显示成功文案（与主会话
                    // lifecycle 的 `clear_summary` 对齐）。
                    if matches!(sub.status, SubagentRunStatus::Stopped | SubagentRunStatus::Failed) {
                        sub.spinner.clear_summary();
                    }
                    sub.pipeline.handle_event(AgentEvent::Done);
                    sub.rebuild();
                    if sub.view_messages.is_empty() && !result.is_empty() {
                        let mut vm = MessageViewModel::assistant();
                        vm.append_chunk(&result);
                        sub.view_messages.push(vm);
                    }
                }
            }
            _ => return (false, false, false),
        }
        self.render_rebuild();
        (true, false, false)
    }
}
