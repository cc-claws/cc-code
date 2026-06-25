//! `/init` 命令 — 自动生成项目 CLAUDE.md 知识库。
//!
//! Passthrough 类型：构建 prompt 注入 agent 管线，由 AI 执行代码库分析并生成 CLAUDE.md。
//! 支持：
//! - 新项目：从零生成完整 CLAUDE.md
//! - 已有 CLAUDE.md：提出增量改进建议，不覆盖

use std::path::Path;

use peri_agent::messages::BaseMessage;

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
static NEW_CLAUDE_MD_PROMPT: &str = r#"你正在帮助用户初始化项目的 CLAUDE.md 文件。

## 执行步骤

### Phase 1: 询问配置选项
使用 AskUserQuestion 工具询问用户：
1. 初始化范围：项目 CLAUDE.md / 个人 CLAUDE.local.md / 两者都要
2. 扩展功能：Skills + Hooks / 仅 Skills / 仅 Hooks / 都不需要

### Phase 2: 代码库探索
使用 Read、Glob、Grep 工具分析项目：
- 读取 manifest 文件（Cargo.toml、package.json、pyproject.toml、go.mod）
- 读取 README.md、Makefile、CI 配置（.github/workflows/*.yml）
- 检测项目结构（monorepo、multi-module、单项目）
- 识别构建/测试/lint 命令（特别关注非标准命令）
- 查找现有 .cursor/rules/、.cursorrules 等其他 AI 配置
- 检测语言、框架、包管理器
- 识别与语言默认不同的代码风格规则
- 发现非明显的陷阱、必需的环境变量、工作流特殊要求

### Phase 3: 交互式问答
使用 AskUserQuestion 工具补充代码分析无法获取的信息：
- 分支命名规范
- PR/Code Review 流程
- 测试约定和策略
- 部署流程
- 团队协作约定

### Phase 4: 生成 CLAUDE.md
使用 Write 工具创建 CLAUDE.md，结构如下：

```markdown
# CLAUDE.md

## 项目概述
[1-2 句话描述项目目的和核心功能]

## 依赖关系
[模块/包之间的依赖图或表格]

## 开发命令
[构建、测试、lint、格式化等常用命令的快捷参考]

## 架构要点
[关键架构决策和设计模式]
**[TRAP]** [重要陷阱和注意事项]

## 编码规范
[项目特定的编码规范，区别于语言默认]

## 测试编写风格
[测试约定和最佳实践]

## 环境变量
[所有必需和可选的环境变量]

## 开发注意事项
[其他重要注意事项]
```

内容质量标准：
- 每一行都必须通过测试："删除这一行会导致 AI 犯错吗？"
- 不包含显而易见的指令
- 不列出每个组件或文件结构
- 不包含通用开发实践
- 聚焦于非标准、非直觉的信息

### Phase 5: 可选扩展
根据用户 Phase 1 的选择：
- 生成 skills 到 .claude/skills/<name>/SKILL.md
- 配置 hooks 到 .claude/settings.json
- 生成 CLAUDE.local.md（添加到 .gitignore）

### Phase 6: 总结
- 展示生成内容摘要
- 提供后续优化建议

## 重要规则
1. 生成的内容必须符合项目的 CLAUDE.md 规范
2. 中文内容必须使用中文标点
3. 技术术语保持英文（如 crate、trait、async、package、module）
4. 如果是 monorepo，正确识别多模块结构并展示依赖关系"#;

/// 已有 CLAUDE.md 优化 Prompt。
static EXISTING_CLAUDE_MD_PROMPT: &str = r#"你正在帮助用户优化现有的 CLAUDE.md 文件。

## 执行步骤

### Step 1: 读取现有文件
使用 Read 工具读取 CLAUDE.md 的完整内容。

### Step 2: 分析代码库
使用 Read、Glob、Grep 工具分析项目，识别：
- CLAUDE.md 中缺失的重要信息
- CLAUDE.md 中过时的信息
- 新发现的陷阱和约束
- 与实际代码不一致的描述

### Step 3: 提出改进建议
使用 AskUserQuestion 工具展示建议：
- 具体的修改内容（说明新增/修改/删除了什么）
- 每个修改的原因说明
- 让用户选择接受全部/部分建议

### Step 4: 应用修改
根据用户确认，使用 Edit 工具应用修改。

## 重要规则
1. 不要覆盖现有内容，只做增量改进
2. 保留用户的个人风格和组织结构
3. 新增内容放在合适的位置
4. 删除过时内容前必须确认
5. 中文内容必须使用中文标点
6. 技术术语保持英文"#;

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
            prompt.contains("初始化项目的 CLAUDE.md"),
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
            prompt.contains("优化现有"),
            "存在 CLAUDE.md 时应使用优化 prompt"
        );
        assert!(
            prompt.contains("增量改进"),
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
        assert!(
            content.contains("Phase 1"),
            "新项目 prompt 应包含 Phase 1"
        );
        assert!(
            content.contains("代码库探索"),
            "新项目 prompt 应包含代码库探索"
        );
    }
}
