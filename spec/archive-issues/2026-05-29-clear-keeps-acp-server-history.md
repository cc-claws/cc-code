> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-clear-keeps-acp-server-history.md
# /clear 后 ACP Server 端 history 未清理，新会话延续旧上下文

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-29

## 问题描述

用户在 TUI 中执行 `/clear`（或 `/reset`、`/new`）后，TUI 本地状态清空了，但 ACP Server 端的 `session.history` 未被清理。新的 prompt 发送到同一个 session 时，Agent 仍然能看到之前对话的完整上下文，导致"延续旧对话"的现象。

## 症状详情

| 维度 | 表现 |
|------|------|
| 用户操作 | 先进行一轮普通对话（来回几条消息），然后执行 `/clear` |
| TUI 界面 | 消息列表已清空，看起来是"新对话" |
| Agent 行为 | 在新对话中 Agent 回复引用了 `/clear` 之前对话的内容 |
| 根因 | ACP Server 端 `SessionState.history` 保留了旧消息 |

### 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，进行一轮普通对话
  2. 执行 `/clear`（或别名 `/reset`、`/new`）
  3. 发送新消息
  4. Agent 回复中引用了第一步对话的内容
- **环境**：TUI 模式，所有模型

## 涉及文件

- `peri-tui/src/app/thread_ops.rs` — `new_thread()` 清理 TUI 本地状态但未通知 ACP server
- `peri-tui/src/command/core/clear.rs` — `/clear` 命令调用 `app.new_thread()`
- `peri-tui/src/acp_server/mod.rs` — `SessionState` 的 `history` 字段

## 背景

在 P3（ACP Slash Commands）重构中，删除了 `client.clear()` 调用（原 `session/clear` RPC），因为 `/clear` 被设计为 UICommand（本地创建新 session）。但 `new_thread()` 实际上并未通过 ACP 创建新 session，只是清空了 TUI 本地的渲染状态。ACP server 端的 `state.history` 仍然保留旧消息。

## 修复

**根因**：`new_thread()` 没有清除 `acp_client.current_session_id`。下次 `submit_message()` 时 `has_session()` 返回 true → 复用旧 session → Agent 看到旧 history。

**修复**：在 `new_thread()` 中调用 `acp_client.reset_session()` 清除 session id，下次 submit 自动走 `client.new_session()` 创建全新 session。

**文件**：
- `peri-tui/src/acp_client/client.rs` — 新增 `reset_session()` 方法
- `peri-tui/src/app/thread_ops.rs` — `new_thread()` 中调用 `reset_session()`
