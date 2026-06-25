# Compact 后 ThreadStore 未更新导致 Session 恢复时消息重复

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix compact-duplicated messages on session restore by updating the ThreadStore when compact reduces message count.

**Architecture:** Single-file fix in `prompt.rs`. After execution, when compact causes `result.messages.len() < history_len`, delete old messages from ThreadStore and persist compacted messages.

**Tech Stack:** Rust

**Background:** In `prompt.rs:142-151`, after successful agent execution, only messages that GROW are persisted to ThreadStore. When compact replaces many messages with a few (making `result.messages.len() < history_len`), nothing is persisted. Old messages remain in ThreadStore permanently. On session restore, both old (pre-compact) and new (post-compact) messages are loaded, causing duplication.

---

### Task 1: Fix ThreadStore persistence for compacted messages

**Files:**
- Modify: `peri-tui/src/acp_server/prompt.rs`

- [ ] **Step 1: Save history message IDs before history is moved**

In `prompt.rs`, after line 93 (`let history_len = history.len();`), add:

```rust
let history_len = history.len();
// Save message IDs for compact persistence path (history is moved into execute_prompt below).
let history_ids: Vec<peri_agent::messages::MessageId> =
    history.iter().map(|m| m.id()).collect();
```

This is cheap — only copies `MessageId` (UUID-backed, ~16 bytes), not the entire message content.

- [ ] **Step 2: Add compact persistence branch**

Replace lines 142-151 (the success path persistence block):

```rust
if result.ok {
    info!(session_id = %session_id, messages = result.messages.len(), "Agent execution completed");
    if history_len < result.messages.len() {
        // Normal: new messages appended (e.g. user msg + AI response + tool results)
        let new_msgs = &result.messages[history_len..];
        if let Err(e) = thread_store.append_messages(&thread_id, new_msgs).await {
            tracing::warn!(error = %e, "Failed to persist messages to ThreadStore");
        }
    } else if result.messages.len() < history_len {
        // Compact replaced own messages with a condensed summary.
        // Old messages must be removed from ThreadStore and compacted messages persisted,
        // otherwise session restore loads old + new messages together, causing duplication.
        info!(
            session_id = %session_id,
            old_count = history_len,
            new_count = result.messages.len(),
            "Compact detected: updating ThreadStore"
        );
        if let Err(e) = thread_store.delete_messages(&thread_id, &history_ids).await {
            tracing::warn!(error = %e, "Failed to delete pre-compact messages from ThreadStore");
        }
        if let Err(e) = thread_store.append_messages(&thread_id, &result.messages).await {
            tracing::warn!(error = %e, "Failed to persist compacted messages to ThreadStore");
        }
    }
    // else: history_len == result.messages.len() — no change (e.g., empty response)
    state.history = result.messages;
}
```

Why `history_ids` is safe to delete:
- For the main agent, `ancestor_len = 0` — ALL messages in history are own messages replaced by compact
- `delete_messages` in SQLite store does `DELETE WHERE message_id = ?` — non-existent IDs are silently ignored (no-ops)
- Filesystem store's `delete_messages` is already a no-op

- [ ] **Step 3: Verify compilation**

```bash
cargo build -p peri-tui
```

Expected: PASS

- [ ] **Step 4: Run existing tests**

```bash
cargo test -p peri-tui -p peri-acp --lib
```

Expected: all existing tests PASS (no new tests in this task — the fix is in the ACP server handler which requires full server setup to test)

- [ ] **Step 5: Manual verification scenario**

1. Start TUI, have a long conversation that triggers auto-compact (or manually run `/compact`)
2. Verify in-session behavior is normal (compact shows summary, conversation continues)
3. Exit TUI
4. Restart TUI, resume the session
5. Verify NO duplicate messages — old pre-compact messages should NOT appear

- [ ] **Step 6: Check clippy**

```bash
cargo clippy -p peri-tui
```

Expected: no new warnings

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/acp_server/prompt.rs
git commit -m "fix: persist compacted messages to ThreadStore, delete pre-compact messages

When compact reduces message count (result.messages.len() < history_len),
delete old messages from ThreadStore and persist the compacted state.
Without this, session restore loads old pre-compact messages alongside
compacted ones, causing duplicated conversation history.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Verification Checklist

```bash
cargo build -p peri-tui
cargo test -p peri-tui -p peri-acp --lib
cargo clippy -p peri-tui
lefthook run pre-commit
```
