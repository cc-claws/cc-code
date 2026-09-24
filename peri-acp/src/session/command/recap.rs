//! `/recap` 命令 — 生成当前会话的一句话回顾。
//!
//! 参考 Claude Code 的 away-summary 实现（`claude-code-best/claude-code`
//! `src/commands/recap/`），别名 `/away`、`/catchup`。
//!
//! 与 CCB 的差异：
//! - CCB 是 `LocalCommand`，结果直接返回 REPL 文本；peri 侧 `CommandResult`
//!   不带输出文本，故通过 [`AgentEvent::RecapCompleted`] / [`AgentEvent::RecapError`]
//!   事件推送到 TUI 渲染。
//! - CCB 通过 CacheSafeParams 共享主循环 prompt cache 前缀；peri 的 slash 命令
//!   拦截点在 agent 构建前，暂不共享 cache（低频场景成本可接受）。

use std::sync::Arc;

use peri_agent::agent::{
    events::AgentEvent as ExecutorEvent, recap::generate_recap, recap::RecapResult,
};

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 会话回顾命令。
pub struct RecapCommand;

impl RecapCommand {
    pub const NAME: &'static str = "recap";
}

#[async_trait::async_trait]
impl AgentCommand for RecapCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["away", "catchup"]
    }

    fn description(&self) -> &str {
        "生成当前会话的一句话回顾（目标 + 当前任务 + 下一步）"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let CommandContext {
            session_id,
            history,
            peri_config: _,
            aux_model,
            event_sink,
            cancel_token,
            ..
        } = ctx;

        tracing::debug!(history_len = history.len(), "recap: execute called");

        // 获取模型（使用通用辅助模型，不受 compact 开关影响）
        let model: Arc<dyn peri_agent::llm::BaseModel> = match aux_model {
            Some(m) => m,
            None => {
                tracing::warn!("recap: 无可用模型");
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::RecapError {
                            message: "no model available for recap".into(),
                        },
                        0,
                    )
                    .await;
                return CommandResult {
                    messages: history,
                    stop_reason: PromptStopReason::EndTurn,
                };
            }
        };

        let result = generate_recap(&history, model.as_ref(), &cancel_token).await;

        match result {
            RecapResult::Ok { text } => {
                tracing::info!(session_id = %session_id, text_len = text.len(), "recap: 生成完成");
                event_sink
                    .push_event(&session_id, &ExecutorEvent::RecapCompleted { text }, 0)
                    .await;
            }
            RecapResult::ApiError { text } => {
                tracing::warn!(session_id = %session_id, "recap: API 错误");
                event_sink
                    .push_event(&session_id, &ExecutorEvent::RecapError { message: text }, 0)
                    .await;
            }
            RecapResult::NoTurn => {
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::RecapError {
                            message: "no history to recap".into(),
                        },
                        0,
                    )
                    .await;
            }
            RecapResult::Aborted => {
                tracing::info!(session_id = %session_id, "recap: 已取消");
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::RecapError {
                            message: "recap cancelled".into(),
                        },
                        0,
                    )
                    .await;
                return CommandResult {
                    messages: history,
                    stop_reason: PromptStopReason::Cancelled,
                };
            }
            RecapResult::Failed => {
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::RecapError {
                            message: "recap generation failed".into(),
                        },
                        0,
                    )
                    .await;
            }
        }

        // recap 不修改 history（对应 CCB skipTranscript）
        CommandResult {
            messages: history,
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use peri_agent::{
        agent::{events::AgentEvent as ExecutorEvent, AgentCancellationToken},
        messages::BaseMessage,
    };

    use super::*;
    use crate::session::{
        command::CommandContext, event_sink::EventSink, executor::PromptStopReason,
    };

    // ── Mock EventSink ────────────────────────────────────────────────────

    struct MockEventSink {
        events: Mutex<Vec<(String, String)>>,
    }

    impl MockEventSink {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<(String, String)> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EventSink for MockEventSink {
        async fn push_event(&self, session_id: &str, event: &ExecutorEvent, _context_window: u32) {
            let json = serde_json::to_string(event).unwrap_or_default();
            self.events
                .lock()
                .unwrap()
                .push((session_id.to_string(), json));
        }

        async fn push_done(&self, _session_id: &str) {}
    }

    // ── Mock BaseModel ────────────────────────────────────────────────────

    struct MockBaseModel {
        response: String,
    }

    #[async_trait]
    impl peri_agent::llm::BaseModel for MockBaseModel {
        async fn invoke(
            &self,
            _request: peri_agent::llm::types::LlmRequest,
        ) -> peri_agent::error::AgentResult<peri_agent::llm::types::LlmResponse> {
            Ok(peri_agent::llm::types::LlmResponse {
                message: BaseMessage::ai(self.response.clone()),
                stop_reason: peri_agent::llm::types::StopReason::EndTurn,
                usage: None,
                request_id: None,
            })
        }
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    fn make_ctx(
        sink: Arc<dyn EventSink>,
        history: Vec<BaseMessage>,
        model: Option<Arc<dyn peri_agent::llm::BaseModel>>,
    ) -> CommandContext {
        CommandContext {
            session_id: "test-session".to_string(),
            history,
            cwd: "/tmp".to_string(),
            peri_config: Arc::new(Default::default()),
            compact_model: model.clone(),
            aux_model: model,
            event_sink: sink,
            args: String::new(),
            cancel_token: AgentCancellationToken::new(),
            thread_store: None,
            thread_id: None,
        }
    }

    // ── RecapCommand 属性测试 ─────────────────────────────────────────────

    #[test]
    fn test_recap_command_name_and_aliases() {
        let cmd = RecapCommand;

        assert_eq!(cmd.name(), "recap");
        let aliases = cmd.aliases();
        assert!(aliases.contains(&"away"), "应包含 away 别名");
        assert!(aliases.contains(&"catchup"), "应包含 catchup 别名");
        assert_eq!(cmd.kind(), CommandKind::Immediate);
        assert!(!cmd.description().is_empty());
    }

    // ── RecapCommand execute 测试 ─────────────────────────────────────────

    #[tokio::test]
    async fn test_recap_command_no_model_emits_error_event() {
        // Arrange: 有历史但无模型
        let sink = Arc::new(MockEventSink::new());
        let history = vec![BaseMessage::human("你好"), BaseMessage::ai("世界")];
        let ctx = make_ctx(sink.clone(), history.clone(), None);
        let cmd = RecapCommand;

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 返回原历史 + EndTurn + RecapError 事件
        assert_eq!(result.messages.len(), 2, "history 不应被修改");
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);

        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(
            events[0].1.contains("recap_error"),
            "应推送 recap_error 事件，实际: {}",
            events[0].1
        );
        assert!(
            events[0].1.contains("no model available"),
            "错误消息应包含 'no model available'，实际: {}",
            events[0].1
        );
    }

    #[tokio::test]
    async fn test_recap_command_empty_history_emits_error_event() {
        // Arrange: 空历史 + 有模型
        let sink = Arc::new(MockEventSink::new());
        let model: Arc<dyn peri_agent::llm::BaseModel> = Arc::new(MockBaseModel {
            response: "不应被调用".to_string(),
        });
        let ctx = make_ctx(sink.clone(), vec![], Some(model));
        let cmd = RecapCommand;

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 空历史应返回 NoTurn → RecapError
        assert!(result.messages.is_empty());
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);

        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(
            events[0].1.contains("recap_error"),
            "空历史应推送 recap_error，实际: {}",
            events[0].1
        );
        assert!(
            events[0].1.contains("no history to recap"),
            "错误消息应包含 'no history to recap'，实际: {}",
            events[0].1
        );
    }

    #[tokio::test]
    async fn test_recap_command_success_emits_completed_event() {
        // Arrange: 有历史 + 模型返回摘要
        let sink = Arc::new(MockEventSink::new());
        let model: Arc<dyn peri_agent::llm::BaseModel> = Arc::new(MockBaseModel {
            response: "正在排查订单出库仓问题。下一步：确认应走官方仓还是自发仓。".to_string(),
        });
        let history = vec![
            BaseMessage::human("排查订单 IM20260910361758 出库单为何一直用官方仓"),
            BaseMessage::ai("根因是 Allegro One Fulfillment 订单被代码强制改派官方仓。"),
        ];
        let ctx = make_ctx(sink.clone(), history.clone(), Some(model));
        let cmd = RecapCommand;

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: history 不变 + EndTurn + RecapCompleted 事件
        assert_eq!(result.messages.len(), 2, "history 不应被修改");
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);

        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(
            events[0].1.contains("recap_completed"),
            "应推送 recap_completed 事件，实际: {}",
            events[0].1
        );
        assert!(
            events[0].1.contains("排查订单出库仓问题"),
            "事件应包含 recap 文本，实际: {}",
            events[0].1
        );
    }
}
