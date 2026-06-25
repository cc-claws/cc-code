# Interrupt Undo Last User Message Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When user presses Ctrl+C to interrupt an agent execution, automatically remove the last user message from history and restore it to the textarea for editing, regardless of whether the agent has started responding.

**Architecture:** Two-layer fix: (1) ACP server rolls back `state.history` on cancel so the next prompt doesn't include the cancelled round; (2) TUI always removes user message + partial agent response from `view_messages` and restores text to textarea, removing the `agent_replied` condition that previously limited this behavior to the "no reply yet" case.

**Tech Stack:** Rust, tokio async, ACP protocol (MpscTransport)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `peri-tui/src/acp_server/prompt.rs` | Modify | Roll back `state.history` to pre-submit length on cancel |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | Modify | Unify interrupt handler: always rollback + restore textarea |

---

### Task 1: ACP Server — Roll back history on cancel

**Files:**
- Modify: `peri-tui/src/acp_server/prompt.rs:151-169`

**Context:** After `execute_prompt` returns, `state.history = result.messages` always runs, even on cancel. This means the cancelled round's user message + partial AI response stay in the ACP server's conversation state. The next `session/prompt` will include these stale messages. On cancel, we must truncate back to the pre-submit history length.

- [ ] **Step 1: Modify `prompt.rs` — truncate history on cancel**

In `peri-tui/src/acp_server/prompt.rs`, find this block (around line 151-169):

```rust
    // Persist new messages to ThreadStore and update in-memory state.
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            if result.ok {
                info!(session_id = %session_id, messages = result.messages.len(), "Agent execution completed");
                // Persist only the newly added messages.
                if history_len < result.messages.len() {
                    let new_msgs = &result.messages[history_len..];
                    if let Err(e) = thread_store.append_messages(&thread_id, new_msgs).await {
                        tracing::warn!(error = %e, "Failed to persist messages to ThreadStore");
                    }
                }
            }
            state.history = result.messages;
            state.recall_items = result.recall_items;
            state.cancel_token = None;
        }
    }
```

Replace the `state.history = result.messages;` line (line 165) with cancel-aware logic:

```rust
    // Persist new messages to ThreadStore and update in-memory state.
    {
        let mut sessions = sessions.lock().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            if result.ok {
                info!(session_id = %session_id, messages = result.messages.len(), "Agent execution completed");
                // Persist only the newly added messages.
                if history_len < result.messages.len() {
                    let new_msgs = &result.messages[history_len..];
                    if let Err(e) = thread_store.append_messages(&thread_id, new_msgs).await {
                        tracing::warn!(error = %e, "Failed to persist messages to ThreadStore");
                    }
                }
                state.history = result.messages;
            } else {
                // Execution failed or was cancelled — roll back to pre-submit state.
                // This prevents the cancelled round's user message + partial AI response
                // from appearing in the next prompt's context.
                state.history.truncate(history_len);
                info!(session_id = %session_id, history_len, "Agent execution failed/cancelled, rolled back history");
            }
            state.recall_items = result.recall_items;
            state.cancel_token = None;
        }
    }
```

Key change: `state.history = result.messages` now only runs on success. On failure/cancel, `state.history.truncate(history_len)` removes everything added during this round.

- [ ] **Step 2: Build and verify**

Run: `cargo build -p peri-tui`
Expected: Clean build, no errors.

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server/prompt.rs
git commit -m "fix(acp): roll back state.history on agent execution failure/cancel

Previously, state.history was always replaced with result.messages
after execute_prompt, even on cancel. This caused the cancelled
round's user message to persist in conversation context.
Now truncate back to pre-submit history length on failure/cancel."
```

---

### Task 2: TUI — Always remove user message + restore textarea on interrupt

**Files:**
- Modify: `peri-tui/src/app/agent_ops/lifecycle.rs:145-236`

**Context:** Currently `handle_interrupted()` has two branches: `!agent_replied` (full rollback + restore textarea) and `agent_replied` (just show notification, leave message in history). The user wants rollback + restore to happen regardless of `agent_replied`. The change unifies both branches.

- [ ] **Step 1: Rewrite `handle_interrupted()` in `lifecycle.rs`**

In `peri-tui/src/app/agent_ops/lifecycle.rs`, replace the entire `handle_interrupted()` method (lines 145-236) with:

```rust
    pub(super) fn handle_interrupted(&mut self) -> (bool, bool, bool) {
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .cancel_sent_at = None;
        // Child agent interrupted during tool execution — ignore
        if self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .in_subagent()
        {
            return (false, false, false);
        }
        // Pipeline：finalize 当前状态
        let actions = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .pipeline
            .handle_event(AgentEvent::Interrupted);
        for action in actions {
            self.apply_pipeline_action(action);
        }

        // 始终尝试恢复用户文本到输入框（无论 agent 是否已回复）
        if let Some(text) = self.session_mgr.sessions[self.session_mgr.active]
            .messages
            .last_submitted_text
            .take()
        {
            let round_start = self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .round_start_vm_idx;
            // 截断 view_messages（移除本轮 Human 消息 + Agent 响应）
            self.apply_pipeline_action(PipelineAction::RebuildAll {
                prefix_len: round_start,
                tail_vms: vec![],
            });
            // 截断 agent_state_messages（回滚 StateSnapshot 扩展的内容）
            let pre_len = self.session_mgr.sessions[self.session_mgr.active]
                .metadata
                .pre_submit_state_len;
            self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_state_messages
                .truncate(pre_len);
            // 恢复文本到输入框
            let mut ta = crate::app::build_textarea(false);
            ta.insert_str(text.clone());
            self.session_mgr.sessions[self.session_mgr.active]
                .ui
                .textarea = ta;
            // 清除 pending 缓冲
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pending_messages
                .clear();
            // 清除 sticky header
            self.session_mgr.sessions[self.session_mgr.active]
                .metadata
                .last_human_message = None;
            // 清除 pipeline 状态
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .done();
            let restored = self.session_mgr.sessions[self.session_mgr.active]
                .agent
                .agent_state_messages
                .clone();
            self.session_mgr.sessions[self.session_mgr.active]
                .messages
                .pipeline
                .restore_completed(restored);
            let vm = MessageViewModel::system(self.services.lc.tr("app-interrupted-resumed"));
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        } else {
            let vm = MessageViewModel::system(self.services.lc.tr("app-interrupt-done"));
            self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        }
        // 标记 reconcile 已完成，防止后续 Done 事件重复 RebuildAll 覆盖通知消息
        self.session_mgr.sessions[self.session_mgr.active]
            .agent
            .reconcile_already_done = true;
        (true, false, false)
    }
```

Key changes from original:
1. Removed `agent_replied` condition — rollback always happens when `last_submitted_text` is present
2. `reconcile_already_done = true` is now always set (previously only in `agent_replied` branch) — prevents subsequent `Done` event from overwriting the UI state
3. Unified to use `app-interrupted-resumed` message when text is restored

- [ ] **Step 2: Build and verify**

Run: `cargo build -p peri-tui`
Expected: Clean build, no errors.

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/agent_ops/lifecycle.rs
git commit -m "feat(tui): always restore user text to textarea on Ctrl+C interrupt

Previously, interrupt rollback (remove message from history +
restore to textarea) only happened when the agent hadn't replied
yet (agent_replied == false). Now it always happens regardless
of agent state, allowing users to correct typos even after the
agent has started responding."
```

---

## Self-Review

**1. Spec coverage:**
- "Ctrl+C 中断后，自动将上一条消息从历史中移除" → Task 1 (ACP server) + Task 2 (TUI view)
- "移除的消息内容自动填入输入框" → Task 2 (textarea restore)
- "重新发送后作为新消息进入对话历史" → Already works: `submit_message()` adds a new UserBubble

**2. Placeholder scan:** No TBD/TODO/fill-in patterns found.

**3. Type consistency:**
- `PipelineAction::RebuildAll { prefix_len, tail_vms }` — matches existing usage in the `!agent_replied` branch
- `PipelineAction::AddMessage(vm)` — matches existing pattern
- `build_textarea(false)` — matches existing usage in the same file
- `state.history.truncate(history_len)` — `history_len: usize`, `state.history: Vec<BaseMessage>` — truncate takes usize, correct
- `result.ok: bool` — used in if condition, correct
- `history_len` is already defined at line 102 as `let history_len = history.len();`

**Edge cases verified:**
- **No `last_submitted_text` (e.g., background task interrupt):** Falls through to the `else` branch, shows "app-interrupt-done", no rollback attempted
- **SubAgent interrupt:** Early return with `(false, false, false)` — unchanged
- **Double Interrupted→Done:** `reconcile_already_done = true` prevents Done from overwriting UI state
- **ThreadStore persistence:** Only persisted on `result.ok` — cancelled rounds never persisted (unchanged behavior)
