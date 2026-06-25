# Immediate 命令缺失 push_done 修复计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `/compact` 和 `/clear` 等 Immediate 命令执行后 TUI 永久卡在 loading 状态的问题，确保 `push_done` 和 `StateSnapshot` 事件正确发送。

**Architecture:** 在 `executor.rs` 的 Immediate 命令拦截路径中，执行完命令后调用 `sink.push_done()` 发送 `peri/agent_event_done` 通知。`/clear` 命令额外发送一个空的 `StateSnapshot` 事件通知 TUI 清空本地视图状态。

**Tech Stack:** Rust, async/await (tokio), ACP 协议通知机制

---

### Task 1: 修复 executor Immediate 命令路径缺失 push_done

**Files:**
- Modify: `peri-acp/src/session/executor.rs:161-183`

**问题：** Immediate 命令（`/compact`、`/clear`）在 `executor.rs:176` 处直接 `return PromptResult`，跳过了 agent event pump（236-341 行），因此 `sink.push_done(&sid)`（333 行）永远不会被调用。TUI 永远收不到 `AgentDone` 事件，界面卡在 loading。

- [ ] **Step 1: 修改 executor.rs Immediate 命令拦截路径，执行后调用 push_done**

在 `executor.rs` 的 Immediate 命令分支中，执行完命令后、return 之前，调用 `event_sink.push_done(&session_id).await`。

将当前的 Immediate 命令拦截代码（161-183 行）：

```rust
    // Command interception — check if content is a slash command before building agent.
    if let Some(text) = content.text_content().strip_prefix('/') {
        if !text.is_empty() {
            let command_registry = crate::session::command::default_command_registry();
            if let Some((cmd, args)) = command_registry.find(&content.text_content()) {
                if cmd.kind() == crate::session::command::CommandKind::Immediate {
                    let ctx = crate::session::command::CommandContext {
                        session_id: session_id.clone(),
                        history: history.clone(),
                        cwd: cwd.to_string(),
                        peri_config: Arc::new(peri_config.as_ref().clone()),
                        compact_model: compact_model.clone(),
                        event_sink: event_sink.clone(),
                        args: args.to_string(),
                    };
                    let result = cmd.execute(ctx).await;
                    return PromptResult {
                        messages: result.messages,
                        ok: true,
                        stop_reason: result.stop_reason,
                        recall_items: Vec::new(),
                    };
                }
                // Passthrough/Transform → fall through to normal agent flow
            }
        }
    }
```

替换为：

```rust
    // Command interception — check if content is a slash command before building agent.
    if let Some(text) = content.text_content().strip_prefix('/') {
        if !text.is_empty() {
            let command_registry = crate::session::command::default_command_registry();
            if let Some((cmd, args)) = command_registry.find(&content.text_content()) {
                if cmd.kind() == crate::session::command::CommandKind::Immediate {
                    let ctx = crate::session::command::CommandContext {
                        session_id: session_id.clone(),
                        history: history.clone(),
                        cwd: cwd.to_string(),
                        peri_config: Arc::new(peri_config.as_ref().clone()),
                        compact_model: compact_model.clone(),
                        event_sink: event_sink.clone(),
                        args: args.to_string(),
                    };
                    let result = cmd.execute(ctx).await;
                    // Immediate 命令跳过 agent event pump，必须手动发送 push_done
                    // 通知 TUI agent 执行完成，否则界面永久卡在 loading 状态。
                    event_sink.push_done(&session_id).await;
                    return PromptResult {
                        messages: result.messages,
                        ok: true,
                        stop_reason: result.stop_reason,
                        recall_items: Vec::new(),
                    };
                }
                // Passthrough/Transform → fall through to normal agent flow
            }
        }
    }
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-acp`
Expected: 编译成功，无错误

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/session/executor.rs
git commit -m "fix(acp): send push_done after Immediate commands to prevent TUI freeze

Immediate commands (/compact, /clear) bypass the agent event pump,
so sink.push_done() was never called. TUI never received AgentDone
and stayed in loading state forever.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: 修复 /clear 命令缺失 StateSnapshot 事件

**Files:**
- Modify: `peri-acp/src/session/command/clear.rs:31-36`

**问题：** `/clear` 命令只返回空的 `messages: Vec::new()`，不发送 `StateSnapshot` 事件。即使 `push_done` 修复后 TUI 的 `handle_done` 能触发，pipeline 的 reconcile 使用旧的 `origin_messages`，`view_messages` 不会被清空——旧消息残留。

`CompactCompleted` 事件携带 `messages` 字段，TUI 的 `handle_compact_completed` 用它更新 `agent_state_messages` 和 pipeline。`/clear` 需要类似机制通知 TUI 清空本地状态。

解决方案：`ClearCommand::execute` 在返回空 messages 之前，通过 event_sink 发送一个空的 `StateSnapshot` 事件。TUI 的 `handle_done` 在收到 `AgentDone` 后会触发 `request_rebuild()`，此时如果 `origin_messages` 已更新为空，pipeline 会正确清空 view_messages。

但更可靠的方式是：`/clear` 命令返回空 messages → `execute_prompt` 的 Immediate 路径返回 `PromptResult { messages: vec![], ok: true, ... }` → TUI 侧 `prompt.rs` 的 `execute_prompt` 把 `state.history` 设为空 → 同时 Immediate 命令发送的 `push_done` 让 TUI 调用 `handle_done()`。

关键在于 TUI 侧 `handle_done` 会调用 `pipeline.handle_event(Done)` + `request_rebuild()`，而 `prompt.rs` 在 `push_done` 之后更新了 `state.history = result.messages`（空）。但 `handle_done` 的 reconcile 依赖 `origin_messages`（TUI 本地），不是 ACP server 端的 `state.history`。

最可靠的方案是让 `/clear` 发送一个专门的 `ClearCompleted` ExecutorEvent，TUI 收到后主动清空本地状态。但新增事件变体需要改动 `peri-agent`，范围较大。

**更简洁的方案：** 让 `ClearCommand` 在 execute 中通过 event_sink 发送一个 `CompactCompleted { messages: vec![], summary: "对话已清空", ... }` 事件。TUI 已有 `handle_compact_completed` 处理器，会正确执行 pipeline 三步清理（clear → restore_completed(空) → RebuildAll { prefix_len: 0 }）。

- [ ] **Step 1: 修改 ClearCommand 发送 CompactCompleted 事件**

在 `clear.rs` 中，`execute` 方法通过 event_sink 发送 `CompactCompleted` 事件（messages 为空），复用 TUI 已有的 compact 清理路径。

将当前的 `clear.rs`:

```rust
//! `/clear` 命令 — 清空对话历史。

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 清空历史命令。
pub struct ClearCommand;

impl ClearCommand {
    pub const NAME: &'static str = "clear";
}

#[async_trait::async_trait]
impl AgentCommand for ClearCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["cls", "reset"]
    }

    fn description(&self) -> &str {
        "清空当前会话的对话历史"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult {
            messages: Vec::new(),
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}
```

替换为：

```rust
//! `/clear` 命令 — 清空对话历史。

use peri_agent::agent::events::AgentEvent as ExecutorEvent;

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 清空历史命令。
pub struct ClearCommand;

impl ClearCommand {
    pub const NAME: &'static str = "clear";
}

#[async_trait::async_trait]
impl AgentCommand for ClearCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["cls", "reset"]
    }

    fn description(&self) -> &str {
        "清空当前会话的对话历史"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        // 发送 CompactCompleted（空 messages）复用 TUI 的 compact 清理路径：
        // pipeline.clear() → restore_completed(vec![]) → RebuildAll { prefix_len: 0 }
        // 这确保 TUI 的 view_messages 和 origin_messages 被正确清空。
        ctx.event_sink
            .push_event(
                &ctx.session_id,
                &ExecutorEvent::CompactCompleted {
                    summary: "对话已清空".to_string(),
                    files: vec![],
                    skills: vec![],
                    micro_cleared: 0,
                    messages: vec![],
                },
                0,
            )
            .await;

        CommandResult {
            messages: Vec::new(),
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p peri-acp`
Expected: 编译成功，无错误

- [ ] **Step 3: Commit**

```bash
git add peri-acp/src/session/command/clear.rs
git commit -m "fix(acp): /clear sends CompactCompleted event to flush TUI view state

/clear was returning empty messages but not notifying TUI to clear
view_messages and origin_messages. Now sends CompactCompleted with
empty messages to reuse the existing compact cleanup pipeline:
clear → restore_completed([]) → RebuildAll{prefix_len:0}.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: 验证修复效果

**Files:**
- Read-only verification

- [ ] **Step 1: 全量构建确认无编译错误**

Run: `cargo build`
Expected: 编译成功

- [ ] **Step 2: 运行 peri-acp 测试**

Run: `cargo test -p peri-acp`
Expected: 所有测试通过

- [ ] **Step 3: 运行 peri-tui 测试**

Run: `cargo test -p peri-tui`
Expected: 所有测试通过

- [ ] **Step 4: Clippy 检查**

Run: `cargo clippy -p peri-acp -p peri-tui -- -D warnings`
Expected: 无新的 warning

## Self-Review

### 1. Spec 覆盖检查

| Issue 要求 | 对应 Task | 状态 |
|------------|----------|------|
| Immediate 命令跳过 `push_done` 导致 TUI 卡在 loading | Task 1 | ✓ |
| `/clear` 不发送 StateSnapshot 导致 view_messages 残留 | Task 2 | ✓ |
| `/compact` 手动调用 `push_event` 但不调用 `push_done` | Task 1 | ✓（compact 事件已有 push_event，加 push_done 后完整） |

### 2. Placeholder 扫描

无 TBD/TODO/placeholders。所有步骤包含完整代码。

### 3. 类型一致性检查

- `event_sink.push_done(&session_id).await` — 签名匹配 `EventSink::push_done(&self, session_id: &str)` ✓
- `event_sink.push_event(&session_id, &ExecutorEvent::CompactCompleted{..}, 0)` — 签名匹配 `EventSink::push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32)` ✓
- `CommandContext` 中 `event_sink: Arc<dyn EventSink>` — 与 `push_event`/`push_done` 的 `&self` 接收者兼容 ✓
