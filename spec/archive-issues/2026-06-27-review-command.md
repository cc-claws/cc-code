# `/review` 命令 — PR Code Review

## Status
- [ ] Phase 1: 实现 ReviewCommand（Passthrough 类型）
- [ ] Phase 2: 注册到 default_command_registry
- [ ] Phase 3: 单元测试

## Created
2026-06-27

## Severity
Feature — 开发效率工具

## Platform
全平台 (Windows / macOS / Linux)

## Problem

peri 缺少 `/review` 命令。用户无法在 TUI 中快速对 PR 进行 Code Review。

Claude Code（TS 版）已实现 `/review` 命令（`C:\Work\open-cladue\src\commands\review.ts`），核心思路极简：**`prompt` 类型命令，注入一段 review prompt，由 LLM 调用 `gh` CLI 工具完成整个 review 流程**。无子进程、无远程调用、无额外依赖。

## 参考实现 — Claude Code `/review`

**源码**：`C:\Work\open-cladue\src\commands\review.ts`（57 行）

**核心设计**：

```ts
const review: Command = {
  type: 'prompt',                              // 纯 prompt 注入
  name: 'review',
  description: 'Review a pull request',
  async getPromptForCommand(args) {
    return [{ type: 'text', text: LOCAL_REVIEW_PROMPT(args) }]
  },
}
```

**完整 Prompt**（`LOCAL_REVIEW_PROMPT`，lines 9-31）：

```
You are an expert code reviewer. Follow these steps:

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

PR number: ${args}
```

**执行模型**：
- 不 spawn subagent，不修改 model，不限制 tools
- prompt 注入为 user message → agent 正常执行 → LLM 调用 Bash 工具执行 `gh pr list/view/diff` → 输出 review 结果
- 前置条件：用户环境中需要安装 `gh` CLI 并已认证

## Fix Proposal

### 映射到 peri 架构

Claude Code 的 `type: 'prompt'` = peri 的 `CommandKind::Passthrough`。

参考 `init.rs` 的 Passthrough 模式：command 的 `execute()` 返回 `BaseMessage::human(prompt)`，executor 替换原始内容后正常构建 agent。

### Phase 1: 实现 ReviewCommand

**新建** `peri-acp/src/session/command/review.rs`：

```rust
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
```

**关键设计决策**：
- `CommandKind::Passthrough`：prompt 注入后由 agent 正常执行，可使用所有工具
- 别名 `pr`：`/pr 123` 等价于 `/review 123`，更符合日常使用习惯
- `{args}` 占位符：用户输入的 PR 编号通过 `ctx.args` 传入（executor 在 `CommandRegistry::find()` 时已拆分 name/args）
- prompt 语言保持英文（与 TS 版一致），因为 `gh` CLI 输出是英文，混杂中文 prompt 可能降低 LLM 执行质量

### Phase 2: 注册

**改造** `peri-acp/src/session/command/mod.rs`：

1. 添加模块声明：
```rust
mod review;
```

2. 在 `default_command_registry()` 中注册：
```rust
reg.register(Box::new(review::ReviewCommand));
```

### Phase 3: 单元测试

**追加**到 `review.rs` 底部 `#[cfg(test)] mod tests`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // 复用 init.rs 中的 MockEventSink 和 make_ctx 模式

    #[test]
    fn test_review_command_name_and_aliases() {
        let cmd = ReviewCommand;
        assert_eq!(cmd.name(), "review");
        let aliases = cmd.aliases();
        assert!(aliases.contains(&"pr"), "应包含 pr 别名");
        assert_eq!(cmd.kind(), CommandKind::Passthrough);
        assert!(!cmd.description().is_empty());
    }

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
```

## 使用方式

```
/review          → 列出 open PRs（agent 执行 gh pr list）
/review 42       → 审查 PR #42（agent 执行 gh pr view + gh pr diff + 分析）
/pr 42           → 同上（别名）
```

## 前置条件

- 用户环境已安装 `gh` CLI（GitHub CLI）
- `gh auth login` 已完成认证
- 仓库托管在 GitHub（`gh` 仅支持 GitHub）

如果 `gh` 未安装或未认证，agent 会在执行 `gh pr list` 时收到错误并反馈给用户，无需 command 层面处理。

## Affected Files

### 新建
| 文件 | 职责 |
|------|------|
| `peri-acp/src/session/command/review.rs` | `ReviewCommand` 实现 + prompt + 单元测试 |

### 改造
| 文件 | 行号 | 改动 |
|------|------|------|
| `peri-acp/src/session/command/mod.rs` | L6 附近 + L135 附近 | 添加 `mod review;` + `reg.register(Box::new(review::ReviewCommand));` |
