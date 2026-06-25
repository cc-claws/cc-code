# Fix: LLM 错误导致消息区域完全清空

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复两个相关 bug：(1) agent 执行失败（如 LLM 400）时 executor 不发送 Error 事件，导致用户看不到任何错误信息；(2) Compact 后 LLM 失败时 `round_start_vm_idx=0` 导致 view_messages 完全清空。

**Architecture:** 两层修复。**核心修复（Task 1-3）**：executor 在 agent 失败后通过 event channel 发送 `AgentExecutionFailed` 事件，TUI 收到后调用 `handle_error()` 显示错误 ToolBlock 并设置 `reconcile_already_done=true`，阻止后续 Done 事件触发空 RebuildAll。**防御修复（Task 4）**：在 `handle_done()` 中，当 `prefix_len == 0` 且 pipeline 无新 StateSnapshot 时，跳过 `request_rebuild()` 避免灾难性 drain。

**Tech Stack:** Rust 2021, tokio, peri-agent + peri-acp + peri-tui

---

## 改动总览

| Crate | 改动 | 风险 |
|-------|------|------|
| `peri-agent` | `AgentEvent` 新增 `AgentExecutionFailed { message: String }` 变体 | **低** — 纯新增变体，mapper.rs 有通配符兜底 |
| `peri-acp` | `executor.rs` 在 agent 失败后通过 `event_tx` 发送 `AgentExecutionFailed` | **低** — 在 `close_channel` 前发送 |
| `peri-tui` | `map_executor_event` 映射 `AgentExecutionFailed` → `AgentEvent::Error` | **低** — 一行映射 |
| `peri-tui` | `MessagePipeline` 添加 `has_snapshot_this_round()` getter | **低** — 纯新增 pub 方法 |
| `peri-tui` | `handle_done()` 中 `prefix_len==0` 且无 snapshot 时跳过 rebuild | **低** — 纯防御逻辑 |

**不涉及：** `CompactMiddleware`、`builder.rs`、`peri-middlewares`、`message_state.rs`。

---

### Task 1: peri-agent 新增 `AgentExecutionFailed` 事件变体

**Files:**
- Modify: `peri-agent/src/agent/events.rs:140-145`（`LspDiagnostics` 之后）

- [ ] **Step 1: 在 `AgentEvent` 枚举中新增变体**

在 `peri-agent/src/agent/events.rs` 的 `AgentEvent` 枚举中，在 `LspDiagnostics` 变体之后追加。使用结构体变体与 `CompactError { message: String }` 风格一致：

```rust
    /// LSP 诊断更新
    LspDiagnostics {
        errors: usize,
        warnings: usize,
        files_with_errors: usize,
    },
    /// Agent 执行失败（由 executor 在 agent.execute() 返回 Err 时发送）
    AgentExecutionFailed { message: String },
```

- [ ] **Step 2: 编译确认**

Run: `cargo build -p peri-agent`
Expected: 编译通过（新变体无人 match，仅产生 unused 警告）。

`peri-acp/src/event/mapper.rs` 中 `map_executor_to_updates()` 和 `map_executor_to_peri_notifications()` 均有 `_ => vec![]` 通配符兜底，不会因穷尽性检查报错。

- [ ] **Step 3: 新增 serde roundtrip 测试**

在 `peri-agent/src/agent/events_test.rs` 中追加：

```rust
#[test]
fn test_agent_execution_failed_serde_roundtrip() {
    let event = AgentEvent::AgentExecutionFailed {
        message: "LLM HTTP 错误 (400): invalid request".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let de: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(
        matches!(de, AgentEvent::AgentExecutionFailed { ref message } if message == "LLM HTTP 错误 (400): invalid request"),
        "AgentExecutionFailed serde roundtrip failed"
    );
}
```

Run: `cargo test -p peri-agent --lib -- test_agent_execution_failed_serde_roundtrip`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/events.rs peri-agent/src/agent/events_test.rs
git commit -m "feat(agent): 新增 AgentExecutionFailed 事件变体

用于 executor 在 agent 执行失败时通知前端（如 LLM 400 错误）。
结构体变体与 CompactError 风格一致。"
```

---

### Task 2: executor 在 agent 失败后发送 `AgentExecutionFailed` 事件

**Files:**
- Modify: `peri-acp/src/session/executor.rs:216-231`

- [ ] **Step 1: 在 executor.rs 中，agent 执行失败后发送事件**

修改 `peri-acp/src/session/executor.rs` 的 `execute_prompt` 函数。将 agent 执行结果处理部分从：

```rust
    let ok = result.is_ok();
    if let Err(e) = &result {
        error!(session_id = %session_id, error = %e, "Agent execution failed");
    }

    let stop_reason = if cancel.is_cancelled() {
```

改为：

```rust
    let ok = result.is_ok();
    if let Err(e) = &result {
        error!(session_id = %session_id, error = %e, "Agent execution failed");
        // 通过 event channel 通知前端 agent 执行失败
        if let Some(tx) = event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(ExecutorEvent::AgentExecutionFailed {
                message: e.to_string(),
            });
        }
    }

    let stop_reason = if cancel.is_cancelled() {
```

关键点：
- `event_tx` 类型是 `Arc<Mutex<Option<UnboundedSender<ExecutorEvent>>>>`，必须通过 `.lock().unwrap().as_ref()` 访问
- 发送在 `close_channel` **之前**，event pump 保证 FIFO 处理，`AgentExecutionFailed` 一定先于 `peri/agent_event_done` 到达 TUI

- [ ] **Step 2: 编译确认**

Run: `cargo build -p peri-acp`
Expected: 编译通过。

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/session/executor.rs
git commit -m "fix(acp): agent 执行失败时发送 AgentExecutionFailed 事件

之前 executor 只 log 错误不通知前端，导致 TUI 在 LLM 400 错误时
收不到 Error 事件，无法显示错误信息。"
```

---

### Task 3: TUI 映射 `AgentExecutionFailed` → `AgentEvent::Error`

**Files:**
- Modify: `peri-tui/src/app/agent.rs:168-169`（`SessionEnded` 和 `TodoUpdate` 之间）

- [ ] **Step 1: 在 `map_executor_event` 中添加映射**

在 `peri-tui/src/app/agent.rs` 的 `map_executor_event` 函数中，在 `SessionEnded` 之后、`TodoUpdate` 之前追加：

```rust
        ExecutorEvent::SessionEnded => return None,
        ExecutorEvent::AgentExecutionFailed { message } => AgentEvent::Error(message),
        ExecutorEvent::TodoUpdate(entries) => AgentEvent::TodoUpdate(
```

- [ ] **Step 2: 新增映射测试**

在 `peri-tui/src/app/agent_test.rs` 中追加：

```rust
#[test]
fn test_map_executor_event_execution_failed() {
    let event = ExecutorEvent::AgentExecutionFailed {
        message: "LLM HTTP 错误 (400)".to_string(),
    };
    let result = map_executor_event(event, "/tmp");
    assert!(
        result.is_some(),
        "AgentExecutionFailed should map to Some"
    );
    let mapped = result.unwrap();
    assert!(
        matches!(mapped, AgentEvent::Error(ref msg) if msg == "LLM HTTP 错误 (400)"),
        "AgentExecutionFailed should map to AgentEvent::Error"
    );
}
```

Run: `cargo test -p peri-tui --lib -- test_map_executor_event_execution_failed`
Expected: PASS

- [ ] **Step 3: 验证 `handle_error` 能正确处理（无需代码修改）**

`handle_error` 已有完整实现（`lifecycle.rs:216-291`）：
- 调用 `pipeline.done()` 清理 pipeline 状态
- 添加 error ToolBlock 到 view_messages
- 设置 `reconcile_already_done = true`（阻止后续 Done 的 `request_rebuild()` 清空视图）
- 调用 `cleanup_agent_state`

事件顺序保证：`AgentExecutionFailed` 先于 `AgentDone` 到达 → `handle_error()` 设置 `reconcile_already_done=true` → 后续 `handle_done()` 进入 else 分支，只做 `render_rebuild()` 不做 `request_rebuild()`。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/agent.rs peri-tui/src/app/agent_test.rs
git commit -m "fix(tui): 映射 AgentExecutionFailed 到 AgentEvent::Error

LLM 返回 400 等错误时，TUI 现在能收到 Error 事件并显示错误信息，
而不是静默清空消息区域。"
```

---

### Task 4: 防御 compact 后 round_start_vm_idx=0 导致视图清空

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:153-172`（添加 getter）
- Modify: `peri-tui/src/app/agent_ops/lifecycle.rs:63-68`（handle_done 防御逻辑）

这个 Task 是防御性措施：即使 Task 1-3 的 Error 事件修复了"看不到错误"的问题，仍需确保 compact 后 Done 的 RebuildAll 不会因 `prefix_len=0` + 空 tail 而清空视图。采用简化方案——在 `handle_done()` 中检测异常状态直接跳过 rebuild。

- [ ] **Step 1: 添加 `has_snapshot_this_round()` getter**

在 `peri-tui/src/app/message_pipeline/mod.rs` 的 `impl MessagePipeline` 块中（在 `in_subagent()` 方法之后），添加：

```rust
    /// 本轮是否已收到过 StateSnapshot
    pub fn has_snapshot_this_round(&self) -> bool {
        self.has_snapshot_this_round
    }
```

- [ ] **Step 2: 在 `handle_done` 中添加防御逻辑**

修改 `peri-tui/src/app/agent_ops/lifecycle.rs` 的 `handle_done` 函数。将：

```rust
        // 跳过已由 Interrupted/Error 处理器完成的 reconcile
        if !self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done
        {
            self.request_rebuild();
        }
```

改为：

```rust
        // 跳过已由 Interrupted/Error 处理器完成的 reconcile
        if !self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done
        {
            let prefix_len = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .round_start_vm_idx;
            let has_snapshot = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .has_snapshot_this_round();
            // 防御：compact 后 round_start_vm_idx 被设为 0，如果 compact 后
            // 没有新的 StateSnapshot 到达（agent 在 compact 后立即失败），
            // build_tail_vms 会返回空 tail，导致 prefix_len=0 的 drain 清空所有视图。
            // 此时跳过 rebuild，保留现有 view_messages 不变。
            if prefix_len == 0 && !has_snapshot {
                tracing::warn!(
                    session_id = %self.session_mgr.sessions[self.session_mgr.active].agent.session_start_time.is_some(),
                    "handle_done: prefix_len=0 with no snapshot, skipping rebuild to preserve view"
                );
            } else {
                self.request_rebuild();
            }
        }
```

- [ ] **Step 3: 编译确认**

Run: `cargo build -p peri-tui`
Expected: 编译通过。

- [ ] **Step 4: 全量测试**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: 所有测试通过。

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs peri-tui/src/app/agent_ops/lifecycle.rs
git commit -m "fix(tui): 防御 compact 后 LLM 失败导致视图完全清空

compact 后 round_start_vm_idx 被重置为 0，如果 agent 在 compact 后
立即失败（无新 StateSnapshot），Done 的 RebuildAll 会用 prefix_len=0
清空所有 view_messages。检测此异常状态跳过 rebuild。"
```

---

## 验证清单

- [ ] `cargo build --workspace` 编译通过
- [ ] `cargo test --workspace` 全量测试通过
- [ ] `cargo clippy --workspace 2>&1 | grep -i "AgentExecutionFailed"` 无新增警告
- [ ] 手动验证：使用会返回 400 的 DeepSeek 端点发送消息，确认 TUI 显示错误 ToolBlock 而非清空视图

---

## Review 修正记录

基于三个并行 review（代码正确性、架构一致性、测试覆盖），对原始计划做了以下修正：

1. **Task 1**: `AgentExecutionFailed(String)` → `AgentExecutionFailed { message: String }`（结构体变体，与 `CompactError` 风格一致）
2. **Task 1**: 补充 serde roundtrip 测试
3. **Task 3**: 补充映射测试
4. **Task 4**: 简化方案——去掉 `pre_compact_round_start` 保存/恢复机制（过度设计），改为在 `handle_done` 中直接检测 `prefix_len==0 && !has_snapshot` 跳过 rebuild
5. **Task 4**: 明确 `has_snapshot_this_round` 是私有字段，必须先添加 pub getter
6. **Task 4**: 确认 `pre_compact_round_start` 不需要，无需修改 `message_state.rs`

---

## 未涉及的改进（记录但不执行）

1. **AcpAgentConfig 参数分组**（19 字段）——独立重构
2. **CompactMiddleware 构造函数参数**（11 参数）——可后续引入 Builder
3. **executor.rs 零测试覆盖**——后续独立补充
