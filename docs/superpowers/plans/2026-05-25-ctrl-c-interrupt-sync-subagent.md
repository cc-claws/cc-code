# Ctrl+C Interrupt Sync SubAgent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ctrl+C immediately interrupt a synchronously executing SubAgent, returning control to the user.

**Architecture:** The fix adds `tokio::select!` at two levels: (1) inside `SubAgentTool::invoke()` for sync sub-agents, racing `child_cancel.cancelled()` against the child's `execute()`, so cancellation drops the child future directly; (2) inside `executor::execute_prompt()`, racing `cancel.cancelled()` against the agent's `execute()`, as defense-in-depth. The cancel token chain (`session → executor → builder → SubAgentMiddleware → SubAgentTool → child_token`) already propagates correctly — the gap is the absence of a `select!` at the actual `.await` point in the sync sub-agent path.

**Tech Stack:** Rust, tokio, tokio_util::sync::CancellationToken

**Root Cause:** `peri-middlewares/src/subagent/tool/define.rs:1157-1159` does a bare `agent_builder.execute(...).await` — there is no `tokio::select!` wrapping it. The parent's `tool_dispatch.rs:234` select! wraps the Agent tool invocation, and the child's internal `call_llm`/`dispatch_tools` have cancel checkpoints, but if the child agent is between checkpoints (e.g., in `after_model` or `before_model` middleware hooks, or the cancel wakeup is missed), the cancellation signal is not acted upon until the child naturally completes. Adding a direct `select!` at the `define.rs` level ensures immediate future drop on cancel.

---

### Task 1: Add `tokio::select!` wrapper in `SubAgentTool::invoke()` sync path

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs:1157-1165`

- [ ] **Step 1: Wrap the child's `execute()` call with `tokio::select!`**

Replace the bare `.await` at lines 1157-1165:

```rust
// BEFORE (lines 1151-1165):
        tracing::info!(
            "[DEADLOCK] SubAgentTool: START child execute, agent_id={}, prompt_len={}",
            agent_id,
            prompt.len()
        );
        let exec_start = std::time::Instant::now();
        let exec_result = agent_builder
            .execute(AgentInput::text(prompt), &mut state, Some(child_cancel))
            .await;
        tracing::info!(
            "[DEADLOCK] SubAgentTool: END child execute ({:.1?}), agent_id={}, is_ok={}",
            exec_start.elapsed(),
            agent_id,
            exec_result.is_ok()
        );
```

```rust
// AFTER:
        let child_cancel_for_select = child_cancel.clone();
        let exec_fut = agent_builder
            .execute(AgentInput::text(prompt), &mut state, Some(child_cancel));
        tracing::info!(
            "[DEADLOCK] SubAgentTool: START child execute, agent_id={}, prompt_len={}",
            agent_id,
            prompt.len()
        );
        let exec_start = std::time::Instant::now();
        let exec_result = tokio::select! {
            biased;
            _ = child_cancel_for_select.cancelled() => {
                tracing::info!(
                    "[DEADLOCK] SubAgentTool: child cancelled after {:.1?}, agent_id={}",
                    exec_start.elapsed(),
                    agent_id
                );
                Err(peri_agent::error::AgentError::Interrupted)
            }
            result = exec_fut => result,
        };
        tracing::info!(
            "[DEADLOCK] SubAgentTool: END child execute ({:.1?}), agent_id={}, is_ok={}",
            exec_start.elapsed(),
            agent_id,
            exec_result.is_ok()
        );
```

**Rationale:** `child_cancel` is already a `child_token()` of the parent cancel token (computed at lines 1127-1131). When the parent token is cancelled via Ctrl+C, `child_cancel.cancelled()` resolves immediately. The `biased;` ensures the cancel branch is checked first. The `child_cancel_for_select` clone is needed because `child_cancel` may be moved into `execute()` or the `select!` branch — the clone avoids the borrow conflict.

- [ ] **Step 2: Build and check for compilation errors**

Run: `cargo build -p peri-middlewares 2>&1`
Expected: Compilation succeeds without errors. The `peri_agent::error::AgentError::Interrupted` is already in scope via imports at the top of `define.rs`.

---

### Task 2: Add `tokio::select!` wrapper in `executor::execute_prompt()` as defense-in-depth

**Files:**
- Modify: `peri-acp/src/session/executor.rs:468-473`

- [ ] **Step 1: Wrap the agent's `execute()` with `tokio::select!`**

Replace the bare `.await` at lines 468-473:

```rust
// BEFORE (lines 468-474):
    // Execute agent
    let mut agent_state = AgentState::with_messages(cwd.to_string(), history);
    let result = agent_output
        .executor
        .execute(agent_input.clone(), &mut agent_state, Some(cancel.clone()))
        .await;
    drop(agent_output.executor);
```

```rust
// AFTER:
    // Execute agent — move executor out to avoid drop-after-borrow conflict.
    // exec_fut borrows the executor, so we extract it from agent_output first,
    // then drop after the select! completes.
    let mut agent_state = AgentState::with_messages(cwd.to_string(), history);
    let cancel_for_select = cancel.clone();
    let mut executor = agent_output.executor;
    let exec_fut = executor.execute(agent_input.clone(), &mut agent_state, Some(cancel.clone()));
    let result = tokio::select! {
        biased;
        _ = cancel_for_select.cancelled() => {
            Err(AgentError::Interrupted)
        }
        result = exec_fut => result,
    };
    drop(executor);
```

- [ ] **Step 2: Build and check for compilation errors**

Run: `cargo build -p peri-acp 2>&1`
Expected: Compilation succeeds. `AgentError` is imported at `executor.rs:17` via `use peri_agent::error::AgentError;`.

---

### Task 3: Verify with full build

**Files:**
- None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build 2>&1`
Expected: All crates compile without errors.

- [ ] **Step 2: Run existing tests**

Run: `cargo test -p peri-middlewares --lib 2>&1`
Expected: All existing tests pass.

Run: `cargo test -p peri-acp --lib 2>&1`
Expected: All existing tests pass.

---

### Task 4: Manual verification notes

- [ ] **Verify Ctrl+C interrupts sync SubAgent**

Manual test steps:
1. Start TUI: `cargo run -p peri-tui`
2. Submit a prompt that triggers a sync SubAgent (e.g., "use the code-reviewer agent to review src/main.rs")
3. While the SubAgent is running (visible in UI), press Ctrl+C
4. Expected: UI immediately returns to idle state, last user message is undone, SubAgent execution stops

- [ ] **Verify background SubAgents are NOT affected**

1. Trigger a background SubAgent
2. Press Ctrl+C while background SubAgent is running
3. Expected: Background SubAgent continues executing, parent is cancelled normally

---
