# `/commit` 命令 — 一键 Git Commit

## Status
- [ ] Phase 1: 实现 CommitCommand（Passthrough 类型）
- [ ] Phase 2: Git 上下文注入（execute 前运行 git 命令收集信息）
- [ ] Phase 3: 注册到 default_command_registry
- [ ] Phase 4: 单元测试

## Created
2026-06-27

## Severity
Feature — 高频开发工具

## Platform
全平台 (Windows / macOS / Linux)

## Problem

peri 缺少 `/commit` 命令。用户需要手动 `git add` + `git commit`，或在对话中描述"帮我提交"让 agent 自由发挥，缺乏标准化的 commit 流程。

Claude Code（TS 版）已实现 `/commit` 命令（`C:\Work\open-cladue\src\commands\commit.ts`，93 行）。核心设计：**prompt 类型命令，在 prompt 注入前先执行 4 条 git 命令收集上下文（status/diff/branch/log），将结果嵌入 prompt，再由 LLM 分析变更并生成 commit message + 执行 git commit**。

## 参考实现 — Claude Code `/commit`

**源码**：`C:\Work\open-cladue\src\commands\commit.ts`（93 行）

### 核心设计

1. **预执行 shell 命令**：prompt 中嵌入 `` !`git status` `` 语法，`executeShellCommandsInPrompt()` 在发送给 LLM 前先执行这些命令，将 stdout 替换进 prompt 文本
2. **Git Safety Protocol**：prompt 内置安全规则（禁止 amend、禁止 --no-verify、禁止提交 secrets 等）
3. **Co-Authored-By 归属**：commit message 末尾自动追加 `Co-Authored-By: Claude <noreply@anthropic.com>`
4. **工具限制**：`allowedTools` 限制为 `Bash(git add:*)`, `Bash(git status:*)`, `Bash(git commit:*)`

### 完整 Prompt（getPromptContent）

```
## Context

- Current git status: !`git status`
- Current git diff (staged and unstaged changes): !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`

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
   - Ensure the message accurately reflects the changes and their purpose
   - Draft a concise (1-2 sentences) commit message that focuses on the "why" rather than the "what"

2. Stage relevant files and create the commit using HEREDOC syntax:
   git commit -m "$(cat <<'EOF'
   Commit message here.
   EOF
   )"

You have the capability to call multiple tools in a single response. Stage and create the commit using a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls.
```

### 执行流程

```
用户输入 /commit
    │
    ▼
getPromptForCommand()
    │
    ├─ executeShellCommandsInPrompt()
    │   ├─ 执行 `git status` → 替换 !`git status`
    │   ├─ 执行 `git diff HEAD` → 替换 !`git diff HEAD`
    │   ├─ 执行 `git branch --show-current` → 替换 !`git branch --show-current`
    │   └─ 执行 `git log --oneline -10` → 替换 !`git log --oneline -10`
    │
    ▼
注入为 user message → agent 执行
    │
    ├─ LLM 分析 git status + diff + recent commits
    ├─ LLM 生成 commit message（遵循仓库风格）
    ├─ LLM 调用 git add + git commit（工具受限）
    └─ 输出结果
```

## Fix Proposal

### 映射到 peri 架构

Claude Code 的 `type: 'prompt'` + `executeShellCommandsInPrompt()` = peri 的 `CommandKind::Passthrough` + **execute 前 shell 预执行**。

关键差异：Claude Code 用 `executeShellCommandsInPrompt()` 在 prompt 文本中内联执行 shell 命令。peri 的 Passthrough 模式中，`execute()` 方法可以在返回 `CommandResult` 前自行执行 shell 命令并拼接 prompt。

### Phase 1: 实现 CommitCommand

**新建** `peri-acp/src/session/command/commit.rs`：

```rust
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
```

### Phase 2: Git 上下文注入

**核心函数** `build_commit_prompt()` — 在 execute 阶段预执行 4 条 git 命令：

```rust
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

/// 构建 commit prompt，预执行 git 命令嵌入上下文。
fn build_commit_prompt(cwd: &str) -> String {
    let status = run_git(cwd, &["status"]);
    let diff = run_git(cwd, &["diff", "HEAD"]);
    let branch = run_git(cwd, &["branch", "--show-current"]);
    let log = run_git(cwd, &["log", "--oneline", "-10"]);

    // 追加 Co-Authored-By（与 CLAUDE.md 的 Git Attribution 规范一致）
    let attribution = "\n\nCo-Authored-By: peri <noreply@cc-code>";

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
Commit message here.{attribution}
EOF
)"
```

You have the capability to call multiple tools in a single response. Stage and create the commit using a single message. Do not use any other tools or do anything else. Do not send any other text or messages besides these tool calls."#
    )
}
```

**关键设计决策**：

1. **预执行 shell 命令**：与 Claude Code 的 `executeShellCommandsInPrompt()` 对等，但实现更简单——直接在 `build_commit_prompt()` 中用 `std::process::Command` 运行 git，无需解析 prompt 中的 `` !`...` `` 语法。这是 Rust 的自然做法。

2. **diff 截断**：`git diff HEAD` 可能输出巨大（如 lockfile 变更）。需限制最大长度：

```rust
fn truncate_diff(diff: &str, max_bytes: usize) -> &str {
    if diff.len() <= max_bytes {
        diff
    } else {
        // 找到最近的换行符截断
        let truncated = &diff[..max_bytes];
        match truncated.rfind('\n') {
            Some(pos) => &diff[..pos],
            None => truncated,
        }
    }
}

const MAX_DIFF_BYTES: usize = 100_000;  // ~100KB
```

3. **Co-Authored-By**：与 CLAUDE.md 的 Git Attribution 规范一致，追加 `Co-Authored-By: peri <noreply@cc-code>`。可通过 `peri_config` 中的 `attribution` 字段控制开关。

4. **Windows 兼容**：`StdCommand::new("git")` 在 Windows 上需要确保 git 在 PATH 中。`shell_command()` wrapper 用于 shell 命令，但 git 可直接调用（不需要 shell 展开）。

5. **工具限制（Future）**：Claude Code 用 `allowedTools` 限制 agent 只能执行 `git add/status/commit`。peri 当前 `Passthrough` 模式不支持工具限制，agent 可使用所有工具。**MVP 阶段可接受**，agent 收到明确的 prompt 指令后通常不会偏离。后续可在 `CommandResult` 中增加 `allowed_tools` 字段。

### Phase 3: 注册

**改造** `peri-acp/src/session/command/mod.rs`：

```rust
// 添加模块声明
mod commit;

// 在 default_command_registry() 中注册
reg.register(Box::new(commit::CommitCommand));
```

### Phase 4: 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_command_properties() {
        let cmd = CommitCommand;
        assert_eq!(cmd.name(), "commit");
        assert!(cmd.aliases().contains(&"ci"), "应包含 ci 别名");
        assert_eq!(cmd.kind(), CommandKind::Passthrough);
        assert!(!cmd.description().is_empty());
    }

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
        assert!(content.contains("NEVER use git commit --amend"), "应禁止 amend");
    }

    #[tokio::test]
    async fn test_execute_prompt_contains_context_sections() {
        let cmd = CommitCommand;
        let ctx = make_ctx("/tmp");
        let result = cmd.execute(ctx).await;
        let content = result.messages[0].content();
        assert!(content.contains("git status"), "应包含 git status 上下文");
        assert!(content.contains("git diff"), "应包含 git diff 上下文");
        assert!(content.contains("git log"), "应包含 git log 上下文");
        assert!(content.contains("Co-Authored-By"), "应包含归属行");
    }

    #[test]
    fn test_run_git_returns_output() {
        // 在 git 仓库中运行应有输出
        let output = run_git("/tmp", &["--version"]);
        assert!(output.contains("git"), "git --version 应返回版本信息");
    }

    #[test]
    fn test_run_git_invalid_dir_graceful() {
        // 不存在的目录应返回错误信息而非 panic
        let output = run_git("/nonexistent_path_xyz", &["status"]);
        assert!(output.contains("git not available"), "无效路径应优雅降级");
    }

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
        assert!(result.ends_with('\n') || result.len() < 100, "应在换行符处截断");
    }
}
```

## 使用方式

```
/commit          → 分析当前变更，自动生成 commit message 并提交
/ci              → 同上（别名）
```

## 前置条件

- 项目是 git 仓库（有 `.git` 目录）
- `git` 在 PATH 中

如果不在 git 仓库中，`run_git()` 会返回错误信息，agent 会在 prompt 上下文中看到并反馈给用户。

## 与 CLAUDE.md 的关系

CLAUDE.md 已规定 "Git Attribution"：commit message 末尾追加 `Co-Authored-By`。`/commit` 的 prompt 中已内嵌此规则，agent 生成的 commit message 会自动包含归属行，**与系统提示词中的规则一致，不冲突**。

## Affected Files

### 新建
| 文件 | 职责 |
|------|------|
| `peri-acp/src/session/command/commit.rs` | `CommitCommand` + `build_commit_prompt()` + `run_git()` + `truncate_diff()` + 单元测试 |

### 改造
| 文件 | 行号 | 改动 |
|------|------|------|
| `peri-acp/src/session/command/mod.rs` | L6 附近 + L135 附近 | 添加 `mod commit;` + `reg.register(Box::new(commit::CommitCommand));` |
