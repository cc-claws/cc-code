//! `/review` 命令 — PR Code Review。
//!
//! Passthrough 类型：构建 review prompt 注入 agent 管线，
//! 由 AI 调用 `gh` CLI 工具完成 PR 审查。

use peri_agent::messages::BaseMessage;

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

pub struct ReviewCommand;

impl ReviewCommand {
    pub const NAME: &'static str = "review";
}

#[async_trait::async_trait]
impl AgentCommand for ReviewCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["pr"]
    }

    fn description(&self) -> &str {
        "Review a pull request"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Passthrough
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let prompt = REVIEW_PROMPT.replace("{args}", &ctx.args);

        CommandResult {
            messages: vec![BaseMessage::human(prompt)],
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

/// Review prompt — 与 Claude Code TS 版 `LOCAL_REVIEW_PROMPT` 对齐。
/// agent 通过 Bash 工具调用 `gh` CLI 完成 PR 审查。
static REVIEW_PROMPT: &str = r#"You are an expert code reviewer. Follow these steps:

1. If no PR number is provided in the args, run `gh pr list` to show open PRs
2. If a PR number is provided, run `gh pr view <number>` to get PR details
3. Run `gh pr diff <number>` to get the diff
4. Analyze the changes and provide a thorough code review that includes:
   - Overview of what the PR does
   - Analysis of code quality and style
   - Specific suggestions for improvements
   - Any potential issues or risks

Keep your review concise but thorough. Focus on:
- Code correctness
- Following project conventions
- Performance implications
- Test coverage
- Security considerations

Format your review with clear sections and bullet points.

PR number: {args}"#;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use peri_agent::agent::events::AgentEvent as ExecutorEvent;

    use super::*;

    // ── Mock EventSink ────────────────────────────────────────────────────

    struct MockEventSink;
    #[async_trait]
    impl crate::session::event_sink::EventSink for MockEventSink {
        async fn push_event(
            &self,
            _session_id: &str,
            _event: &ExecutorEvent,
            _context_window: u32,
        ) {
        }
        async fn push_done(&self, _session_id: &str) {}
    }

    fn make_ctx(cwd: &str) -> CommandContext {
        CommandContext {
            session_id: "test-session".to_string(),
            history: vec![],
            cwd: cwd.to_string(),
            peri_config: Arc::new(Default::default()),
            compact_model: None,
            event_sink: Arc::new(MockEventSink),
            args: String::new(),
            cancel_token: peri_agent::agent::AgentCancellationToken::new(),
            thread_store: None,
            thread_id: None,
        }
    }

    // ── 属性测试 ──────────────────────────────────────────────────────────

    #[test]
    fn test_review_command_name_and_aliases() {
        let cmd = ReviewCommand;
        assert_eq!(cmd.name(), "review");
        let aliases = cmd.aliases();
        assert!(aliases.contains(&"pr"), "应包含 pr 别名");
        assert_eq!(cmd.kind(), CommandKind::Passthrough);
        assert!(!cmd.description().is_empty());
    }

    // ── execute 测试 ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_execute_returns_human_message() {
        let cmd = ReviewCommand;
        let mut ctx = make_ctx("/tmp");
        ctx.args = "42".to_string();
        let result = cmd.execute(ctx).await;
        assert_eq!(result.messages.len(), 1);
        assert!(matches!(result.messages[0], BaseMessage::Human { .. }));
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);
    }

    #[tokio::test]
    async fn test_execute_prompt_contains_pr_number() {
        let cmd = ReviewCommand;
        let mut ctx = make_ctx("/tmp");
        ctx.args = "42".to_string();
        let result = cmd.execute(ctx).await;
        let content = result.messages[0].content();
        assert!(content.contains("42"), "prompt 应包含用户传入的 PR 编号");
        assert!(content.contains("gh pr"), "prompt 应包含 gh pr 命令指引");
    }

    #[tokio::test]
    async fn test_execute_empty_args_shows_list_instruction() {
        let cmd = ReviewCommand;
        let ctx = make_ctx("/tmp");
        let result = cmd.execute(ctx).await;
        let content = result.messages[0].content();
        assert!(content.contains("gh pr list"), "无参数时应指引列出 open PRs");
    }
}
