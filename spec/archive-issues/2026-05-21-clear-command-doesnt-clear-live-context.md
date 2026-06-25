> 归档于 2026-05-24，原路径 spec/issues/2026-05-21-clear-command-doesnt-clear-live-context.md

# /clear 命令未清理 ACP Server 上下文

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-21
**修复日期**：2026-05-21

## 问题描述

执行 `/clear` 命令后，TUI 界面清空了，但实际 LLM 的上下文（messages 数组）仍在。发新消息时 LLM 仍能引用到 `/clear` 之前的内容。

## 症状详情

| 表现 | 说明 |
|------|------|
| 触发时机 | 执行 `/clear` 或 `/reset` 或 `/new` |
| UI 表现 | 消息列表清空 ✓ |
| LLM 上下文 | 旧消息仍在，LLM 继续引用历史内容 ✗ |
| 配置文件 | 无关 |

## 根因分析

### 数据流

```
/clear → new_thread()
  TUI 层清理:
    ✓ view_messages.clear()
    ✓ agent_state_messages.clear()
    ✓ pipeline.clear()
    ✓ current_thread_id = None
    ✓ RenderEvent::Clear

  ACP Server 层：
    SessionState.history: Vec<BaseMessage>  ← 未清理 ✗

submit_message():
  → ACP session/prompt
  → prompt.rs:88: state.history.clone()
  → 历史消息仍包含 /clear 之前的内容
  → LLM 收到完整上下文
```

### 关键代码位置

1. **`/clear` 命令** — `peri-tui/src/command/core/clear.rs:20`：调用 `app.new_thread()`
2. **`new_thread()`** — `peri-tui/src/app/thread_ops.rs:259-335`：清 TUI 层，未动 ACP Server
3. **ACP SessionState** — `peri-tui/src/acp_server/mod.rs:39-52`：`history: Vec<BaseMessage>` 未被清
4. **ACP prompt 执行** — `peri-tui/src/acp_server/prompt.rs:88`：`state.history.clone()` 直接使用完整历史

### 架构问题

`SharedSessions`（`Arc<tokio::sync::Mutex<HashMap<String, SessionState>>>`）由 ACP Server 持有，TUI 层无直接引用。`new_thread()` 只能清 TUI 侧的状态。

## 涉及文件

- `peri-tui/src/command/core/clear.rs:19-21` — `/clear` 命令入口
- `peri-tui/src/app/thread_ops.rs:259-335` — `new_thread()` 清 TUI 层
- `peri-tui/src/acp_server/mod.rs:39-52` — `SessionState.history`
- `peri-tui/src/acp_server/prompt.rs:88-155` — prompt 执行使用 `state.history`
- `peri-tui/src/acp_server/requests.rs` — ACP 请求处理（需新增 `session/clear`）

## 建议修复方向

在 `new_thread()` 中通过 ACP client 发送一个通知或请求，让 ACP Server 清空对应 session 的 `history`：

1. **新增 ACP 方法**：在 `acp_server/requests.rs` 中添加 `session/clear` 请求处理
2. **TUI 侧调用**：在 `new_thread()` 末尾调用 `acp_client.clear()`
3. **ACP Server 处理**：收到后清空 `SessionState.history`

## 修复方案

通过 ACP `session/clear` 请求同步清空 Server 端历史：

| 步骤 | 文件 | 变更 |
|------|------|------|
| ACP Server 处理 | `acp_server/requests.rs` | `handle_request()` 新增 `"session/clear"` → `state.history.clear()` |
| ACP Client 封装 | `acp_client/client.rs` | 新增 `clear()` 方法（参照 `compact()`） |
| TUI 调用点 | `app/thread_ops.rs` | `new_thread()` 末尾 `tokio::spawn` 调用 `acp_client.clear()` |

### 计划

`spec/issues/2026-05-21-clear-command-doesnt-clear-live-context-plan.md`
