# LLM 流式错误导致 Agent 失忆

**状态**：Open
**优先级**：高
**类型**：Bug
**创建日期**：2026-05-29

## 问题描述

Agent 执行了多个工具调用并完成部分任务后，LLM 流式读取失败（`error decoding response body`）。界面上仍能看到之前的对话内容，但当用户输入"继续"后，Agent 回复"没有之前的上下文"，完全丧失当前轮次的所有记忆。与已修复的 Ctrl+C 中断失忆（`issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia`）症状相似，但触发条件不同——这次是 LLM 流式错���而非用户主动中断。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发条件 | LLM 流式读取失败：`error decoding response body` |
| 错误发生时机 | Agent 已完成工具调用（Read/Write/Bash 等），LLM 正在生成下一步回复时 |
| UI 表现 | 错误前 agent 的回复和工具调用结果仍可见 |
| 继续对话 | Agent 完全不记得之前的上下文，像新对话一样 |
| 复现频率 | 必现 |

### 用户可见输出

```
● Read

Thought for 769 chars
任务1已完成，但尚未提交。我先验证一下，然后提交，再继续处理任务

✗ Agent Error
  ⎿ LLM error: 流式读取失败: error decoding response body

❯ 继续

Thought for 311 chars
没有之前的上下文。你想让我做什么？
```

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 给 Agent 一个多步骤任务（如"完成以下 3 个任务并逐个提交"）
  2. Agent 开始执行，完成部分工具调用
  3. 等待 LLM 流式错误发生（网络波动、Provider 限流等）
  4. 输入"继续"
  5. Agent 失忆，不记得之前做过什么
- **环境**：所有模型、所有 OS

## 涉及文件

| 文件 | 角色 |
|------|------|
| `peri-tui/src/acp_server/prompt.rs` | Ctrl+C amnesia 的修复点，LLM 错误路径可能缺少相同的保护逻辑 |
| `peri-acp/src/session/executor.rs` | 共享 agent 执行管线，错误处理路径 |
| `peri-agent/src/agent/executor/mod.rs` | ReAct 循环中的 LLM 错误处理 |

## 关联 Issue

- `issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia`（已修复）——相同症状（agent 失忆），不同触发条件（Ctrl+C vs LLM 流式错误）。修复时检查了 cancel 路径的 history truncation，LLM error 路径可能需要同样的保护。

## 排查发现：同一根因影响更多场景

`prompt.rs:180` 的保护条件要求 `stop_reason == Cancelled`，导致所有 `stop_reason == EndTurn` 或 `MaxTurnRequests` 的错误路径在有进展时也会 truncate history。受影响的完整场景：

| 错误类型 | stop_reason | 有进展时 |
|---------|-------------|---------|
| LLM 流式/HTTP 错误 | EndTurn | 失忆 |
| LLM 重试耗尽 | EndTurn | 失忆 |
| 工具执行 deferred_error | EndTurn | 失忆 |
| 中间件错误（step > 0） | EndTurn | 失忆 |
| **MaxIterationsExceeded** | MaxTurnRequests | **失忆** |

其中 MaxIterationsExceeded 尤为严重：agent 执行了 N 轮工具调用后达到迭代上限，所有工作成果被丢弃。

修复方案见 `docs/designs/2026-05-29-fix-llm-error-amnesia.md`。
