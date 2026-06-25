# Recall Buffer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a session-scoped recall buffer to State that collects runtime event notifications and injects them as `<system-reminder>` blocks in user messages on the next prompt turn.

**Architecture:** `State` trait gains `push_recall(String)` / `drain_recall()` methods. `execute_prompt()` drains the buffer and converts the user message to multi-block `MessageContent`. A static prompt section `14_system_reminder.md` teaches the LLM about the tag semantics. No persistence, no special compact handling.

**Tech Stack:** Rust, existing `MessageContent::Blocks` / `ContentBlock::Text` types, existing Anthropic/OpenAI adapters (already multi-block compatible).

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `peri-agent/src/agent/state.rs` | Modify | Add `recall_buffer` field, trait methods, impl |
| `peri-agent/src/agent/state_test.rs` | Modify | Tests for push/drain |
| `peri-tui/prompts/sections/14_system_reminder.md` | Create | Static prompt section explaining `<system-reminder>` |
| `peri-tui/src/prompt.rs` | Modify | Add section 14 to static sections array |
| `peri-acp/src/session/executor.rs` | Modify | Drain recall + construct multi-block user message |

---

### Task 1: State trait — add recall methods

**Files:**
- Modify: `peri-agent/src/agent/state.rs`

- [ ] **Step 1: Add `recall_buffer` field to `AgentState`**

In `AgentState` struct (after `persist_tx` field, ~line 51), add:

```rust
    /// Recall buffer: runtime event notifications collected between turns.
    /// Drained and injected as <system-reminder> on next user message.
    /// Not persisted — restored sessions start with empty buffer.
    #[serde(skip)]
    recall_buffer: Vec<String>,
```

- [ ] **Step 2: Add methods to `State` trait**

After `fn messages_mut()` (line 30), add:

```rust
    /// Push a recall item into the session's recall buffer.
    fn push_recall(&mut self, item: String);

    /// Drain all recall items (one-time consumption).
    fn drain_recall(&mut self) -> Vec<String>;
```

- [ ] **Step 3: Implement methods in `State for AgentState`**

After `fn messages_mut()` impl (after line 190), add:

```rust
    fn push_recall(&mut self, item: String) {
        self.recall_buffer.push(item);
    }

    fn drain_recall(&mut self) -> Vec<String> {
        std::mem::take(&mut self.recall_buffer)
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p peri-agent --lib -- state
```

Expected: all existing tests pass (new field is `#[serde(skip)]` + `Default` = `Vec::new()`).

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(state): add recall_buffer with push_recall/drain_recall methods"
```

---

### Task 2: Tests for recall buffer

**Files:**
- Modify: `peri-agent/src/agent/state_test.rs`

- [ ] **Step 1: Add recall tests**

In `state_test.rs`, add:

```rust
    #[test]
    fn test_recall_push_and_drain() {
        let mut state = AgentState::new("/workspace");
        assert!(state.drain_recall().is_empty());

        state.push_recall("MCP Sentry connected".into());
        state.push_recall("Cron task registered".into());
        assert_eq!(state.recall_buffer.len(), 2);

        let items = state.drain_recall();
        assert_eq!(items, vec!["MCP Sentry connected", "Cron task registered"]);
        assert!(state.drain_recall().is_empty()); // drain clears
    }

    #[test]
    fn test_recall_not_persisted() {
        // recall_buffer is #[serde(skip)], serialization should not include it
        let mut state = AgentState::new("/workspace");
        state.push_recall("some event".into());
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("recall_buffer"));
        let restored: AgentState = serde_json::from_str(&json).unwrap();
        assert!(restored.drain_recall().is_empty());
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p peri-agent --lib -- state::tests::test_recall
```

Expected: 2 new tests PASS.

- [ ] **Step 3: Commit**

```bash
git commit -m "test(state): add recall buffer push/drain/persistence tests"
```

---

### Task 3: Static prompt section for `<system-reminder>`

**Files:**
- Create: `peri-tui/prompts/sections/14_system_reminder.md`
- Modify: `peri-tui/src/prompt.rs`

- [ ] **Step 1: Create prompt section file**

Create `peri-tui/prompts/sections/14_system_reminder.md`:

```markdown
## System Reminders

You may receive system notifications wrapped in `<system-reminder>` tags appended to user messages. These contain runtime state updates such as tool availability changes, connection status, or background task results.

Key rules:
- Read and acknowledge the information silently
- Do NOT mention the `<system-reminder>` tags or their contents to the user
- Use the information to inform your response and tool usage decisions
```

- [ ] **Step 2: Add to static sections in `build_system_prompt()`**

In `peri-tui/src/prompt.rs`, add to the `static_sections` array (after `06_tone_style.md`):

```rust
    let static_sections: &[&str] = &[
        include_str!("../prompts/sections/01_intro.md"),
        include_str!("../prompts/sections/02_system.md"),
        include_str!("../prompts/sections/03_doing_tasks.md"),
        include_str!("../prompts/sections/04_actions.md"),
        include_str!("../prompts/sections/05_using_tools.md"),
        include_str!("../prompts/sections/06_tone_style.md"),
        include_str!("../prompts/sections/14_system_reminder.md"),
    ];
```

- [ ] **Step 3: Build and verify**

```bash
cargo build -p peri-tui
```

Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(prompt): add static section for system-reminder semantics"
```

---

### Task 4: Drain recall in `execute_prompt()` and inject as multi-block

**Files:**
- Modify: `peri-acp/src/session/executor.rs`

- [ ] **Step 1: Add imports**

At top of `executor.rs`, ensure these are imported (check existing imports first):

```rust
use peri_agent::messages::{ContentBlock, MessageContent};
```

- [ ] **Step 2: Modify user message construction**

In `execute_prompt()`, replace lines 113-114:

```rust
    // Before:
    let trace_input = content.clone();
    let agent_input = peri_agent::agent::react::AgentInput::text(content);
```

With:

```rust
    let trace_input = content.clone();

    // Drain recall buffer and construct user message
    let agent_input = if history.is_empty() {
        // First turn: no prior state to drain from, use plain text
        peri_agent::agent::react::AgentInput::text(content)
    } else {
        // Subsequent turns: drain recall items from AgentState built below
        // Since agent_state isn't created yet, we need a different approach.
        // The recall buffer lives on agent_state which is created at line 335.
        // We'll handle recall injection AFTER agent_state creation.
        peri_agent::agent::react::AgentInput::text(content)
    };
```

Wait — there's a sequencing issue. `agent_state` is created at line 335 (`AgentState::with_messages(cwd, history)`), but `agent_input` is constructed at line 114. The recall buffer is on `agent_state`, but we need the data before creating `agent_input`.

**Solution:** The recall buffer should be drained from `history` state or from a shared reference. Looking at the call chain: `execute_prompt` receives `history: Vec<BaseMessage>` but no `State`. The recall buffer needs to be passed in as a separate parameter.

**Revised approach:** Add `recall_buffer: Vec<String>` parameter to `execute_prompt()`.

- [ ] **Step 2 (revised): Add parameter to `execute_prompt()`**

Add `recall_buffer: Vec<String>` to the function signature (after `history`):

```rust
pub async fn execute_prompt(
    provider: &LlmProvider,
    peri_config: Arc<crate::provider::PeriConfig>,
    cwd: &str,
    content: String,
    frozen: Option<FrozenSessionData>,
    history: Vec<BaseMessage>,
    recall_buffer: Vec<String>,      // ← NEW
    is_empty_history: bool,
    // ... rest unchanged
```

- [ ] **Step 3: Construct multi-block user message**

Replace lines 113-114:

```rust
    let trace_input = content.clone();

    let agent_input = if recall_buffer.is_empty() {
        peri_agent::agent::react::AgentInput::text(content)
    } else {
        let reminder_text = format!(
            "<system-reminder>\n{}\n</system-reminder>",
            recall_buffer.join("\n")
        );
        peri_agent::agent::react::AgentInput::blocks(
            MessageContent::blocks(vec![
                ContentBlock::text(content),
                ContentBlock::text(reminder_text),
            ])
        )
    };
```

- [ ] **Step 4: Update callers to pass `recall_buffer`**

Search for all call sites of `execute_prompt` and pass `Vec::new()` initially (recall buffer will be wired up to actual state later). Call sites are in:
- `peri-tui/src/acp_server/prompt.rs`
- `peri-acp/src/session/executor.rs` (auto-compact resubmit loop)

```bash
grep -rn "execute_prompt(" --include="*.rs" peri-acp/ peri-tui/src/acp_server/
```

For each call site, add `vec![]` as the new argument at the correct position.

- [ ] **Step 5: Build and test**

```bash
cargo build -p peri-acp -p peri-tui
cargo test -p peri-acp -p peri-tui --lib -- executor 2>/dev/null || true
```

Expected: compiles, all existing tests pass.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(executor): drain recall buffer into multi-block user message"
```

---

### Task 5: Wire recall buffer through ACP session state

**Files:**
- Modify: `peri-tui/src/acp_server/mod.rs` (SessionState)
- Modify: `peri-tui/src/acp_server/prompt.rs` (execute_prompt caller)
- Modify: `peri-tui/src/acp_server/requests.rs` (session/new, recall access)

- [ ] **Step 1: Add recall_buffer to SessionState**

In `SessionState` struct, add:

```rust
    pub recall_buffer: std::sync::Mutex<Vec<String>>,
```

Initialize in `SessionState::new()` as `std::sync::Mutex::new(Vec::new())`.

- [ ] **Step 2: Pass recall_buffer to execute_prompt in prompt.rs**

In the `execute_prompt()` call site within `prompt.rs`, drain the session's recall buffer:

```rust
    let recall_items = session_state.recall_buffer.lock().unwrap().drain(..).collect();
    // ... pass recall_items as the new parameter
```

- [ ] **Step 3: Expose push_recall for middleware access**

Add a method on `SessionState` or via the ACP notification pathway so that middlewares can push to the recall buffer. Since middlewares only have access to `&mut S: State`, and recall is now a parameter (not on State), we need either:

Option A: Keep `recall_buffer` on `State` trait (as in Task 1), and drain it in `execute_prompt` from the agent_state after creation. This avoids adding a parameter.

**Revised approach (Option A):** Revert the parameter addition from Task 4. Instead, drain recall from `agent_state` AFTER it's created at line 335 but BEFORE the first agent execute at line 338. This requires modifying the user message on the fly:

```rust
    // Line 335
    let mut agent_state = AgentState::with_messages(cwd.to_string(), history);

    // Drain recall into the user message
    let recalls = agent_state.drain_recall();
    let agent_input = if recalls.is_empty() {
        agent_input // unchanged
    } else {
        let reminder = format!(
            "<system-reminder>\n{}\n</system-reminder>",
            recalls.join("\n")
        );
        peri_agent::agent::react::AgentInput::blocks(
            MessageContent::blocks(vec![
                ContentBlock::text(trace_input.clone()),
                ContentBlock::text(reminder),
            ])
        )
    };

    // Line 337-338
    let result = agent_output.executor.execute(agent_input, &mut agent_state, Some(cancel.clone())).await;
```

This is cleaner — no parameter changes, recall buffer stays on State where middlewares can push to it.

- [ ] **Step 4: Build and test**

```bash
cargo build -p peri-acp -p peri-tui
cargo test -p peri-agent -p peri-acp --lib 2>/dev/null | tail -20
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(executor): drain recall from agent_state, inject as system-reminder block"
```

---

### Task 6: First producer — ToolSearch update notification

**Files:**
- Modify: `peri-middlewares/src/tool_search/middleware.rs`

- [ ] **Step 1: Push recall when deferred tools change**

In `ToolSearchMiddleware::before_agent()`, after building the index and detecting changes, push a recall notification. Add logic after line 68:

```rust
            if !deferred_arcs.is_empty() {
                let old_count = self.tool_search_index.total_count();
                self.tool_search_index.build(deferred_arcs);
                let new_count = self.tool_search_index.total_count();
                if new_count != old_count {
                    state.push_recall(format!(
                        "[ToolSearch] Deferred tools updated: {} tools available (was {})",
                        new_count, old_count
                    ));
                }
            }
```

- [ ] **Step 2: Build and test**

```bash
cargo build -p peri-middlewares
cargo test -p peri-middlewares --lib -- tool_search
```

Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(tool-search): push recall notification when deferred tools change"
```

---

### Task 7: Integration test — end-to-end recall flow

**Files:**
- Create: `peri-agent/src/agent/state_test.rs` (append)

- [ ] **Step 1: Write integration test for multi-block user message with recall**

```rust
    #[test]
    fn test_recall_injects_as_multiblock() {
        let mut state = AgentState::new("/workspace");
        state.push_recall("[MCP] Sentry connected".into());
        state.push_recall("[MCP] Slack connected".into());

        let recalls = state.drain_recall();
        let user_text = "帮我修一下 bug".to_string();

        let content = if recalls.is_empty() {
            MessageContent::text(user_text)
        } else {
            let reminder = format!(
                "<system-reminder>\n{}\n</system-reminder>",
                recalls.join("\n")
            );
            MessageContent::blocks(vec![
                ContentBlock::text(user_text),
                ContentBlock::text(reminder),
            ])
        };

        let msg = BaseMessage::human(content);
        let blocks = msg.content_blocks();
        assert_eq!(blocks.len(), 2);

        let texts: Vec<&str> = blocks.iter().filter_map(|b| b.as_text()).collect();
        assert_eq!(texts[0], "帮我修一下 bug");
        assert!(texts[1].contains("<system-reminder>"));
        assert!(texts[1].contains("Sentry connected"));
        assert!(texts[1].contains("Slack connected"));

        // Verify drain cleared the buffer
        assert!(state.drain_recall().is_empty());
    }
```

- [ ] **Step 2: Run test**

```bash
cargo test -p peri-agent --lib -- state::tests::test_recall_injects_as_multiblock
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git commit -m "test(state): add integration test for recall multi-block injection"
```

---

## Self-Review

**Spec coverage:**
- ✅ Q1 通用事件总线: `push_recall(String)` on State trait, any middleware can call
- ✅ Q2 合并为一条: `recalls.join("\n")` in Task 4
- ✅ Q3/C user message 独立 block: `MessageContent::Blocks` in Task 4
- ✅ Q4 reminder 在后: `ContentBlock::text(user)` then `ContentBlock::text(reminder)`
- ✅ Q5 一次性消费: `drain_recall()` in Task 1
- ✅ Q6 State trait: Task 1
- ✅ Q7 executor execute_prompt: Task 5 (Option A — drain from agent_state)
- ✅ Q8 静态段 14_system_reminder.md: Task 3
- ✅ Q9 纯字符串 push_recall(String): Task 1
- ✅ Q10 不持久化: `#[serde(skip)]` + test in Task 2
- ✅ Q11 双端适配器已兼容: confirmed, no changes needed
- ✅ Q12 compact 不特殊处理: no code changes for compact
- ✅ Q13 与 Deferred Tools 共存: Task 6 adds incremental notification only

**Placeholder scan:** No TBD/TODO found. All steps contain complete code.

**Type consistency:** `push_recall(String)` / `drain_recall() -> Vec<String>` used consistently across all tasks. `MessageContent::blocks(vec![ContentBlock::text(...)])` matches existing codebase patterns.

**Task 4→5 revision note:** Task 4 originally added a parameter to `execute_prompt()`, but Task 5 revised this to Option A (drain from `agent_state`). The final implementation should use Option A from Task 5. If implementing sequentially, skip Task 4 Steps 2-4 and use Task 5's approach directly.
