//! `/init` 命令 — 自动生成项目 CLAUDE.md 知识库。
//!
//! Passthrough 类型：构建 prompt 注入 agent 管线，由 AI 执行代码库分析并生成 CLAUDE.md。
//! 支持：
//! - 新项目：从零生成完整 CLAUDE.md
//! - 已有 CLAUDE.md：提出增量改进建议，不覆盖

use std::path::Path;

use cc_agent::messages::BaseMessage;

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 项目初始化命令。
pub struct InitCommand;

impl InitCommand {
    pub const NAME: &'static str = "init";
}

#[async_trait::async_trait]
impl AgentCommand for InitCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["setup"]
    }

    fn description(&self) -> &str {
        "生成或优化项目 CLAUDE.md 知识库"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Passthrough
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let prompt = self.build_init_prompt(&ctx.cwd);

        CommandResult {
            messages: vec![BaseMessage::human(prompt)],
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

impl InitCommand {
    /// 根据是否已有 CLAUDE.md 选择不同的 prompt。
    fn build_init_prompt(&self, cwd: &str) -> String {
        let has_claude_md = Path::new(cwd).join("CLAUDE.md").exists();

        if has_claude_md {
            EXISTING_CLAUDE_MD_PROMPT.to_string()
        } else {
            NEW_CLAUDE_MD_PROMPT.to_string()
        }
    }
}

/// 新项目初始化 Prompt。
static NEW_CLAUDE_MD_PROMPT: &str = r#"Initialize the project's CLAUDE.md as a concise guide for future work in this repository.

## Phase 1: Explore the repository

- Read relevant project instructions, manifests, README files, build scripts, CI configuration, and representative source files.
- Identify the actual architecture, module dependencies, development commands, testing approach, and project-specific conventions.
- Focus on verified constraints and non-obvious pitfalls. Check existing documentation before asking about undocumented rules that materially affect the guide.
- Keep the default scope to the project's CLAUDE.md. Do not create personal configuration, skills, hooks, or ignore rules unless the user explicitly requests that additional work.

## Phase 2: Write the project guide

- Use the language explicitly requested by the user; otherwise follow the established project documentation language or the conversation language. Keep commands, paths, identifiers, and technical names unchanged.
- Organize the guide around what a contributor needs: project purpose, important dependencies and boundaries, development commands, architectural constraints, relevant coding and test conventions, and environment requirements.
- Include only information supported by repository evidence or explicit user guidance. Do not invent commands, branch policies, deployment procedures, or secret values.
- Favor concise project-specific guidance over generic advice, exhaustive file inventories, or duplicated documentation. Link to detailed project documents when appropriate.
- Before creating the file, check that CLAUDE.md is still absent. If it already exists, read it and propose incremental changes for confirmation unless those changes are already explicitly authorized.

## Phase 3: Report the result

State which file was created, the important guidance included, and any unresolved information. Do not claim that documented commands were executed unless they actually ran."#;

/// 已有 CLAUDE.md 优化 Prompt。
static EXISTING_CLAUDE_MD_PROMPT: &str = r#"Improve the existing CLAUDE.md through evidence-based incremental changes.

1. Read the complete current file and the relevant repository implementation, manifests, scripts, and documentation.
2. Identify missing project-specific guidance, outdated statements, verified pitfalls, and descriptions that disagree with the code. Preserve user-authored conventions and useful organization.
3. Present concrete proposed additions, edits, or removals with their reasons. Obtain confirmation before modifying the existing guide, including removing outdated content, unless the user has already explicitly authorized those changes. Continue independent investigation while a required decision is pending.
4. Apply only the confirmed or previously authorized changes using targeted edits. Do not replace the entire document or expand into personal configuration, skills, or hooks without a separate request.
5. Report the applied changes, their evidence, and any information that remains unverified.

Use the user's requested document language; otherwise preserve the existing guide's language and style. Keep commands, paths, identifiers, and technical names unchanged. Do not invent project rules or secret values, and do not claim to have run commands that were only inspected."#;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use cc_agent::agent::events::AgentEvent as ExecutorEvent;

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
            aux_model: None,
            event_sink: Arc::new(MockEventSink),
            args: String::new(),
            cancel_token: cc_agent::agent::AgentCancellationToken::new(),
            thread_store: None,
            thread_id: None,
        }
    }

    // ── 属性测试 ──────────────────────────────────────────────────────────

    #[test]
    fn test_init_command_name_and_aliases() {
        let cmd = InitCommand;
        assert_eq!(cmd.name(), "init");
        let aliases = cmd.aliases();
        assert!(aliases.contains(&"setup"), "应包含 setup 别名");
        assert_eq!(cmd.kind(), CommandKind::Passthrough);
        assert!(!cmd.description().is_empty());
    }

    // ── Prompt 生成测试 ───────────────────────────────────────────────────

    #[test]
    fn test_build_init_prompt_uses_new_prompt_when_no_claude_md() {
        // Arrange: 使用不存在的路径
        let cmd = InitCommand;
        let cwd = "/tmp/nonexistent_init_test_dir_xyz";

        // Act
        let prompt = cmd.build_init_prompt(cwd);

        // Assert: 应该使用新项目 prompt
        assert!(
            prompt.contains("Initialize the project's CLAUDE.md"),
            "不存在的 CLAUDE.md 应使用新项目 prompt"
        );
        assert!(
            prompt.contains("Phase 1"),
            "新项目 prompt 应包含 Phase 步骤"
        );
    }

    #[test]
    fn test_build_init_prompt_uses_existing_prompt_when_claude_md_exists() {
        // Arrange: 使用临时目录并创建 CLAUDE.md
        let tmp = tempfile::tempdir().unwrap();
        let claude_md_path = tmp.path().join("CLAUDE.md");
        std::fs::write(&claude_md_path, "# test").unwrap();

        let cmd = InitCommand;

        // Act
        let prompt = cmd.build_init_prompt(tmp.path().to_str().unwrap());

        // Assert: 应该使用已有 CLAUDE.md 的优化 prompt
        assert!(
            prompt.contains("Improve the existing CLAUDE.md"),
            "存在 CLAUDE.md 时应使用优化 prompt"
        );
        assert!(
            prompt.contains("incremental changes"),
            "优化 prompt 应提及增量改进"
        );
    }

    // ── execute 测试 ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_execute_returns_human_message_with_continue() {
        // Arrange
        let cmd = InitCommand;
        let ctx = make_ctx("/tmp");

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 返回一条 Human 消息，stop_reason 为 Continue
        assert_eq!(result.messages.len(), 1);
        assert!(
            matches!(result.messages[0], BaseMessage::Human { .. }),
            "应为 Human 消息"
        );
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);
    }

    #[tokio::test]
    async fn test_execute_new_project_prompt_content() {
        // Arrange: 不存在 CLAUDE.md 的路径
        let cmd = InitCommand;
        let ctx = make_ctx("/tmp/nonexistent_init_xyz");

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 内容包含关键段落
        let content = result.messages[0].content();
        assert!(content.contains("Phase 1"), "新项目 prompt 应包含 Phase 1");
        assert!(
            content.contains("Explore the repository"),
            "新项目 prompt 应包含代码库探索"
        );
    }
}
