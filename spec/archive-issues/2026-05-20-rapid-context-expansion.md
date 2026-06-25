> 归档于 2026-05-20，原路径 spec/issues/2026-05-20-rapid-context-expansion.md

# System Prompt 每轮重复注入导致上下文膨胀

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-20
**修复日期**：2026-05-20

## 问题描述

Agent 每轮 LLM 调用时，system prompt 中的 "## Deferred Tools" 段落（含 MCP 工具描述）被完整追加而非替换。随着轮次增加，system prompt 呈倍数膨胀：Round 8 为 69K chars（1 份），Round 9 为 145K（2 份），Round 12 为 374K（5 份），Round 55 为 451K（6 份）。

## 根因

每轮 Agent 执行时重新构建 system prompt（包括 MCP 工具注册），`prepend_message` 向消息头部插入导致 `StateSnapshot` 快照范围扩大，system prompt 内容跨轮次累积。

## 修复方案

采用 **Frozen Session Data** 模式——system prompt 在 `session/new` 时构建一次并冻结，所有后续 `session/prompt` 轮次直接使用已冻结的值。

### 修改文件

| 文件 | 变更 |
|------|------|
| `peri-acp/src/session/executor.rs:48-67` | 新增 `FrozenSessionData` 结构体（system_prompt/claude_md/skill_summary/date/is_git_repo） |
| `peri-acp/src/session/executor.rs:165-173` | `execute_prompt()` 优先使用 frozen 数据，跳过重建 |
| `peri-tui/src/acp_server/prompt.rs` | TUI 侧 `execute_prompt()` 传入 frozen 数据 |
| `peri-tui/src/acp_server/requests.rs` | `session/new` 时构建 frozen 数据 |

### 修复后数据流

```
session/new → chrono::Local::now() → frozen_date
            → AgentsMdMiddleware::read_frozen_content(cwd) → frozen_claude_md
            → SkillsMiddleware::build_frozen_summary(cwd) → frozen_skill_summary
            → build_system_prompt(None, cwd, features, agent_dirs, Some(&frozen_date)) → frozen_system_prompt
            → SessionState.frozen_*
            → 后续所有 prompt 轮次直接使用 frozen 值
```

## 症状详情

### System Prompt 倍数膨胀

| 轮次 | sys[1] 总大小 | "Deferred Tools" 副本数 |
|------|-------------|----------------------|
| 8 | 69K chars | 1 |
| 9 | 145K chars | 2 |
| 11 | 298K chars | 4 |
| 12 | 374K chars | 5 |
| 55 | 451K chars | 6 |

## 相关 Issue

- `spec/issues/2026-05-20-auto-compact-empty-messages-400.md`
- `spec/issues/2026-05-20-compact-command-not-triggering.md`
