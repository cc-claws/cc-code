//! `/commit` 命令 — 一键 Git Commit。
//!
//! Passthrough 类型：预执行 git 命令收集上下文，构建 commit prompt 注入 agent 管线。
//! LLM 分析变更、生成 commit message、执行 git add + commit。

use std::process::Command as StdCommand;

use peri_agent::messages::BaseMessage;

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// Git commit 命令。
pub struct CommitCommand;

impl CommitCommand {
    pub const NAME: &'static str = "commit";
}

#[async_trait::async_trait]
impl AgentCommand for CommitCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["ci"]
    }

    fn description(&self) -> &str {
        "Create a git commit"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Passthrough
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let prompt = build_commit_prompt(&ctx.cwd);

        CommandResult {
            messages: vec![BaseMessage::human(prompt)],
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

/// 运行 git 命令获取输出，失败时返回 fallback 文本。
fn run_git(cwd: &str, args: &[&str]) -> String {
    StdCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            if out.is_empty() {
                "(no output)".to_string()
            } else {
                out
            }
        })
        .unwrap_or_else(|e| format!("(git not available: {e})"))
}

/// 截断 diff 到指定字节数，在换行符处截断。
fn truncate_diff(diff: &str, max_bytes: usize) -> &str {
    if diff.len() <= max_bytes {
        diff
    } else {
        let truncated = &diff[..max_bytes];
        match truncated.rfind('\n') {
            Some(pos) => &diff[..pos],
            None => truncated,
        }
    }
}

const MAX_DIFF_BYTES: usize = 100_000; // ~100KB

/// 构建 commit prompt，预执行 git 命令嵌入上下文。
fn build_commit_prompt(cwd: &str) -> String {
    let status = run_git(cwd, &["status"]);
    let diff = run_git(cwd, &["diff", "HEAD"]);
    let diff = truncate_diff(&diff, MAX_DIFF_BYTES).to_string();
    let branch = run_git(cwd, &["branch", "--show-current"]);
    let log = run_git(cwd, &["log", "--oneline", "-10"]);

    let attribution = "Co-Authored-By: mimo-v2.5-pro <XiaomiMiMo@cc-code>";

    format!(
        r#"## Context

- Current git status:
{status}
- Current git diff (staged and unstaged changes):
{diff}
- Current branch:
{branch}
- Recent commits:
{log}

## Git Safety Protocol

- NEVER update the git config
- NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it
- CRITICAL: ALWAYS create NEW commits. NEVER use git commit --amend, unless the user explicitly requests it
- Do not commit files that likely contain secrets (.env, credentials.json, etc). Warn the user if they specifically request to commit those files
- If there are no changes to commit (i.e., no untracked files and no modifications), do not create an empty commit
- Never use git commands with the -i flag (like git rebase -i or git add -i) since they require interactive input which is not supported

## Your task

Based on the above changes, create a single git commit:

1. Analyze all staged changes and draft a commit message:
   - Look at the recent commits above to follow this repository's commit message style
   - Summarize the nature of the changes (new feature, enhancement, bug fix, refactoring, test, docs, etc.)
   - Ensure the message accurately reflects the changes and their purpose (i.e. "add" means a wholly new feature, "update" means an enhancement to an existing feature, "fix" means a bug fix, etc.)
   - Draft a concise (1-2 sentences) commit message that focuses on the "why" rather than the "what"

2. Stage relevant files and create the commit using HEREDOC syntax:
```
git commit -m "$(cat <<'EOF'
Commit message here.

{attribution}
EOF
)"
```

You have the capability to call multiple tools in a single response. Stage and create the commit using a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls."#
    )
}

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
    fn test_commit_command_properties() {
        let cmd = CommitCommand;
        assert_eq!(cmd.name(), "commit");
        assert!(cmd.aliases().contains(&"ci"), "应包含 ci 别名");
        assert_eq!(cmd.kind(), CommandKind::Passthrough);
        assert!(!cmd.description().is_empty());
    }

    // ── execute 测试 ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_execute_returns_human_message() {
        let cmd = CommitCommand;
        let ctx = make_ctx("/tmp");
        let result = cmd.execute(ctx).await;
        assert_eq!(result.messages.len(), 1);
        assert!(matches!(result.messages[0], BaseMessage::Human { .. }));
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);
    }

    #[tokio::test]
    async fn test_execute_prompt_contains_git_safety_protocol() {
        let cmd = CommitCommand;
        let ctx = make_ctx("/tmp");
        let result = cmd.execute(ctx).await;
        let content = result.messages[0].content();
        assert!(content.contains("Git Safety Protocol"), "应包含安全协议");
        assert!(
            content.contains("NEVER use git commit --amend"),
            "应禁止 amend"
        );
    }

    #[tokio::test]
    async fn test_execute_prompt_contains_context_sections() {
        let cmd = CommitCommand;
        let ctx = make_ctx("/tmp");
        let result = cmd.execute(ctx).await;
        let content = result.messages[0].content();
        assert!(content.contains("git status"), "应包含 git status 上下文");
        assert!(content.contains("git diff"), "应包含 git diff 上下文");
        assert!(
            content.contains("Recent commits"),
            "应包含 Recent commits 段落"
        );
        assert!(content.contains("Co-Authored-By"), "应包含归属行");
    }

    // ── run_git 测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_run_git_returns_output() {
        let output = run_git("/tmp", &["--version"]);
        assert!(output.contains("git"), "git --version 应返回版本信息");
    }

    #[test]
    fn test_run_git_invalid_dir_graceful() {
        let output = run_git("/nonexistent_path_xyz", &["status"]);
        assert!(output.contains("git not available"), "无效路径应优雅降级");
    }

    // ── truncate_diff 测试 ────────────────────────────────────────────────

    #[test]
    fn test_truncate_diff_within_limit() {
        let diff = "a".repeat(50_000);
        let result = truncate_diff(&diff, 100_000);
        assert_eq!(result.len(), 50_000, "未超限不应截断");
    }

    #[test]
    fn test_truncate_diff_exceeds_limit() {
        let diff = "line1\nline2\nline3\n".repeat(10_000);
        let result = truncate_diff(&diff, 100);
        assert!(result.len() <= 100, "超限时应截断");
        assert!(
            result.ends_with('\n') || result.len() < 100,
            "应在换行符处截断"
        );
    }
}
