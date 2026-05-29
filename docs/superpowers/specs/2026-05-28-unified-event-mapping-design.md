# Unified Event Mapping: peri/agent_event + session/update 混合模式

**日期**: 2026-05-28
**状态**: Draft

## 背景与动机

当前 `peri-acp` 存在两套独立的事件映射逻辑：

1. **`map_executor_to_updates()`**（`peri-acp/src/event/mapper.rs`）— ExecutorEvent → `Vec<SessionUpdate>`，供 Stdio/IDE 客户端消费
2. **`map_executor_event()`**（`peri-tui/src/app/agent.rs`）— ExecutorEvent → `Option<AgentEvent>`，供 TUI 消费
3. **`map_executor_to_peri_notifications()`**（同 mapper.rs）— ExecutorEvent → `peri/*` 自定义通知，TUI 当前忽略

三套映射逻辑维护成本高、容易不一致。目标：合并为单一 `map_event()` 函数，消除重复。

## 设计决策

**混合模式**：能映射到标准 `SessionUpdate` 的事件走 `session/update`，无法映射的事件保留 `peri/agent_event`。不引入新的枚举类型（避免与 `ExecutorEvent` 重复）。

**理由**：
- 标准 `SessionUpdate` 无法覆盖所有 TUI 需要的事件（StateSnapshot、SubAgent、Compact 等）
- 不向上游 ACP 规范提 proposal（周期长、可能被拒）
- 不引入 `PeriExtension` 枚举（本质是复制 ExecutorEvent 变体，无意义）

## 三类事件分区

`map_event()` 统一函数将 ExecutorEvent 分为三类：

| 类别 | 事件 | TUI 通道 | Stdio 通道 |
|------|------|----------|-----------|
| **① Full SessionUpdate** | TextChunk, AiReasoning, ToolStart, ToolEnd, TodoUpdate | `session/update` | `session/update` |
| **② Lossy SessionUpdate** | LlmCallEnd(usage), ContextWarning, LlmRetrying | `peri/agent_event`（TUI 需完整数据） | `session/update`（IDE 只需标准字段） |
| **③ No SessionUpdate** | StateSnapshot, SubagentStarted/Stopped, CompactStarted/Completed/Error, BackgroundTaskCompleted, LspDiagnostics, AgentExecutionFailed | `peri/agent_event` | 不发 |

**类别②说明**：`SessionUpdate::UsageUpdate` 丢失了 `model`、`percentage` 等字段，`SessionInfoUpdate` 丢失了 `error` 细节。TUI 需要这些完整数据，因此继续通过 `peri/agent_event` 发送原始 ExecutorEvent。Stdio/IDE 客户端只需要标准字段，走 `session/update`。

## 统一映射输出

```rust
// peri-acp/src/event/mapper.rs

/// 统一映射结果
pub struct MappedEvent {
    /// 标准 ACP SessionUpdate（类别①②有值）
    pub updates: Vec<SessionUpdate>,
    /// 是否通过 peri/agent_event 转发原始 ExecutorEvent（类别②③为 true）
    pub forward_to_tui: bool,
    /// 附加在 session/update 通知 params 中的 source_agent_id
    /// 标准 ACP 客户端忽略此字段
    pub source_agent_id: Option<String>,
}

/// 统一映射函数，替代 map_executor_to_updates() + map_executor_to_peri_notifications() + map_executor_event()
pub fn map_event(event: &ExecutorEvent, context_window: u32) -> Vec<MappedEvent>
```

### 完整映射表

| ExecutorEvent | updates (SessionUpdate) | forward_to_tui | source_agent_id | 类别 |
|---|---|---|---|---|
| `TextChunk { chunk, source_agent_id, .. }` | `[AgentMessageChunk]` | `false` | 提取 | ① |
| `AiReasoning(text)` | `[AgentThoughtChunk]` | `false` | `None` | ① |
| `ToolStart { tool_call_id, name, input, source_agent_id, .. }` | `[ToolCall]` | `false` | 提取 | ① |
| `ToolEnd { tool_call_id, output, is_error, source_agent_id, .. }` | `[ToolCallUpdate]` | `false` | 提取 | ① |
| `TodoUpdate(entries)` | `[Plan]` | `false` | `None` | ① |
| `LlmCallEnd { usage: Some, .. }` | `[UsageUpdate]` | `true` | `None` | ② |
| `ContextWarning { .. }` | `[UsageUpdate]` | `true` | `None` | ② |
| `LlmRetrying { .. }` | `[SessionInfoUpdate]` | `true` | `None` | ② |
| `LlmCallEnd { usage: None, .. }` | `[]` | `false` | `None` | 过滤 |
| `StateSnapshot(_)` | `[]` | `true` | `None` | ③ |
| `SubagentStarted { .. }` | `[]` | `true` | `None` | ③ |
| `SubagentStopped { .. }` | `[]` | `true` | `None` | ③ |
| `CompactStarted` | `[]` | `true` | `None` | ③ |
| `CompactCompleted { .. }` | `[]` | `true` | `None` | ③ |
| `CompactError { .. }` | `[]` | `true` | `None` | ③ |
| `BackgroundTaskCompleted(_)` | `[]` | `true` | `None` | ③ |
| `LspDiagnostics { .. }` | `[]` | `true` | `None` | ③ |
| `AgentExecutionFailed { .. }` | `[]` | `true` | `None` | ③ |
| `StepDone`, `MessageAdded`, `LlmCallStart`, `SessionEnded` | `[]` | `false` | `None` | 过滤 |

## session/update 通知 params 扩展

TUI 路径的 `session/update` 通知 params 增加 `_peri` 私有扩展字段：

```json
{
  "sessionId": "xxx",
  "update": { "/* 标准 SessionUpdate */" },
  "_peri": {
    "sourceAgentId": "yyy"
  }
}
```

- `_` 前缀为业界惯例的私有扩展，标准 ACP 客户端忽略未知字段
- TUI bridge 读取 `_peri.sourceAgentId` 用于 SubAgent 事件路由
- StdioEventSink 不发送 `_peri` 字段

## Transport 层变更

### TransportEventSink（TUI 路径）

```
for each MappedEvent:
  if !updates.is_empty():
    send "session/update" notification (带 _peri.sourceAgentId)
  if forward_to_tui:
    send "peri/agent_event" notification (原 ExecutorEvent 序列化)
```

### StdioEventSink（Stdio 路径）

```
for each MappedEvent:
  if !updates.is_empty():
    send "session/update" notification (不带 _peri)
  // forward_to_tui 被忽略
```

## TUI Bridge 变更

### 新流程

```
session/update   → handle_session_update() → UI 更新
peri/agent_event → map_executor_event()    → AgentEvent → handle_agent_event()
```

### handle_session_update()（新增）

处理类别①事件，替代原 `AgentEvent` 中对应的变体：

```rust
fn handle_session_update(
    app: &mut App,
    session_id: &str,
    update: SessionUpdate,
    peri_meta: Option<Value>,  // _peri 字段
) {
    let source_agent_id = peri_meta
        .as_ref()
        .and_then(|p| p.get("sourceAgentId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            // 替代 AgentEvent::AssistantChunk
            // source_agent_id 从 peri_meta 获取
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            // 替代 AgentEvent::AiReasoning
        }
        SessionUpdate::ToolCall(tool_call) => {
            // 替代 AgentEvent::ToolStart
            // display/args 格式化在此处完成：
            //   let display = format_tool_name(&tool_call.name);
            //   let args = format_tool_args(&tool_call.name, &tool_call.raw_input, cwd);
        }
        SessionUpdate::ToolCallUpdate(update) => {
            // 替代 AgentEvent::ToolEnd
            // output 截断在此处完成：
            //   let output = truncate(&output, 200);
        }
        SessionUpdate::Plan(plan) => {
            // 替代 AgentEvent::TodoUpdate
            // PlanEntryStatus → TuiTodoStatus 转换在此处
        }
        SessionUpdate::UsageUpdate(_) | SessionUpdate::SessionInfoUpdate(_) => {
            // TUI 路径忽略（完整数据通过 peri/agent_event 的类别②获取）
        }
        _ => {}
    }
}
```

### map_executor_event() 简化

类别①②事件返回 `None`，仅保留类别③映射：

```rust
pub(crate) fn map_executor_event(event: ExecutorEvent, cwd: &str) -> Option<AgentEvent> {
    Some(match event {
        // ── 类别③：无 SessionUpdate 映射，仍通过 peri/agent_event ──
        ExecutorEvent::StateSnapshot(msgs) => AgentEvent::StateSnapshot(msgs),
        ExecutorEvent::SubagentStarted { agent_name, instance_id, is_background } =>
            AgentEvent::SubAgentStart { agent_id: agent_name.clone(), instance_id, task_preview: String::new(), is_background },
        ExecutorEvent::SubagentStopped { agent_name, result, is_error, instance_id } =>
            AgentEvent::SubAgentEnd { agent_id: Some(agent_name), instance_id: Some(instance_id), result, is_error },
        ExecutorEvent::CompactStarted => AgentEvent::CompactStarted,
        ExecutorEvent::CompactCompleted { summary, files, skills, micro_cleared, messages } =>
            AgentEvent::CompactCompleted { summary, files, skills, micro_cleared, messages },
        ExecutorEvent::CompactError { message } => AgentEvent::CompactError(message),
        ExecutorEvent::BackgroundTaskCompleted(result) => AgentEvent::BackgroundTaskCompleted { /* 字段映射 */ },
        ExecutorEvent::LspDiagnostics { errors, warnings, files_with_errors } =>
            AgentEvent::LspDiagnostics { errors, warnings, files_with_errors },
        ExecutorEvent::AgentExecutionFailed { message } => {
            if message == "Interrupted by user" { AgentEvent::Interrupted }
            else { AgentEvent::Error(message) }
        }

        // ── 类别①：已有 SessionUpdate 映射，TUI 通过 session/update 处理 ──
        ExecutorEvent::TextChunk { .. }
        | ExecutorEvent::AiReasoning(_)
        | ExecutorEvent::ToolStart { .. }
        | ExecutorEvent::ToolEnd { .. }
        | ExecutorEvent::TodoUpdate(_)

        // ── 过滤：不转发 ──
        | ExecutorEvent::StepDone { .. }
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. }
        | ExecutorEvent::SessionEnded => return None,
    })
}
```

### AgentEvent 变体变化

| AgentEvent 变体 | 变化 | 替代方案 |
|---|---|---|
| `AssistantChunk` | **删除** | `SessionUpdate::AgentMessageChunk` + `peri_meta.sourceAgentId` |
| `AiReasoning` | **删除** | `SessionUpdate::AgentThoughtChunk` |
| `ToolStart` (含 display/args) | **删除** | `SessionUpdate::ToolCall` + TUI 侧 format_tool_name/format_tool_args |
| `ToolEnd` (含截断格式化) | **删除** | `SessionUpdate::ToolCallUpdate` + TUI 侧 truncate |
| `TodoUpdate` | **删除** | `SessionUpdate::Plan` + TUI 侧 PlanEntryStatus → TuiTodoStatus |
| `ContextWarning` | **保留** | 类别②，仍通过 `peri/agent_event`，map_executor_event 保留映射 |
| `TokenUsageUpdate` | **保留** | 类别②，同上 |
| `LlmRetrying` | **保留** | 类别②，同上 |
| `StateSnapshot` | **保留** | 类别③ |
| `SubAgentStart/End` | **保留** | 类别③ |
| `CompactStarted/Completed/Error` | **保留** | 类别③ |
| `BackgroundTaskCompleted` | **保留** | 类别③ |
| `LspDiagnostics` | **保留** | 类别③ |
| `Interrupted` / `Error` | **保留** | 类别③ |
| `Done` | **保留** | 来自 AgentDone 通知 |
| `InteractionRequest` 等 | **保留** | 非 ExecutorEvent 来源（HITL/MCP/Plugin） |

### acp_bridge.rs 变更

```rust
fn handle_acp_notification(notif: AcpNotification, app: &mut App) {
    match notif {
        AcpNotification::SessionUpdate { session_id, params } => {
            // 解析 params.update + params._peri
            // 调用 handle_session_update()
        }
        AcpNotification::AgentEvent { session_id, event } => {
            // 调用 map_executor_event()（仅类别③事件会返回 Some）
            if let Some(agent_event) = map_executor_event(event, &cwd) {
                handle_agent_event(app, agent_event);
            }
        }
        // AgentDone, RequestPermission, Elicitation 等不变
        ..
    }
}
```

## 删除清单

| 文件 | 删除内容 |
|---|---|
| `peri-acp/src/event/mapper.rs` | `map_executor_to_updates()`、`map_executor_to_peri_notifications()` → 替换为 `map_event()` |
| `peri-tui/src/app/agent.rs` | `map_executor_event()` 中类别①②映射 → 简化为仅类别③ |
| `peri-acp/src/session/event_sink.rs` | TransportEventSink/StdioEventSink 重构为使用 `map_event()` |

## AcpNotification 变体影响

`AcpNotification` 枚举结构不变：
- `AgentEvent` — 继续使用，但 TUI bridge 仅处理类别③事件
- `SessionUpdate` — 从"被忽略"变为"被处理"
- `Peri` — 可删除（`map_event()` 吸收了 `map_executor_to_peri_notifications()` 的职责）
- 其余不变

## 兼容性

- **Stdio/IDE 客户端**：无影响，仍只消费 `session/update`
- **TUI**：需同时处理 `session/update`（类别①）和 `peri/agent_event`（类别②③）
- **ACP 规范兼容**：`session/update` 通知格式符合标准，`_peri` 为私有扩展
- **向后兼容**：不支持 `_peri` 的客户端忽略该字段

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| TUI bridge 双路径处理增加复杂度 | 两个 handler 职责清晰：session/update 处理标准事件，peri/agent_event 处理扩展事件 |
| 类别②事件 TUI 通过 peri/agent_event 拿完整数据，同时 TransportEventSink 也发 session/update | TUI bridge 中 session/update handler 对 UsageUpdate/SessionInfoUpdate 跳过处理（因为完整数据已通过 peri/agent_event 的类别② AgentEvent 获取） |
| `format_tool_name`/`format_tool_args` 从 map_executor_event 移到 TUI handler | 这些函数已在 `peri-tui/src/app/tool_display.rs`，移入 handler 是自然归位 |
| `source_agent_id` 通过 `_peri` 传递，非标准 | `_peri` 是可选字段，标准客户端忽略；未来若 ACP 规范增加类似字段可平滑迁移 |
