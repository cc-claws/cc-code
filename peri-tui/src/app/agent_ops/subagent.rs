//! SubAgent state tracking — token usage updates + subagent start events.
//! Extracted from original agent_ops.rs (2026-05-20 split).

use super::super::*;

use crate::app::{message_pipeline::PipelineAction, App};

impl App {
    pub(super) fn handle_token_usage_update(
        &mut self,
        usage: peri_agent::llm::types::TokenUsage,
    ) -> (bool, bool, bool) {
        // SubAgent 的 TokenUsageUpdate 不应污染父 agent 的 tracker
        if self.session_mgr.current_mut().agent.subagent_depth > 0 {
            return (true, false, false);
        }

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
            let percentage = (rate * 100.0) as u32;
            let req_id = tracker.last_request_id.as_deref().unwrap_or("-");
            let msg = format!(
                "⚠ {}",
                self.services.lc.tr_args(
                    "app-prompt-cache-low",
                    &[
                        ("rate".into(), (percentage as i64).into()),
                        ("req".into(), req_id.to_string().into()),
                    ]
                )
            );
            let vm = MessageViewModel::system(msg);
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
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
    ) -> (bool, bool, bool) {
        if is_background {
            use super::super::chat_session::RunningBgAgent;
            self.session_mgr
                .current_mut()
                .background_agents
                .push(RunningBgAgent {
                    agent_name: agent_id.clone(),
                    instance_id: instance_id.clone(),
                    started_at: std::time::Instant::now(),
                });
        }
        self.session_mgr.current_mut().agent.subagent_depth += 1;
        // Pipeline：创建 SubAgentGroup VM
        let actions = self
            .session_mgr
            .current_mut()
            .messages
            .pipeline
            .handle_event(AgentEvent::SubAgentStart {
                agent_id,
                instance_id,
                task_preview,
                is_background,
            });
        for action in actions {
            self.apply_pipeline_action(action);
        }
        self.request_rebuild();
        (true, false, false)
    }
}
