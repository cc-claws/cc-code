# 统一事件映射系统 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除 ExecutorEvent → AgentEvent 的双重映射路径，将所有事件映射统一到 `map_executor_event` 单一入口。

**Architecture:** 当前 ACP server 将 ExecutorEvent 分为两条路径发送——有 `peri/*` 映射的事件走自定义通知，其余走 `notifications/agent_event`。TUI 端需要在 `agent_ops.rs` 中再次按 method string 将 `peri/*` 通知转回 AgentEvent。本次重构将所有 ExecutorEvent 统一通过 `notifications/agent_event` 发送，`peri/*` 通知仅用于 TUI 不直接处理的辅助事件（Compact、SessionEnded）。`map_executor_event` 成为唯一的映射入口。

**Tech Stack:** Rust, tokio async, serde_json, peri-agent/peri-acp/peri-tui workspace crates

---

## 当前问题

### 事件映射重复

同一个 ExecutorEvent 在 3 处被映射：

| 事件 | `map_executor_event` (agent.rs) | `map_executor_to_peri_notifications` (mapper.rs) | `handle_acp_notification` Peri 分支 (agent_ops.rs) |
|------|------|------|------|
| SubagentStarted | → SubagentLifecycle | → peri/subagent/start | → SubagentLifecycle |
| SubagentStopped | → SubAgentEnd | → peri/subagent/end | → SubAgentEnd |
| BackgroundTaskCompleted | → BackgroundTaskCompleted | → peri/background/completed | → BackgroundTaskCompleted |
| LspDiagnostics | → LspDiagnostics | → peri/lsp/diagnostics | → LspDiagnostics |

新增事件时必须同步更新 3 个位置，遗漏会导致事件丢失或重复。

### 路由逻辑

`acp_server.rs` 的 pump 任务（第 236-264 行）：

```rust
// 当前逻辑：有 peri 映射的事件 → 只走 peri/*，不走 agent_event
let peri_notifs = map_executor_to_peri_notifications(&exec_event);
if peri_notifs.is_empty() {
    // 没有 peri 映射 → 走 agent_event
    transport.send_notification("notifications/agent_event", ...);
}
// peri 映射的事件 → 走 peri/*
for (method, payload) in peri_notifs {
    transport.send_notification(method, payload);
}
```

关键问题：`peri_notifs.is_empty()` 时才发送 `agent_event`，所以有 peri 映射的事件**永远不会**通过 `agent_event` 路径到达 `map_executor_event`。这意味着 `map_executor_event` 中对 `SubagentStarted`/`SubagentStopped`/`BackgroundTaskCompleted`/`LspDiagnostics` 的映射代码在 ACP 模式下是**死代码**——这些事件永远走 peri/* 路径。

## 重构方案

1. **Server 端**：移除互斥逻辑——所有事件都通过 `agent_event` 发送；`peri/*` 通知仅保留 TUI 不处理的事件（Compact、SessionEnded）。
2. **`map_executor_to_peri_notifications`**：从 7 个事件缩减为 2 个（CompactStarted、CompactCompleted + SessionEnded）。
3. **TUI 端 `handle_acp_notification`**：移除 `Peri` 分支中的 `SubagentStarted`/`SubagentStopped`/`BackgroundTaskCompleted`/`LspDiagnostics` 处理（它们现在走 `agent_event` 路径）。
4. **`map_executor_event`**：无变更（已有正确映射），它从死代码变为活代码。

---

## File Structure

| 文件 | 操作 | 职责变更 |
|------|------|----------|
| `peri-tui/src/acp_server.rs:236-264` | 修改 | pump 任务：移除 peri/agent_event 互斥，所有事件都发 agent_event |
| `peri-acp/src/event/mapper.rs:141-204` | 修改 | `map_executor_to_peri_notifications`：移除 4 个事件映射，保留 3 个 |
| `peri-tui/src/app/agent_ops.rs:41-91` | 修改 | `handle_acp_notification` Peri 分支：移除 4 个事件处理，保留 Compact/Session 忽略 |
| `peri-tui/src/acp_client/client.rs:152-162` | 修改 | `run_pump`：简化 peri/* 路由（移除已不需要的变体） |

---

### Task 1: 修改 ACP Server pump 逻辑 — 移除互斥发送

**Files:**
- Modify: `peri-tui/src/acp_server.rs:236-264`

- [ ] **Step 1: 修改 pump 事件循环，所有事件都发送 agent_event**

当前代码（第 236-264 行）：
```rust
let peri_notifs = map_executor_to_peri_notifications(&exec_event);

if peri_notifs.is_empty() {
    let event_value = match serde_json::to_value(&exec_event) {
        // ... serialize and send agent_event
    };
}

for (method, mut payload) in peri_notifs {
    // ... send peri/* notifications
}
```

改为（移除 `if peri_notifs.is_empty()` 互斥，所有事件都发 agent_event）：
```rust
// 所有事件都通过 agent_event 发送到 TUI
let event_value = match serde_json::to_value(&exec_event) {
    Ok(v) => v,
    Err(e) => {
        error!(event_count = event_count, error = %e, "ACP pump: serialize failed");
        continue;
    }
};
let agent_event_params = json!({
    "session_id": sid,
    "event": event_value,
});
if let Err(e) = transport_clone
    .send_notification("notifications/agent_event", agent_event_params)
    .await
{
    error!(event_count = event_count, error = %e, "ACP pump: send agent_event failed");
    break;
}

// peri/* 通知仅用于 TUI 不直接处理的辅助事件（Compact、SessionEnded）
let peri_notifs = map_executor_to_peri_notifications(&exec_event);
for (method, mut payload) in peri_notifs {
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert("session_id".to_string(), json!(sid));
    }
    let _ = transport_clone.send_notification(method, payload).await;
}
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "refactor(acp): send all ExecutorEvents via agent_event path, remove mutual exclusion with peri/* notifications"
```

---

### Task 2: 精简 `map_executor_to_peri_notifications` — 移除已有 agent_event 映射的事件

**Files:**
- Modify: `peri-acp/src/event/mapper.rs:141-204`

- [ ] **Step 1: 从 `map_executor_to_peri_notifications` 中移除 4 个事件映射**

这 4 个事件现在通过 `agent_event` 路径发送，TUI 通过 `map_executor_event` 处理：
- `SubagentStarted` — 已在 `map_executor_event:158` 映射为 `SubagentLifecycle`
- `SubagentStopped` — 已在 `map_executor_event:162` 映射为 `SubAgentEnd`
- `BackgroundTaskCompleted` — 已在 `map_executor_event:140` 映射为 `BackgroundTaskCompleted`
- `LspDiagnostics` — 已在 `map_executor_event:148` 映射为 `LspDiagnostics`

保留 3 个 TUI 不直接处理的事件：
- `CompactStarted` — TUI 的 `map_executor_event:173` 返回 None
- `CompactCompleted` — TUI 的 `map_executor_event:174` 返回 None
- `SessionEnded` — TUI 的 `map_executor_event:172` 返回 None

修改后函数：
```rust
pub fn map_executor_to_peri_notifications(
    event: &ExecutorEvent,
) -> Vec<(&'static str, serde_json::Value)> {
    match event {
        ExecutorEvent::CompactStarted => {
            vec![("notifications/peri/compact/start", json!({}))]
        }
        ExecutorEvent::CompactCompleted => {
            vec![("notifications/peri/compact/end", json!({}))]
        }
        ExecutorEvent::SessionEnded => {
            vec![("notifications/peri/session/ended", json!({}))]
        }
        _ => vec![],
    }
}
```

同步更新函数文档注释：
```rust
/// 将 ExecutorEvent 映射为 `peri/*` 自定义通知列表。
///
/// 仅包含 TUI 通过 `map_executor_event` 过滤掉（返回 None）的事件：
/// - CompactStarted → `notifications/peri/compact/start`
/// - CompactCompleted → `notifications/peri/compact/end`
/// - SessionEnded → `notifications/peri/session/ended`
///
/// 其余事件通过 `notifications/agent_event` 由 `map_executor_event` 统一处理。
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-acp 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/event/mapper.rs
git commit -m "refactor(acp): remove redundant peri/* notifications for events handled by map_executor_event"
```

---

### Task 3: 精简 TUI `handle_acp_notification` Peri 分支

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:41-91`

- [ ] **Step 1: 移除 Peri 分支中已由 agent_event 处理的 4 个事件**

现在 `SubagentStarted`/`SubagentStopped`/`BackgroundTaskCompleted`/`LspDiagnostics` 都通过 `agent_event` → `map_executor_event` 处理，Peri 分支只需忽略 Compact/Session 事件。

修改后的 Peri 分支（替换第 41-91 行）：
```rust
            AcpNotification::Peri { method, params, .. } => {
                // peri/* 通知仅用于 Compact/SessionEnded 等辅助事件。
                // SubAgent、Background、LSP 事件现在统一走 agent_event 路径。
                let _ = (method, params); // suppress unused warning
                (false, false, false)
            }
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误（可能有 unused warning 关于 `_` 绑定，但不应有错误）

- [ ] **Step 3: 如果 Step 2 中有 unused variable warning，调整代码**

如果 `params` 产生 unused warning，改为：
```rust
            AcpNotification::Peri { method, params, .. } => {
                tracing::debug!(%method, "ACP→TUI: peri/* notification ignored (handled via agent_event)");
                let _ = params;
                (false, false, false)
            }
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent_ops.rs
git commit -m "refactor(tui): remove redundant Peri notification handling, events now via agent_event"
```

---

### Task 4: 清理 AcpNotification Peri 变体（可选简化）

**Files:**
- Modify: `peri-tui/src/acp_client/client.rs:152-162`
- Modify: `peri-tui/src/acp_client/client.rs:32-37`

- [ ] **Step 1: 确认 Peri 变体是否仍需保留**

当前 `Peri` 变体仍用于 Compact/SessionEnded 通知（被 TUI 忽略但需接收）。检查是否可以简化。

如果 TUI 对 Compact/SessionEnded 通知完全不感兴趣，可以在 `run_pump` 中直接忽略这些通知而不发送 `AcpNotification::Peri`：

```rust
// client.rs run_pump 中 peri/* 处理（约第 152-162 行）
} else if method.starts_with("notifications/peri/") {
    // Compact/Session 通知仅用于日志，不需要发送到 TUI 事件循环
    tracing::debug!(%method, "ACP pump: peri/* notification received (no TUI action)");
}
```

如果这样做，`AcpNotification::Peri` 变体可以标记为 dead code 或移除。

但要注意：**外部 IDE client** 可能仍需要接收这些 peri/* 通知。如果 StdioTransport 用于外部 IDE 连接，移除 Peri 变体会破坏兼容性。

**建议**：保留 `Peri` 变体但简化处理。不做此步也可以——这是锦上添花，不影响核心重构。

- [ ] **Step 2: 如果决定保留 Peri 变体，确认不需要额外修改**

如果保留，当前 Task 3 的修改已经足够——Peri 分支返回 `(false, false, false)` 不影响任何功能。

- [ ] **Step 3: Commit（如有修改）**

```bash
git add peri-tui/src/acp_client/client.rs
git commit -m "refactor(tui): simplify peri/* notification handling in AcpClient pump"
```

---

### Task 5: 全量构建和测试

**Files:** 无修改

- [ ] **Step 1: 全量构建**

Run: `cargo build 2>&1 | tail -20`
Expected: 成功

- [ ] **Step 2: 运行相关 crate 测试**

Run: `cargo test -p peri-acp 2>&1 | tail -20`
Expected: 所有测试通过

Run: `cargo test -p peri-tui 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 3: 运行 pre-commit hooks**

Run: `lefthook run pre-commit 2>&1 | tail -20`
Expected: 全部通过

- [ ] **Step 4: 手动集成测试**

启动 TUI（`cargo run -p peri-tui`），发送一个消息触发 SubAgent 工具调用，验证：
- SubAgent 启动/停止事件正常显示
- 后台任务完成通知正常显示
- LSP 诊断信息正常显示
- 工具调用审批弹窗正常工作

- [ ] **Step 5: 最终 Commit（如有遗漏的修复）**

```bash
git add -A
git commit -m "fix: follow-up fixes from event mapping unification"
```

---

## Self-Review

### Spec Coverage
- ✅ 双重映射消除：Task 1 移除互斥，Task 2 移除 peri 映射，Task 3 移除 TUI Peri 分支
- ✅ `map_executor_event` 保持不变：已是正确的单一映射入口
- ✅ Compact/SessionEnded 保留为 peri/* 通知：TUI 不处理这些事件
- ✅ 向后兼容：`SessionUpdate` 路径不受影响（始终发送）

### Placeholder Scan
- 无 TBD/TODO/placeholder
- 所有步骤包含完整代码

### Type Consistency
- `ExecutorEvent` 枚举变体名称在所有文件中一致
- `AcpNotification::Peri` 变体结构不变（`{ session_id, method, params }`）
- `map_executor_event` 返回 `Option<AgentEvent>`，签名不变
