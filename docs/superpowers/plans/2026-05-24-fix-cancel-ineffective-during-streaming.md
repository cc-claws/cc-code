# Cancel（Ctrl+C）在 ACP 路径下失效修复

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Ctrl+C 在 LLM 流式输出和工具执行中无法真正取消底层请求的问题——UI 显示中断但 ACP server 端 agent 继续运行。

**Architecture:** `App::interrupt()` 在 ACP 路径下（`acp_client` 存在时）只发送 cancel 通知，不执行强制 UI 清理。UI 清理延迟到 ACP server 发回 `Interrupted`/`Done` 事件后由 `handle_interrupted()`/`handle_done()` 正常处理。Legacy 路径（测试用，无 acp_client）保留原有强制清理逻辑。

**Tech Stack:** Rust, tokio async, ACP transport (mpsc channel)

---

## 根因

`interrupt()` 有三条路径：

1. **ACP cancel**：`tokio::spawn(client.cancel())` — fire-and-forget，不等待
2. **直接 cancel_token**：TUI+ACP 路径下 `AgentComm.cancel_token` 始终为 None
3. **强制清理**：`else if loading` → 立即 set_loading(false)、截断 view_messages、清理 agent_rx

问题：路径 1 和路径 3 同时执行。路径 3 的强制清理导致：
- UI 立即显示"已中断"，但 ACP server 端 cancel 通知可能延迟到达
- 后续 ACP server 的 `Interrupted`/`Done` 事件到达后，`handle_interrupted()`/`handle_done()` 再次清理，造成双重清理和 UI 不一致
- agent 在 cancel 生效前继续运行（消耗 token、执行工具）

## 修复策略

在 ACP 路径下（`acp_client` 存在时），`interrupt()` 只发送 cancel 通知后立即 return，不执行强制清理。让 ACP server 端的 `Interrupted`/`Error`/`Done` 事件通过正常事件流完成 UI 清理。

**安全网**：如果 cancel 通知失败（session 已结束等），ACP server 端的 agent 自然完成后会发回 `Done`，TUI 的 `handle_done()` 正常清理。最坏情况下用户看到 agent 多运行了一小会儿然后正常结束。

---

### Task 1: 重构 `interrupt()` — ACP 路径提前返回

**Files:**
- Modify: `peri-tui/src/app/mod.rs:346-473`

**当前代码结构（伪代码）：**
```rust
pub fn interrupt(&mut self) {
    // 路径1: ACP cancel
    if let Some(ref acp_client) = self.acp_client {
        tokio::spawn(async move { client.cancel().await });
    }
    // 路径2: direct cancel_token
    if let Some(token) = &...cancel_token {
        token.cancel();
    } else if ...loading {
        // 路径3: 强制清理（130行代码）
    }
}
```

**目标代码结构：**
```rust
pub fn interrupt(&mut self) {
    // ACP 路径：只发 cancel，UI 清理由后续 Interrupted/Done 事件完成
    if let Some(ref acp_client) = self.acp_client {
        let client = acp_client.clone();
        tokio::spawn(async move {
            if let Err(e) = client.cancel().await {
                tracing::warn!(error = %e, "ACP cancel failed (session may have ended)");
            }
        });
        return; // ← 关键：不执行强制清理
    }

    // Legacy 路径（测试用）：直接 cancel_token + 强制清理
    if let Some(token) = &self.session_mgr.sessions[self.session_mgr.active]
        .agent
        .cancel_token
    {
        token.cancel();
    } else if self.session_mgr.sessions[self.session_mgr.active]
        .ui
        .loading
    {
        // ... 保留原有强制清理代码不变 ...
    }
}
```

- [ ] **Step 1: 修改 `interrupt()` 方法**

在 `peri-tui/src/app/mod.rs` 的 `interrupt()` 方法中，在 ACP cancel spawn 后添加 `return;`。

将现有的 ACP cancel block 从：
```rust
if let Some(ref acp_client) = self.acp_client {
    let client = acp_client.clone();
    tokio::spawn(async move {
        if let Err(e) = client.cancel().await {
            tracing::warn!(error = %e, "ACP cancel failed (session may have ended)");
        }
    });
}
```
改为：
```rust
if let Some(ref acp_client) = self.acp_client {
    let client = acp_client.clone();
    tokio::spawn(async move {
        if let Err(e) = client.cancel().await {
            tracing::warn!(error = %e, "ACP cancel failed (session may have ended)");
        }
    });
    // ACP 路径：cancel 已发送，UI 清理由后续 Interrupted/Done 事件完成。
    // 不执行强制清理——避免与 ACP server 端事件竞态导致双重清理。
    return;
}
```

后续的 `if let Some(token)` 和 `else if loading` 强制清理代码保持不变，仅在 legacy 路径（无 acp_client）下执行。

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译成功，无错误

- [ ] **Step 3: 运行现有测试**

Run: `cargo test -p peri-tui --lib`
Expected: 所有测试通过（无 regression）

---

### Task 2: 验证 handle_interrupted 在 ACP 路径下的正确性

**Files:**
- Read-only: `peri-tui/src/app/agent_ops/lifecycle.rs:142-229`
- Read-only: `peri-tui/src/app/agent_ops/acp_bridge.rs`

**验证内容**：确认 ACP server 端收到 cancel 后发出的 `Interrupted` 事件，通过 `acp_bridge.rs` → `handle_agent_event(Interrupted)` → `handle_interrupted()` 能正确完成 UI 清理。

关键检查点：
1. `handle_interrupted()` 是否正确调用 `cleanup_agent_state()` → `set_loading(false)`
2. `handle_interrupted()` 是否处理了 `agent_replied=false` 的文本恢复场景
3. `handle_interrupted()` 是否设置了 `reconcile_already_done=true` 防止后续 `Done` 重复清理

**已有代码分析**（lifecycle.rs）：
- 第 142-229 行 `handle_interrupted()` **已正确处理所有场景**：
  - 调用 `pipeline.handle_event(Interrupted)` finalize 状态
  - `!agent_replied` 时恢复用户文本到输入框（truncate view_messages + restore textarea）
  - `agent_replied` 时 request_rebuild + 添加中断通知
  - 设置 `reconcile_already_done = true`
  - 返回 `(true, false, false)` — updated=true, 不 break, 不 return

**结论**：`handle_interrupted()` 已完整覆盖 ACP 路径下的 UI 清理，无需额外修改。

- [ ] **Step 1: 验证 handle_interrupted 覆盖所有清理操作**

对比 `interrupt()` 强制清理路径和 `handle_interrupted()` 的操作：

| 操作 | interrupt() 强制清理 | handle_interrupted() |
|------|---------------------|---------------------|
| set_loading(false) | ✅ | ✅ (via cleanup_agent_state) |
| agent_rx = None | ✅ | ✅ (handle_done 中清理) |
| interaction_prompt = None | ✅ | ✅ (via cleanup_agent_state) |
| 截断 view_messages | ✅ | ✅ (via PipelineAction) |
| 恢复用户文本 | ✅ | ✅ (第 165-215 行) |
| reconcile_already_done | ❌ 未设置 | ✅ 设置 true |
| pipeline.done() | ✅ | ✅ |
| restore_completed | ✅ | ✅ |

`handle_interrupted()` 的覆盖比强制清理更完整（多了 `reconcile_already_done` 保护）。

---

### Task 3: 处理 cancel 通知失败的安全网

**Files:**
- Modify: `peri-tui/src/app/mod.rs`

**场景**：如果 ACP cancel 通知失败（session 已结束、transport 已关闭），TUI 会永远停在 loading=true 状态。

**解决方案**：在 `interrupt()` 中记录 cancel 时间戳。如果 loading 持续超过 5 秒且没有收到任何 ACP 事件，执行 fallback 强制清理。

- [ ] **Step 1: 添加 cancel 超时安全网**

在 `interrupt()` ACP 路径中添加超时 spawn：

```rust
if let Some(ref acp_client) = self.acp_client {
    let client = acp_client.clone();
    tokio::spawn(async move {
        if let Err(e) = client.cancel().await {
            tracing::warn!(error = %e, "ACP cancel failed (session may have ended)");
        }
    });

    // 安全网：记录 cancel 时间，5 秒后如果仍在 loading 则强制清理
    self.session_mgr.sessions[self.session_mgr.active]
        .agent
        .cancel_sent_at = Some(std::time::Instant::now());

    return;
}
```

在 `AgentComm` 中添加字段：
```rust
/// cancel 通知发送时间（用于超时 fallback）
pub cancel_sent_at: Option<std::time::Instant>,
```

在 `poll_agent()` 开头添加超时检查：
```rust
// Cancel 超时安全网：5 秒后仍未收到 Interrupted/Done，强制清理
if let Some(cancel_at) = self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .cancel_sent_at
{
    if cancel_at.elapsed() > std::time::Duration::from_secs(5)
        && self.session_mgr.sessions[self.session_mgr.active].ui.loading
    {
        tracing::warn!("cancel timeout: 5s elapsed without Interrupted/Done, force cleanup");
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_sent_at = None;
        self.cleanup_agent_state(None);
    }
}
```

在 `handle_interrupted()` 和 `handle_done()` 和 `handle_error()` 中清除 `cancel_sent_at`：
```rust
self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .cancel_sent_at = None;
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译成功

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-tui --lib`
Expected: 所有测试通过

---

### Task 4: 更新 issue 文档

**Files:**
- Modify: `spec/issues/2026-05-24-cancel-ineffective-during-streaming-and-tool-execution.md`

- [ ] **Step 1: 更新 issue 状态和修复记录**

在 issue 文档末尾追加：

```markdown
## 根因分析

`App::interrupt()` 在 ACP 路径下同时执行异步 cancel（fire-and-forget）和同步强制 UI 清理。强制清理立即生效使 UI 显示"已中断"，但 ACP server 端 cancel 通知可能延迟到达。后续 `Interrupted`/`Done` 事件到达后触发二次清理。

## 修复方案

`interrupt()` 在 ACP 路径下只发送 cancel 通知后 return，不执行强制清理。UI 清理延迟到 ACP server 发回的 `Interrupted`/`Done` 事件正常处理。5 秒超时安全网防止 cancel 通知丢失导致的永久 loading。
```

- [ ] **Step 2: Commit**

```bash
git add spec/issues/2026-05-24-cancel-ineffective-during-streaming-and-tool-execution.md
git commit -m "docs: update cancel issue with root cause and fix plan"
```

---

### Task 5: 最终验证

- [ ] **Step 1: cargo build 全量构建**

Run: `cargo build`
Expected: 成功

- [ ] **Step 2: cargo test 全量测试**

Run: `cargo test`
Expected: 全部通过

- [ ] **Step 3: lefthook pre-commit 检查**

Run: `lefthook run pre-commit`
Expected: fmt/check/clippy 全部通过
