# ACP 事件 ↔ UI 能力映射表

## 一、主执行流程（prompt 实时推送）

| 方向 | ACP 方法/通知 | 数据载体 | TUI 显示 |
|------|-------------|---------|---------|
| TUI→ACP | `session/prompt` (req) | `{message: {content: "..."}}` | 用户输入提交 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::AgentMessageChunk` | 流式文本渲染 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::AgentThoughtChunk` | 推理/思考折叠显示 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::ToolCall(InProgress)` | 工具调用开始提示 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::ToolCallUpdate(Completed/Failed)` | 工具结果/错误显示 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::UsageUpdate` | Token 消耗统计 |
| ACP→TUI | `notifications/session_update` | `SessionUpdate::SessionInfoUpdate` | 重试状态提示 |
| ACP→TUI | `session/prompt` (resp) | `{stop_reason: ...}` | Agent 完成/取消 |

## 二、交互中断（HITL / AskUser）

| 方向 | ACP 方法/通知 | 数据载体 | TUI 显示 |
|------|-------------|---------|---------|
| ACP→TUI | `RequestPermission` (req) | `{tool_call, options: [allow_once, reject_once]}` | HITL 审批弹窗 |
| TUI→ACP | `RequestPermission` (resp) | `{outcome: Selected/Cancelled}` | 用户批准/拒绝 |
| ACP→TUI | `elicitation/create` (req) | `{form: MultiSelect/SingleSelect/Text}` | AskUser 问答弹窗 |
| TUI→ACP | `elicitation/create` (resp) | `{action: Accept/Decline, content: {...}}` | 用户应答 |

## 三、会话管理

| 方向 | ACP 方法/通知 | 数据载体 | TUI 显示 |
|------|-------------|---------|---------|
| TUI→ACP | `session/new` (req) | `{cwd, model}` | 新会话创建 |
| TUI→ACP | `session/set_model` (req) | `{model: "sonnet"}` | 模型切换 |
| TUI→ACP | `session/set_mode` (req) | `{mode: "auto"/"acceptEdits"}` | 权限模式切换 |
| TUI→ACP | `$/cancel_request` (notif) | `{session_id}` | 用户 Ctrl+C 取消 |

## 四、自定义通知（TUI 专用，非标准 ACP）

| 方向 | 自定义方法 | 数据 | TUI 显示 |
|------|----------|------|---------|
| ACP→TUI | `notifications/agent_event` | `{session_id, event: ExecutorEvent}` | TUI 完整事件处理（含 StateSnapshot、SubAgent、Compact 等内部事件） |

---

# 五、Peri 自定义事件命名空间设计

> **背景**：ACP 标准协议不包含 SubAgent、Compact、LSP、后台任务等概念。
> 当前这些事件通过 `notifications/agent_event` 携带完整的 `ExecutorEvent` JSON 传输，
> 但外部 ACP 客户端（IDE 插件等）无法消费这种私有格式。
> 以下方案定义 `peri/*` 命名空间的结构化通知，作为公开接口。

## 5.1 命名规范

所有自定义通知以 `notifications/peri/` 为前缀，按类别分层：

```
notifications/peri/<category>/<event>
```

| 类别 | 事件 | 通知方法 |
|------|------|---------|
| subagent | 开始执行 | `notifications/peri/subagent/start` |
| subagent | 执行完成 | `notifications/peri/subagent/end` |
| background | 后台任务完成 | `notifications/peri/background/completed` |
| compact | 上下文压缩开始 | `notifications/peri/compact/start` |
| compact | 上下文压缩完成 | `notifications/peri/compact/end` |
| lsp | 诊断更新 | `notifications/peri/lsp/diagnostics` |
| session | 会话结束 | `notifications/peri/session/ended` |

## 5.2 Payload 定义

### 5.2.1 `notifications/peri/subagent/start`

```json
{
  "session_id": "session-1",
  "agent_name": "code-reviewer"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | 父会话 ID |
| `agent_name` | string | 子 agent 类型名（= `subagent_type` 参数值），如 `code-reviewer`、`explore` |

**对应内部事件**：`ExecutorEvent::SubagentStarted { agent_name }`

**TUI 行为**：弹出 SubAgent 执行块（`MessageViewModel::SubAgentGroup { is_running: true }`），内部展示步骤计数和流式消息。

---

### 5.2.2 `notifications/peri/subagent/end`

```json
{
  "session_id": "session-1",
  "agent_name": "code-reviewer",
  "result": "Found 3 issues in src/main.rs...",
  "is_error": false
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | 父会话 ID |
| `agent_name` | string | 子 agent 类型名 |
| `result` | string | 执行结果摘要（工具返回值，截断 500 字符） |
| `is_error` | bool | 是否异常结束 |

**对应内部事件**：`ExecutorEvent::SubagentStopped { agent_name, result, is_error }`

**TUI 行为**：冻结 SubAgentGroup 为完成状态，`final_result = Some(result)`，`is_running = false`。连续多个完成的 SubAgentGroup 会自动聚合成批次汇总视图。

---

### 5.2.3 `notifications/peri/background/completed`

```json
{
  "session_id": "session-1",
  "task_id": "bg-550e8400-e29b-41d4-a716-446655440000",
  "agent_name": "code-reviewer",
  "prompt_summary": "Review the changes in src/",
  "success": true,
  "output": "Background task code-reviewer completed. Result: ...",
  "tool_calls_count": 5,
  "duration_ms": 12345
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | 父会话 ID |
| `task_id` | string | 后台任务唯一 ID |
| `agent_name` | string | agent 类型名 |
| `prompt_summary` | string | 任务描述摘要 |
| `success` | bool | 是否成功 |
| `output` | string | 完整输出文本 |
| `tool_calls_count` | number | 子 agent 执行的工具调用次数 |
| `duration_ms` | number | 执行耗时（毫秒） |

**对应内部事件**：`ExecutorEvent::BackgroundTaskCompleted(BackgroundTaskResult)`

**TUI 行为**：在消息流中插入 SubAgentGroup 完成块。若父 agent 仍在运行，延迟到下一帧提交 continuation 消息；若父 agent 已完成，直接插入。

---

### 5.2.4 `notifications/peri/compact/start`

```json
{
  "session_id": "session-1"
}
```

**对应内部事件**：`ExecutorEvent::CompactStarted`

**TUI 行为**：当前无 UI 反馈（内部事件，不渲染）。

---

### 5.2.5 `notifications/peri/compact/end`

```json
{
  "session_id": "session-1"
}
```

**对应内部事件**：`ExecutorEvent::CompactCompleted`

**TUI 行为**：当前无 UI 反馈。

---

### 5.2.6 `notifications/peri/lsp/diagnostics`

```json
{
  "session_id": "session-1",
  "errors": 3,
  "warnings": 12,
  "files_with_errors": 2
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string | 会话 ID |
| `errors` | number | Error 级别诊断数 |
| `warnings` | number | Warning 级别诊断数 |
| `files_with_errors` | number | 有 Error 的文件数 |

**对应内部事件**：`ExecutorEvent::LspDiagnostics { errors, warnings, files_with_errors }`

**TUI 行为**：更新状态栏 LSP 计数显示。

---

### 5.2.7 `notifications/peri/session/ended`

```json
{
  "session_id": "session-1"
}
```

**对应内部事件**：`ExecutorEvent::SessionEnded`

**TUI 行为**：清理 Agent 状态，标记 session 结束。

---

## 5.3 实现位置

### 5.3.1 新增映射函数：`peri-acp/src/event/mapper.rs`

```rust
/// 将 ExecutorEvent 映射为 peri/* 自定义通知列表。
///
/// 每个元素为 `(method, params_json)`。
/// method 形如 `"notifications/peri/subagent/start"`。
/// params_json 除事件特有字段外，调用方需补充 `"session_id"`。
pub fn map_executor_to_peri_notifications(
    event: &ExecutorEvent,
) -> Vec<(&'static str, serde_json::Value)> {
    match event {
        ExecutorEvent::SubagentStarted { agent_name } => {
            vec![("notifications/peri/subagent/start", json!({
                "agent_name": agent_name,
            }))]
        }
        ExecutorEvent::SubagentStopped { agent_name, result, is_error } => {
            vec![("notifications/peri/subagent/end", json!({
                "agent_name": agent_name,
                "result": result,
                "is_error": is_error,
            }))]
        }
        ExecutorEvent::BackgroundTaskCompleted(r) => {
            vec![("notifications/peri/background/completed", json!({
                "task_id": r.task_id,
                "agent_name": r.agent_name,
                "prompt_summary": r.prompt_summary,
                "success": r.success,
                "output": r.output,
                "tool_calls_count": r.tool_calls_count,
                "duration_ms": r.duration_ms,
            }))]
        }
        ExecutorEvent::CompactStarted => {
            vec![("notifications/peri/compact/start", json!({}))]
        }
        ExecutorEvent::CompactCompleted => {
            vec![("notifications/peri/compact/end", json!({}))]
        }
        ExecutorEvent::LspDiagnostics { errors, warnings, files_with_errors } => {
            vec![("notifications/peri/lsp/diagnostics", json!({
                "errors": errors,
                "warnings": warnings,
                "files_with_errors": files_with_errors,
            }))]
        }
        ExecutorEvent::SessionEnded => {
            vec![("notifications/peri/session/ended", json!({}))]
        }
        _ => vec![],
    }
}
```

**位置**：`peri-acp/src/event/mapper.rs` 底部，与 `map_executor_to_updates` 同级。

---

### 5.3.2 修改通知发送循环：`peri-tui/src/acp_server.rs`

在现有事件泵中，`map_executor_to_updates` 之后追加 `map_executor_to_peri_notifications` 调用：

```rust
// 现有：发送 notifications/agent_event (TUI 向后兼容)
let event_value = serde_json::to_value(&exec_event).unwrap_or_default();
let _ = transport_clone
    .send_notification("notifications/agent_event", json!({
        "session_id": sid,
        "event": event_value,
    }))
    .await;

// 现有：发送 notifications/session_update (标准 ACP)
let updates = map_executor_to_updates(&exec_event, context_window_u32);
for update in updates { ... }

// 新增：发送 notifications/peri/* (自定义扩展)
let peri_notifs = map_executor_to_peri_notifications(&exec_event);
for (method, mut payload) in peri_notifs {
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert("session_id".to_string(), json!(sid));
    }
    let _ = transport_clone.send_notification(method, payload).await;
}
```

---

### 5.3.3 可选：acp_client 新增 `AcpNotification::Peri` 变体

`peri-tui/src/acp_client/client.rs` 的 pump 中新增匹配分支：

```rust
} else if method.starts_with("notifications/peri/") {
    let session_id = params.get("session_id")
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let _ = notification_tx.send(AcpNotification::Peri {
        session_id,
        method,
        params,
    });
}
```

`AcpNotification` 新增变体：

```rust
pub enum AcpNotification {
    // ... existing variants ...
    /// A peri/* custom notification (SubAgent, Compact, LSP, etc.)
    Peri {
        session_id: String,
        method: String,
        params: Value,
    },
}
```

TUI `handle_acp_notification` 新增路由：

```rust
AcpNotification::Peri { method, params, .. } => {
    match method.as_str() {
        "notifications/peri/subagent/start" => {
            let agent_name = params["agent_name"].as_str().unwrap_or("").to_string();
            self.handle_agent_event(AgentEvent::SubAgentStart {
                agent_id: agent_name,
                task_preview: String::new(),
                is_background: false,
            })
        }
        "notifications/peri/subagent/end" => {
            let agent_id = params["agent_name"].as_str().unwrap_or("").to_string();
            let result = params["result"].as_str().unwrap_or("").to_string();
            let is_error = params["is_error"].as_bool().unwrap_or(false);
            self.handle_agent_event(AgentEvent::SubAgentEnd {
                agent_id: Some(agent_id),
                result,
                is_error,
            })
        }
        "notifications/peri/background/completed" => {
            // deserialize into BackgroundTaskResult → BackgroundTaskDone
            // ...
        }
        // ... 其他方法 ...
        _ => (false, false, false),
    }
}
```

> **说明**：TUI 当前通过 `notifications/agent_event` 路径已能消费 SubAgent/Compact 事件，
> 因此 `Peri` 变体的路由是**可选的增强项**——它让 TUI 将来可以逐步迁移到结构化通知，
> 也为外部客户端提供解析参考。

---

## 5.4 数据流图

```
┌─────────────────────────────────────────────────────────────────┐
│ ReActAgent.execute()                                            │
│   │                                                             │
│   ├── ExecutorEvent::TextChunk ──→ map_executor_to_updates      │
│   │     └── SessionUpdate::AgentMessageChunk                    │
│   │         └── notifications/session_update  ──→ ACP Client    │
│   │                                                             │
│   ├── ExecutorEvent::SubagentStarted ──→ map_executor_to_peri   │
│   │     └── ("notifications/peri/subagent/start", {...})        │
│   │         └── send_notification  ──→ ACP Client               │
│   │                                                             │
│   └── (所有事件) ──→ notifications/agent_event                  │
│         └── {event: <完整 ExecutorEvent JSON>}  ──→ TUI (向后兼容)│
└─────────────────────────────────────────────────────────────────┘

ACP Client (TUI):
  notifications/agent_event     ──→ AcpNotification::AgentEvent
  notifications/session_update ──→ AcpNotification::SessionUpdate
  notifications/peri/*          ──→ AcpNotification::Peri  (新增)
```

## 5.5 与标准 ACP 的边界

| 事件类别 | 标准 ACP | Peri 自定义 | 说明 |
|---------|---------|-----------|------|
| 流式文本 | `AgentMessageChunk` | — | 标准已覆盖 |
| 思考内容 | `AgentThoughtChunk` | — | 标准已覆盖 |
| 工具调用 | `ToolCall`/`ToolCallUpdate` | — | 标准已覆盖 |
| Token 用量 | `UsageUpdate` | — | 标准已覆盖 |
| 重试状态 | `SessionInfoUpdate` | — | 标准已覆盖 |
| **SubAgent 生命周期** | — | `peri/subagent/start` `peri/subagent/end` | ACP 无此概念 |
| **后台任务完成** | — | `peri/background/completed` | ACP 无此概念 |
| **上下文压缩** | — | `peri/compact/start` `peri/compact/end` | ACP 无此概念 |
| **LSP 诊断** | — | `peri/lsp/diagnostics` | ACP 无此概念 |
| **会话结束** | — | `peri/session/ended` | 可映射为 prompt response 但语义不同 |

## 5.6 设计原则

1. **不入侵标准 ACP 类型**：`peri/*` 通知是独立于 `SessionUpdate` 的 JSON-RPC notification，不修改任何 `agent_client_protocol` crate 的类型定义
2. **payload 自包含**：每个通知的 JSON 对象包含该事件的所有展示所需字段，客户端无需查找关联事件
3. **`notifications/agent_event` 保持不变**：TUI 向后兼容路径不被破坏，新旧路径并行运行
4. **命名空间可扩展**：新增事件类型只需在 `map_executor_to_peri_notifications` 中添加 match 分支和命名
