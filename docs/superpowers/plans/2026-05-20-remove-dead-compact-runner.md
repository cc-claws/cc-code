# Remove Dead `compact_runner` + Cleanup `CompactMiddleware`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除已无调用方的 `compact_runner` 死代码，清理 `CompactMiddleware` 中因历史遗留而存在的冗余代码，并移除 `PromptResult.compacted` 废弃字段。

**Architecture:** `CompactMiddleware`（`peri-middlewares`）是 compact 唯一的执行入口，通过 `before_model` 钩子在 ReAct 循环内原地压缩上下文。旧的 `compact_runner`（`peri-acp`）是为手动 `/compact` + auto-compact 双路径设计的，现在手动路径已移除（`/compact` 命令直接走 prompt 路径），auto-compact 已完全由 `CompactMiddleware` 接管，`compact_runner` 成为无调用方的死代码。

**Tech Stack:** Rust 2021, tokio, async-trait, peri-agent + peri-middlewares + peri-acp

---

## 改动总览

| Crate | 改动 | 风险 |
|-------|------|------|
| `peri-acp` | 删除 `compact_runner.rs`，移除模块声明，删除 `PromptResult.compacted` | **低** — 纯删除死代码 |
| `peri-acp` | 清理 `executor.rs` 中过时的 compact 注释和 `compacted` 字段 | **低** |

**不涉及：** `CompactMiddleware`（已验证功能正确）、`builder.rs`（已正确注入 CompactMiddleware）、TUI 层。

---

### Task 1: 删除 `compact_runner.rs` 文件

**Files:**
- Delete: `peri-acp/src/session/compact_runner.rs`

- [ ] **Step 1: 确认文件无外部引用**

Run: `grep -rn "compact_runner" --include="*.rs" peri-acp/ peri-tui/ peri-middlewares/ peri-agent/`
Expected: 仅出现 `peri-acp/src/session/mod.rs:6:pub mod compact_runner;`（模块声明）和 `compact_runner.rs` 内部引用。无外部调用方。

- [ ] **Step 2: 删除 `compact_runner.rs`**

```bash
rm peri-acp/src/session/compact_runner.rs
```

- [ ] **Step 3: 编译确认——预期失败**

Run: `cargo build -p peri-acp 2>&1 | head -20`
Expected: 编译错误 `could not find module compact_runner`，指向 `peri-acp/src/session/mod.rs:6`。

- [ ] **Step 4: Commit（中间状态，不编译通过——稍后修复）**

这一步暂不 commit，等 Task 2 完成后一起提交。

---

### Task 2: 移除 `compact_runner` 模块声明

**Files:**
- Modify: `peri-acp/src/session/mod.rs:6`

- [ ] **Step 1: 删除模块声明**

打开 `peri-acp/src/session/mod.rs`，删除第 6 行：

```rust
// 删除此行:
pub mod compact_runner;
```

最终文件顶部应为：

```rust
//! Session lifecycle management.
//!
//! Manages ACP session creation, loading, resumption, and closure.
//! Each session owns a ThreadStore entry, an Agent instance, and associated state.

pub mod event_sink;
pub mod executor;
pub mod state_builders;
```

- [ ] **Step 2: 编译确认**

Run: `cargo build -p peri-acp`
Expected: 编译通过（`compact_runner` 无外部调用方，删除不影响其他代码）。

- [ ] **Step 3: 全量测试**

Run: `cargo test -p peri-acp --lib`
Expected: 所有测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/compact_runner.rs peri-acp/src/session/mod.rs
git commit -m "refactor(acp): 删除无调用方的 compact_runner 死代码

compact_runner 原为手动 /compact + auto-compact 双路径设计。
手动路径已移除，auto-compact 已由 CompactMiddleware 在 before_model
钩子中统一处理。compact_runner 零外部调用方，安全删除。"
```

---

### Task 3: 清理 `PromptResult.compacted` 废弃字段

**Files:**
- Modify: `peri-acp/src/session/executor.rs:44-45`
- Modify: `peri-acp/src/session/executor.rs:239`

- [ ] **Step 1: 确认 `compacted` 字段无读取方**

Run: `grep -rn '\.compacted' --include="*.rs" .`
Expected: 仅出现 `peri-acp/src/session/executor.rs` 中的定义和赋值，无外部读取。

- [ ] **Step 2: 从 `PromptResult` 结构体删除 `compacted` 字段**

在 `peri-acp/src/session/executor.rs` 中，修改 `PromptResult` 结构体（约第 38-48 行）：

```rust
/// Result of prompt execution.
pub struct PromptResult {
    /// Updated message history after execution.
    pub messages: Vec<BaseMessage>,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Why the prompt execution stopped.
    pub stop_reason: PromptStopReason,
}
```

删除 `compacted` 字段及其注释（第 44-45 行）。

- [ ] **Step 3: 删除 `PromptResult` 构造处的 `compacted` 赋值**

在同文件末尾的 `PromptResult { ... }` 构造处（约第 236-241 行），删除 `compacted` 行：

```rust
PromptResult {
    messages: agent_state.into_messages(),
    ok,
    stop_reason,
}
```

- [ ] **Step 4: 清理过时注释**

在同文件中，找到以下过时注释并删除或更新：

```
/// 5. Check token thresholds → compact if needed (micro or full + resubmit)
```

替换为：

```
/// 5. Auto-compact handled by CompactMiddleware (before_model hook)
```

同时删除：

```
/// Whether a compact occurred during execution.
```

（已在 Step 2 中随字段一起删除。）

- [ ] **Step 5: 编译确认**

Run: `cargo build -p peri-acp`
Expected: 编译通过。

- [ ] **Step 6: 全量测试**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: 所有测试通过。

- [ ] **Step 7: Commit**

```bash
git add peri-acp/src/session/executor.rs
git commit -m "refactor(acp): 移除 PromptResult.compacted 废弃字段

compacted 字段始终为 false，无任何读取方。compact 状态完全由
CompactMiddleware 通过事件通道通知，无需通过返回值传递。"
```

---

## 验证清单

- [ ] `cargo build --workspace` 编译通过
- [ ] `cargo test --workspace` 全量测试通过
- [ ] `grep -rn "compact_runner" --include="*.rs" .` 仅出现在 spec/docs 中，不出现于源码
- [ ] `grep -rn '\.compacted' --include="*.rs" .` 无结果
- [ ] `cargo clippy --workspace 2>&1 | grep -i compact` 无 compact 相关警告

---

## 未涉及的改进（记录但不执行）

1. **AcpAgentConfig 参数分组**（19 字段）——独立重构，不在本次范围
2. **CompactMiddleware 构造函数参数**（11 参数）——可后续引入 Builder，不在本次范围
3. **executor.rs 中 compact_config 双重解析**（executor 和 builder 各解析一次）——可后续统一，不在本次范围
