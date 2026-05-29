# Unified Event Mapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 合并 `map_executor_to_updates()`, `map_executor_event()`, `map_executor_to_peri_notifications()` 为单一 `map_event()` 函数，TUI bridge 对标准事件消费 `session/update` 转为 AgentEvent。

**Architecture:** 新增 `MappedEvent` 结构体作为统一映射输出，TransportEventSink/StdioEventSink 使用 `map_event()` 替代三套旧映射；TUI bridge 新增 Peri 模式下的 `session/update` → `AgentEvent` 转换，再走现有 `handle_agent_event()` pipeline。

**Tech Stack:** Rust, ACP schema crate (`agent-client-protocol-schema`), serde_json, tokio

**Design doc:** `docs/superpowers/specs/2026-05-28-unified-event-mapping-design.md`

---

## File Structure

| 文件 | 职责 | 操作 |
|------|------|------|
| `peri-acp/src/event/mapper.rs` | `MappedEvent` + `map_event()` 统一映射函数 | **重写** |
| `peri-acp/src/event/mod.rs` | Re-export mapper | **微调** |
| `peri-acp/src/session/event_sink.rs` | TransportEventSink/StdioEventSink 使用 `map_event()` | **重写** |
| `peri-tui/src/app/agent.rs` | `map_executor_event()` 简化为仅类别③ | **编辑** |
| `peri-tui/src/app/agent_ops/acp_bridge.rs` | Bridge 增加 Peri 模式 session/update 消费 | **编辑** |
| `peri-tui/src/app/agent_ops/session_update.rs` | 新增 Peri 模式 session/update → AgentEvent 转换 | **新增方法** |
| `peri-tui/src/acp_client/client.rs` | 无需变更 | **不变** |
| `peri-tui/src/app/events.rs` | 无需变更（AgentEvent 保留所有变体） | **不变** |
| `peri-tui/src/app/agent_ops/mod.rs` | 无需变更（handle_agent_event 不变） | **不变** |
| `peri-agent/src/agent/events.rs` | 无需变更 | **不变** |

---

### Task 1: 创建 `MappedEvent` 和 `map_event()`

**Files:**
- Rewrite: `peri-acp/src/event/mapper.rs`
- Modify: `peri-acp/src/event/mod.rs`

- [ ] **Step 1: 重写 mapper.rs 为 MappedEvent + map_event()**

```rust
//! Event mapping from ExecutorEvent to ACP SessionUpdate + peri/agent_event routing.
//!
//! Produces [`MappedEvent`] — a unified output that both [`TransportEventSink`]
//! (TUI) and [`StdioEventSink`] (stdio) consume.

use agent_client_protocol::schema::{
    ContentBlock, ContentChunk, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    SessionInfoUpdate, SessionUpdate, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, UsageUpdate,
};
use peri_agent::agent::events::AgentEvent as ExecutorEvent;

/// 统一映射输出：标准 SessionUpdate 列表 + 是否需 TUI 转发 + source_agent_id
pub struct MappedEvent {
    /// 标准 ACP SessionUpdate 列表（类别①②有值，其余为空）
    pub updates: Vec<SessionUpdate>,
    /// 是否通过 peri/agent_event 转发原始 ExecutorEvent
    /// true = 类别②（TUI 需完整数据，同时 Stdio 通过 updates 获取标准字段）
    ///       + 类别③（无 SessionUpdate 映射，TUI 独有）
    pub forward_to_tui: bool,
    /// Stream events 的 source_agent_id（用于 SubAgent 路由）
    pub source_agent_id: Option<String>,
}

impl MappedEvent {
    fn standard(updates: Vec<SessionUpdate>) -> Self {
        Self { updates, forward_to_tui: false, source_agent_id: None }
    }

    fn standard_with_src(updates: Vec<SessionUpdate>, source_agent_id: Option<String>) -> Self {
        Self { updates, forward_to_tui: false, source_agent_id }
    }

    fn tui_only() -> Self {
        Self { updates: vec![], forward_to_tui: true, source_agent_id: None }
    }

    fn both(updates: Vec<SessionUpdate>) -> Self {
        Self { updates, forward_to_tui: true, source_agent_id: None }
    }

    fn none() -> Self {
        Self { updates: vec![], forward_to_tui: false, source_agent_id: None }
    }
}

/// 统一映射函数，替代 map_executor_to_updates() + map_executor_to_peri_notifications()
///
/// `context_window` 用于填充 UsageUpdate.size。
pub fn map_event(event: &ExecutorEvent, context_window: u32) -> Vec<MappedEvent> {
    match event {
        // ── 类别①：Full SessionUpdate ──
        ExecutorEvent::TextChunk { chunk, source_agent_id, .. } => {
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::Text(TextContent::new(chunk.clone())),
                ))],
                source_agent_id.clone(),
            )]
        }
        ExecutorEvent::AiReasoning(text) => {
            vec![MappedEvent::standard(vec![SessionUpdate::AgentThoughtChunk(
                ContentChunk::new(ContentBlock::Text(TextContent::new(text.clone()))),
            )])]
        }
        ExecutorEvent::ToolStart { tool_call_id, name, input, source_agent_id, .. } => {
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::ToolCall(
                    ToolCall::new(tool_call_id.clone(), name.clone())
                        .kind(infer_tool_kind(name))
                        .status(ToolCallStatus::InProgress)
                        .raw_input(Some(input.clone())),
                )],
                source_agent_id.clone(),
            )]
        }
        ExecutorEvent::ToolEnd { tool_call_id, output, is_error, source_agent_id, .. } => {
            let raw_output = match serde_json::from_str::<serde_json::Value>(output) {
                Ok(v) => Some(v),
                Err(_) => Some(serde_json::Value::String(output.clone())),
            };
            vec![MappedEvent::standard_with_src(
                vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    tool_call_id.clone(),
                    ToolCallUpdateFields::new()
                        .status(if *is_error { ToolCallStatus::Failed } else { ToolCallStatus::Completed })
                        .raw_output(raw_output),
                ))],
                source_agent_id.clone(),
            )]
        }
        ExecutorEvent::TodoUpdate(entries) => {
            let plan_entries: Vec<PlanEntry> = entries.iter().map(|e| {
                PlanEntry::new(
                    e.content.clone(),
                    PlanEntryPriority::Medium,
                    match e.status {
                        peri_agent::agent::events::TodoStatus::Pending => PlanEntryStatus::Pending,
                        peri_agent::agent::events::TodoStatus::InProgress => PlanEntryStatus::InProgress,
                        peri_agent::agent::events::TodoStatus::Completed => PlanEntryStatus::Completed,
                    },
                )
            }).collect();
            vec![MappedEvent::standard(vec![SessionUpdate::Plan(Plan::new(plan_entries))])]
        }

        // ── 类别②：Lossy SessionUpdate（IDE 用标准字段，TUI 用原始事件）──
        ExecutorEvent::LlmCallEnd { usage: Some(u), .. } => {
            let updates = vec![SessionUpdate::UsageUpdate(UsageUpdate::new(
                u64::from(u.input_tokens) + u64::from(u.output_tokens),
                u64::from(context_window),
            ))];
            vec![MappedEvent { updates, forward_to_tui: true, source_agent_id: None }]
        }
        ExecutorEvent::ContextWarning { used_tokens, total_tokens, .. } => {
            let updates = vec![SessionUpdate::UsageUpdate(UsageUpdate::new(
                *used_tokens,
                *total_tokens,
            ))];
            vec![MappedEvent { updates, forward_to_tui: true, source_agent_id: None }]
        }
        ExecutorEvent::LlmRetrying { attempt, max_attempts, delay_ms, .. } => {
            let updates = vec![SessionUpdate::SessionInfoUpdate(
                SessionInfoUpdate::new().title(format!(
                    "Retrying LLM call (attempt {}/{}, {}ms delay)",
                    attempt, max_attempts, delay_ms
                )),
            )];
            vec![MappedEvent { updates, forward_to_tui: true, source_agent_id: None }]
        }

        // ── 类别③：无 SessionUpdate 映射 — TUI 独有 ──
        ExecutorEvent::StateSnapshot(_)
        | ExecutorEvent::SubagentStarted { .. }
        | ExecutorEvent::SubagentStopped { .. }
        | ExecutorEvent::CompactStarted
        | ExecutorEvent::CompactCompleted { .. }
        | ExecutorEvent::CompactError { .. }
        | ExecutorEvent::BackgroundTaskCompleted(_)
        | ExecutorEvent::LspDiagnostics { .. }
        | ExecutorEvent::AgentExecutionFailed { .. } => {
            vec![MappedEvent::tui_only()]
        }

        // ── 过滤：不转发 ──
        ExecutorEvent::LlmCallEnd { usage: None, .. }
        | ExecutorEvent::StepDone { .. }
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. }
        | ExecutorEvent::SessionEnded => {
            vec![MappedEvent::none()]
        }
    }
}

fn infer_tool_kind(name: &str) -> ToolKind {
    match name {
        "Read" => ToolKind::Read,
        "Write" | "Edit" | "folder_operations" => ToolKind::Edit,
        "Bash" => ToolKind::Execute,
        "Grep" | "Glob" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

#[cfg(test)]
#[path = "mapper_test.rs"]
mod tests;
```

- [ ] **Step 2: 更新 mod.rs 导出**

```rust
// peri-acp/src/event/mod.rs
//! Event mapping from ExecutorEvent to ACP SessionUpdate and peri/agent_event routing.

pub mod mapper;
pub use mapper::{map_event, MappedEvent};
```

- [ ] **Step 3: 运行编译验证**

```bash
cargo build -p peri-acp
```
Expected: 编译通过，旧函数调用处会有编译错误（因为 `map_executor_to_updates()` / `map_executor_to_peri_notifications()` 已被删除）

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/event/mapper.rs peri-acp/src/event/mod.rs
git commit -m "feat: add MappedEvent + map_event() unified event mapper"
```

---

### Task 2: 更新 TransportEventSink 使用 `map_event()`

**Files:**
- Modify: `peri-acp/src/session/event_sink.rs` (TransportEventSink impl)

- [ ] **Step 1: 替换 TransportEventSink 的 push_event 实现**

```rust
// peri-acp/src/session/event_sink.rs

use crate::event::{map_event, MappedEvent};
// 删除原有 import:
// use crate::event::{map_executor_to_peri_notifications, map_executor_to_updates};

#[async_trait]
impl EventSink for TransportEventSink {
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        let mapped = map_event(event, context_window);

        for m in mapped {
            // 1. session/update — standard ACP notifications
            for update in m.updates {
                let mut payload = match serde_json::to_value(&update) {
                    Ok(p) => p,
                    Err(e) => {
                        error!(error = %e, "EventSink: serialize SessionUpdate failed");
                        continue;
                    }
                };
                if let serde_json::Value::Object(ref mut map) = payload {
                    map.insert("sessionId".to_string(), json!(session_id));
                    // Inject _peri metadata for TUI consumption (source_agent_id)
                    if let Some(ref aid) = m.source_agent_id {
                        map.insert(
                            "_peri".to_string(),
                            json!({ "sourceAgentId": aid }),
                        );
                    }
                }
                let _ = self.transport
                    .send_notification("session/update", payload)
                    .await;
            }

            // 2. peri/agent_event — TUI-specific events (类别②③)
            if m.forward_to_tui {
                let event_json = match serde_json::to_string(event) {
                    Ok(s) => s,
                    Err(e) => {
                        error!(error = %e, "EventSink: serialize ExecutorEvent failed");
                        continue;
                    }
                };
                let agent_event_params = json!({
                    "sessionId": session_id,
                    "event_json": event_json,
                });
                if let Err(e) = self.transport
                    .send_notification("peri/agent_event", agent_event_params)
                    .await
                {
                    error!(error = %e, "EventSink: send peri/agent_event failed");
                }
            }
        }
    }

    async fn push_done(&self, session_id: &str) {
        debug!(session_id = %session_id, "EventSink: sending agent_event_done");
        if let Err(e) = self
            .transport
            .send_notification("peri/agent_event_done", json!({ "sessionId": session_id }))
            .await
        {
            error!(session_id = %session_id, error = %e, "EventSink: agent_event_done send failed")
        }
    }
}
```

- [ ] **Step 2: 运行编译验证**

```bash
cargo build -p peri-acp
```
Expected: TransportEventSink 编译通过，不再引用旧函数

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/session/event_sink.rs
git commit -m "feat: TransportEventSink uses map_event() with _peri metadata"
```

---

### Task 3: 更新 StdioEventSink 使用 `map_event()`

**Files:**
- Modify: `peri-acp/src/session/event_sink.rs` (StdioEventSink impl)

- [ ] **Step 1: 替换 StdioEventSink 的 push_event 实现**

```rust
#[async_trait]
impl EventSink for StdioEventSink {
    async fn push_event(&self, _session_id: &str, event: &ExecutorEvent, context_window: u32) {
        let mapped = map_event(event, context_window);
        for m in mapped {
            for update in m.updates {
                let notif = SessionNotification::new(self.session_id.clone(), update);
                if let Err(e) = self.cx.send_notification(notif) {
                    error!(error = %e, "StdioEventSink: failed to send SessionNotification");
                    break;
                }
            }
        }
    }

    async fn push_done(&self, _session_id: &str) {
        // No explicit done signal in standard ACP protocol.
    }
}
```

- [ ] **Step 2: 运行编译验证**

```bash
cargo build -p peri-acp
```
Expected: 整个 `peri-acp` crate 编译通过

- [ ] **Step 3: 确认 peri-acp 不再引用旧函数**

```bash
cargo build -p peri-acp --all-targets
```
Expected: 编译通过，无 `map_executor_to_updates` 或 `map_executor_to_peri_notifications` 残留引用。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/event_sink.rs
git commit -m "feat: StdioEventSink uses map_event()"
```

---

### Task 4: 新增 Peri 模式 session/update → AgentEvent 桥接

**Files:**
- Modify: `peri-tui/src/app/agent_ops/session_update.rs`

TUI Peri 模式收到 `session/update` 后，解析为 AgentEvent 走现有 pipeline。在已有 External 模式 handler **之后**追加 Peri 模式方法。

- [ ] **Step 1: 在 session_update.rs 末尾追加 Peri 模式方法**

```rust
// 追加到 session_update.rs 末尾

use crate::app::AgentEvent;
use peri_agent::agent::events::{CompactFileInfo, TodoEntry, TodoStatus};

impl App {
    /// Peri 模式：将 session/update JSON 转换为 AgentEvent 并派发
    ///
    /// 与 External 模式不同：External 模式直接操作 view_messages；
    /// Peri 模式转换为 AgentEvent 走 handle_agent_event() → pipeline。
    pub(crate) fn handle_session_update_peri(
        &mut self,
        params: &serde_json::Value,
    ) -> (bool, bool, bool) {
        let update = match params.get("update") {
            Some(u) => u,
            None => {
                warn!("SessionUpdate missing 'update' field");
                return (false, false, false);
            }
        };

        // 提取 _peri.sourceAgentId
        let source_agent_id = params
            .get("_peri")
            .and_then(|p| p.get("sourceAgentId"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let update_type = update.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match update_type {
            "agent_message_chunk" => {
                let chunk = update
                    .get("content")
                    .and_then(|c| c.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !chunk.is_empty() {
                    self.handle_agent_event(AgentEvent::AssistantChunk {
                        chunk: chunk.to_string(),
                        source_agent_id,
                    })
                } else {
                    (false, false, false)
                }
            }
            "agent_thought_chunk" => {
                let text = update
                    .get("content")
                    .and_then(|c| c.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !text.is_empty() {
                    self.handle_agent_event(AgentEvent::AiReasoning(text.to_string()))
                } else {
                    (false, false, false)
                }
            }
            "tool_call" => {
                let tool_call_id = update
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = update
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = update
                    .get("rawInput")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                // TUI-specific formatting (从原 map_executor_event 移入)
                let display = super::super::super::tool_display::format_tool_name(&name);
                let args = super::super::super::tool_display::format_tool_args(
                    &name,
                    &input,
                    Some(&self.services.cwd),
                )
                .unwrap_or_default();

                self.handle_agent_event(AgentEvent::ToolStart {
                    tool_call_id,
                    name,
                    display,
                    args,
                    input,
                    source_agent_id,
                })
            }
            "tool_call_update" => {
                let tool_call_id = update
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = String::new(); // ToolCallUpdate 不携带 name，TUI 仅用于显示
                let raw_output = update
                    .get("rawOutput")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let is_error = update
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "failed")
                    .unwrap_or(false);

                // TUI-specific 截断（从原 map_executor_event 移入）
                let output = if is_error {
                    format!("✗ {}", truncate(raw_output, 60))
                } else {
                    truncate(raw_output, 200)
                };

                self.handle_agent_event(AgentEvent::ToolEnd {
                    tool_call_id,
                    name,
                    output,
                    is_error,
                    source_agent_id,
                })
            }
            "plan" => {
                let entries = update.get("entries").and_then(|v| v.as_array());
                let mut todos = Vec::new();
                if let Some(entries) = entries {
                    for entry in entries {
                        let content = entry
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let status_str = entry
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pending");
                        let status = match status_str {
                            "in_progress" => peri_middlewares::prelude::TodoStatus::InProgress,
                            "completed" => peri_middlewares::prelude::TodoStatus::Completed,
                            _ => peri_middlewares::prelude::TodoStatus::Pending,
                        };
                        todos.push(peri_middlewares::prelude::TodoItem {
                            content,
                            status,
                            active_form: None,
                        });
                    }
                }
                self.handle_agent_event(AgentEvent::TodoUpdate(todos))
            }
            "usage_update" | "session_info_update" => {
                // Peri 模式忽略 — 完整数据通过 peri/agent_event（类别②）获取
                (false, false, false)
            }
            _ => {
                debug!(update_type, "Peri mode: unhandled SessionUpdate type");
                (false, false, false)
            }
        }
    }
}

/// 辅助：截断字符串（从 agent.rs 移入）
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 2: 运行编译验证**

```bash
cargo build -p peri-tui
```
Expected: `handle_session_update_peri` 被解析但未被调用（no warning from unused）

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops/session_update.rs
git commit -m "feat: add handle_session_update_peri() for Peri mode SessionUpdate→AgentEvent bridge"
```

---

### Task 5: 更新 acp_bridge.rs 调用 Peri 模式桥接

**Files:**
- Modify: `peri-tui/src/app/agent_ops/acp_bridge.rs`

- [ ] **Step 1: 修改 SessionUpdate 分支**

```rust
// peri-tui/src/app/agent_ops/acp_bridge.rs

            AcpNotification::SessionUpdate { params, .. } => {
                if self.backend_mode == super::super::BackendMode::External {
                    return self.handle_session_update(&params);
                }
                // Peri mode: convert SessionUpdate → AgentEvent via existing pipeline
                return self.handle_session_update_peri(&params);
            }
```

- [ ] **Step 2: 运行编译验证**

```bash
cargo build -p peri-tui
```
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops/acp_bridge.rs
git commit -m "feat: Peri mode bridge consumes session/update via handle_session_update_peri()"
```

---

### Task 6: 简化 map_executor_event() — 删除类别①映射

**Files:**
- Modify: `peri-tui/src/app/agent.rs`

- [ ] **Step 1: 简化 map_executor_event()**

```rust
// peri-tui/src/app/agent.rs

use super::AgentEvent;
use peri_agent::agent::events::AgentEvent as ExecutorEvent;

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

use super::tool_display::{format_tool_args, format_tool_name, truncate};

/// 将 ExecutorEvent 映射为 TUI AgentEvent。
///
/// 仅处理 session/update 无法映射的事件（类别②③）。
/// 类别①事件（TextChunk, AiReasoning, ToolStart, ToolEnd, TodoUpdate）
/// 已通过 session/update → handle_session_update_peri() 处理，此处返回 None。
pub(crate) fn map_executor_event(event: ExecutorEvent, cwd: &str) -> Option<AgentEvent> {
    Some(match event {
        // ── 类别③：无 SessionUpdate 映射，仍通过 peri/agent_event ──
        ExecutorEvent::StateSnapshot(msgs) => AgentEvent::StateSnapshot(msgs),
        ExecutorEvent::SubagentStarted {
            agent_name,
            instance_id,
            is_background,
        } => AgentEvent::SubAgentStart {
            agent_id: agent_name.clone(),
            instance_id,
            task_preview: String::new(),
            is_background,
        },
        ExecutorEvent::SubagentStopped {
            agent_name,
            result,
            is_error,
            instance_id,
        } => AgentEvent::SubAgentEnd {
            agent_id: Some(agent_name),
            instance_id: Some(instance_id),
            result,
            is_error,
        },
        ExecutorEvent::CompactStarted => AgentEvent::CompactStarted,
        ExecutorEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
            messages,
        } => AgentEvent::CompactCompleted {
            summary,
            files,
            skills,
            micro_cleared,
            messages,
        },
        ExecutorEvent::CompactError { message } => AgentEvent::CompactError(message),
        ExecutorEvent::BackgroundTaskCompleted(result) => AgentEvent::BackgroundTaskCompleted {
            task_id: result.task_id,
            agent_name: result.agent_name,
            success: result.success,
            output: result.output,
            tool_calls_count: result.tool_calls_count,
            duration_ms: result.duration_ms,
            child_thread_id: result.child_thread_id,
        },
        ExecutorEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        } => AgentEvent::LspDiagnostics {
            errors,
            warnings,
            files_with_errors,
        },
        ExecutorEvent::AgentExecutionFailed { message } => {
            if message == "Interrupted by user" {
                AgentEvent::Interrupted
            } else {
                AgentEvent::Error(message)
            }
        }
        // ── 类别②：SessionUpdate 丢失信息的增强事件，TUI 仍通过

 peri/agent_event 获取完整数据 ──
        ExecutorEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        } => AgentEvent::ContextWarning {
            used_tokens,
            total_tokens,
            percentage,
        },
        ExecutorEvent::LlmCallEnd {
            usage: Some(usage),
            model,
            ..
        } => AgentEvent::TokenUsageUpdate { usage, model },
        ExecutorEvent::LlmCallEnd { usage: None, .. } => return None,
        ExecutorEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        } => AgentEvent::LlmRetrying {
            attempt,
            max_attempts,
            delay_ms,
            error,
        },

        // ── 类别①：已由 session/update → handle_session_update_peri() 处理 ──
        ExecutorEvent::TextChunk { .. }
        | ExecutorEvent::AiReasoning(_)
        | ExecutorEvent::ToolStart { .. }
        | ExecutorEvent::ToolEnd { .. }
        | ExecutorEvent::TodoUpdate(_)

        // ── 过滤 ──
        | ExecutorEvent::StepDone { .. }
        | ExecutorEvent::MessageAdded(_)
        | ExecutorEvent::LlmCallStart { .. }
        | ExecutorEvent::SessionEnded => return None,
    })
}
```

- [ ] **Step 2: 运行编译验证**

```bash
cargo build -p peri-tui
```
Expected: 编译通过。如果 tool_display 中的 `truncate` 函数与 session_update.rs 中新增的 `truncate` 冲突，删除 session_update.rs 中的 `truncate` 并使用 `tool_display::truncate`。

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent.rs
git commit -m "refactor: simplify map_executor_event() — remove category① mappings"
```

---

### Task 7: 运行全量测试并修复

**Files:** 所有改动的文件

- [ ] **Step 1: 运行 peri-acp 测试**

```bash
cargo test -p peri-acp --lib
cargo build -p peri-acp --all-targets
```

- [ ] **Step 2: 运行 peri-tui 测试**

```bash
cargo test -p peri-tui --lib
cargo build -p peri-tui --all-targets
```

- [ ] **Step 3: 运行全量编译**

```bash
cargo build --all-targets
```

- [ ] **Step 4: 检查 agent.rs 的测试文件是否需要更新**

`peri-tui/src/app/agent_test.rs` 可能包含了类别①事件的映射测试。检查并删除已不适用于新 map_executor_event 的测试用例。

- [ ] **Step 5: 检查 headless_test.rs**

`peri-tui/src/ui/headless_test.rs` 中大量使用 `AgentEvent` 变体（AssistantChunk, ToolStart, ToolEnd 等）。这些测试**不变**——AgentEvent 枚举本身不变，只是其创建方式从 `map_executor_event` 改为 `handle_session_update_peri`。

运行测试验证：

```bash
cargo test -p peri-tui --lib -- headless_test
```

- [ ] **Step 6: Commit**

```bash
git add -u
git commit -m "test: adjust tests for unified event mapping"
```

---

### Task 8: 删除 peri/* 自定义通知残留

**Files:**
- Modify: `peri-tui/src/acp_client/client.rs` (可选)
- Modify: `peri-tui/src/app/agent_ops/acp_bridge.rs`

`Peri` 变体现在不再被 `map_event()` 产生（旧的 `map_executor_to_peri_notifications()` 已被删除）。`AcpNotification::Peri` 可以保留（向后兼容），Bridge 的 `Peri` 分支不变。

- [ ] **Step 1: 确认不再有 peri/* 通知产生**

`map_event()` 不产生 peri/* 通知，`TransportEventSink` 也不调用旧函数。如果 TransportEventSink 中还有 peri/* 相关代码，清理之。

- [ ] **Step 2: 运行编译**

```bash
cargo build -p peri-tui
```

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: peri/* custom notifications removed (now handled by map_event)"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ `MappedEvent` + `map_event()` 统一映射函数 — Task 1
- ✅ 三类事件分区（①②③）— Task 1 实现
- ✅ `session/update` 通知 params 带 `_peri.sourceAgentId` — Task 2
- ✅ TransportEventSink 使用 map_event() — Task 2
- ✅ StdioEventSink 使用 map_event() — Task 3
- ✅ Peri 模式 session/update → AgentEvent 桥接 — Task 4
- ✅ acp_bridge.rs 调用桥接 — Task 5
- ✅ map_executor_event() 简化 — Task 6
- ✅ 删除旧映射函数 — Task 1 (以新替旧)
- ✅ 删除 peri/* 通知 — Task 8

**2. Placeholder scan:** 所有步骤有具体代码。

**3. Type consistency:**
- `MappedEvent` 定义在 `peri-acp/src/event/mapper.rs`，在 `event_sink.rs` + `mapper_test.rs` 中使用 ✅
- `handle_session_update_peri()` 返回签名 `(bool, bool, bool)` 与 `handle_agent_event` 一致 ✅
- `truncate()` 避免重复定义 — 使用 `tool_display::truncate` ✅

---

Plan complete and saved to `docs/superpowers/plans/2026-05-28-unified-event-mapping.md`. Two execution options:

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
