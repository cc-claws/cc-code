# Stale Code Cleanup Batch 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove dead code, unused dependencies, and stale artifacts across all 7 workspace crates — continuing batch 1/2 cleanup.

**Architecture:** Mechanical deletions verified by exhaustive workflow scan (8 dimensions × 90 agents, 71/81 findings confirmed). Each task is independent and compiles alone.

**Tech Stack:** Rust 2021, cargo clippy, cargo build

---

## File Structure

### Files to DELETE
| File | Reason |
|------|--------|
| `peri-tui/src/prompt.rs` | Duplicate of `peri-acp/src/prompt/mod.rs`, zero callers |
| `peri-tui/src/prompt_test.rs` | Tests for dead module |
| `peri-tui/src/langfuse/mod.rs` | Bridge module, zero callers |

### Files to MODIFY
| File | Change |
|------|--------|
| `peri-tui/src/lib.rs` | Remove `pub mod prompt;`, `pub mod langfuse;`, remove 7 dead `#![allow]` lints |
| `peri-tui/src/app/agent_comm.rs` | Remove `agent_rx` field + comments |
| `peri-tui/src/app/agent_ops/polling.rs` | Remove `agent_rx` fallback block (lines 53-187 of `poll_agent_events`) |
| `peri-tui/src/app/agent_events_bg.rs` | Remove `agent_rx = None` cleanup |
| `peri-tui/src/app/mod.rs` | Remove `agent_rx = None` cleanup |
| `peri-tui/src/app/thread_ops.rs` | Remove `agent_rx = None` cleanup, remove `alloc_collect()` fn + 2 calls |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | Remove 2× `agent_rx = None` cleanup |
| `peri-tui/src/app/agent_compact.rs` | Remove `alloc_collect()` call |
| `peri-agent/src/error.rs` | Remove `StateError(String)` variant |
| `peri-agent/src/agent/events.rs` | Remove `StepDone`, `SessionEnded` variants |
| `peri-agent/src/llm/types.rs` | Remove `from_anthropic()`, keep `from_display()` |
| `peri-agent/src/llm/anthropic/invoke.rs` | Change `from_anthropic` → `from_display` |
| `peri-agent/src/llm/anthropic/stream.rs` | Change `from_anthropic` → `from_display` |
| `peri-agent/src/agent/events_test.rs` | Remove `SessionEnded` test |
| `peri-middlewares/src/lib.rs` | Remove 3 dead `#![allow]` lints |
| `peri-middlewares/src/mcp/transport.rs` | Remove 2 dead `TransportError` variants |
| `peri-middlewares/src/plugin/installer/mod.rs` | Remove `ManifestInvalid` variant |
| `peri-middlewares/src/plugin/config.rs` | Remove `MissingField` variant |
| `peri-middlewares/src/mcp/oauth_flow.rs` | Remove `FlowFailed` variant |
| `peri-middlewares/src/mcp/oauth_flow_test.rs` | Remove `FlowFailed` test |
| `peri-middlewares/src/mcp/client.rs` | Remove `CallTimeout` variant |
| `peri-middlewares/src/mcp/auth_store.rs` | Remove `NotFound` variant |
| `peri-middlewares/src/process/mod.rs` | Remove `spawn_shell()` + `spawn_shell_with_env()` |
| `peri-acp/src/event/mapper.rs` | Remove `StepDone`, `SessionEnded` match arms |
| `peri-acp/src/event/mapper_test.rs` | Remove `StepDone`, `SessionEnded` tests |
| `peri-tui/src/app/agent.rs` | Remove `StepDone`, `SessionEnded` match arms |
| `langfuse-client/src/client.rs` | Remove `ingest_native()` |
| `peri-lsp/src/diagnostics.rs` | Remove `has_issues()`, `clear_for_file()` |
| `peri-lsp/src/client.rs` | Remove `did_close()` |
| `peri-lsp/src/protocol/notifications.rs` | Remove `did_close_notification()` |
| `peri-lsp/src/error.rs` | Remove `#[from]` from `Io` and `Json` variants |
| `Cargo.toml` | Remove `tokio-test` workspace dep |
| `peri-agent/Cargo.toml` | Remove `tokio-test`, remove `"blocking"` from reqwest |
| `peri-acp/Cargo.toml` | Remove `tokio-test` |
| `peri-middlewares/Cargo.toml` | Remove `tokio-test` |
| `langfuse-client/Cargo.toml` | Remove `tokio-test` |
| `peri-lsp/Cargo.toml` | Remove `tokio-test` |
| `peri-widgets/Cargo.toml` | Change `lru = "0.12"` → `lru.workspace = true` |
| `peri-tui/Cargo.toml` | Change `thiserror = "1"` → `thiserror.workspace = true` |

---

### Task 1: Delete dead peri-tui prompt module

**Files:**
- Delete: `peri-tui/src/prompt.rs`
- Delete: `peri-tui/src/prompt_test.rs`
- Modify: `peri-tui/src/lib.rs:22`

- [ ] **Step 1: Delete prompt.rs and prompt_test.rs**

```bash
rm peri-tui/src/prompt.rs peri-tui/src/prompt_test.rs
```

- [ ] **Step 2: Remove `pub mod prompt;` from lib.rs**

In `peri-tui/src/lib.rs`, delete the line:
```
pub mod prompt;
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p peri-tui`
Expected: SUCCESS (zero references to `crate::prompt` exist)

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: delete dead peri-tui prompt module (duplicate of peri-acp)
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Delete dead peri-tui langfuse bridge module

**Files:**
- Delete: `peri-tui/src/langfuse/mod.rs` (and directory)
- Modify: `peri-tui/src/lib.rs:21`

- [ ] **Step 1: Delete langfuse directory**

```bash
rm -rf peri-tui/src/langfuse/
```

- [ ] **Step 2: Remove `pub mod langfuse;` from lib.rs**

In `peri-tui/src/lib.rs`, delete the line:
```
pub mod langfuse; // temporary bridge re-export from peri-acp
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p peri-tui`
Expected: SUCCESS (all code uses `peri_acp::langfuse::*` directly)

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: delete dead peri-tui langfuse bridge module
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Remove dead `agent_rx` field and legacy fallback path

**Files:**
- Modify: `peri-tui/src/app/agent_comm.rs` (remove field + comments)
- Modify: `peri-tui/src/app/agent_ops/polling.rs` (remove legacy block)
- Modify: `peri-tui/src/app/agent_events_bg.rs:343`
- Modify: `peri-tui/src/app/mod.rs:466`
- Modify: `peri-tui/src/app/thread_ops.rs:142`
- Modify: `peri-tui/src/app/agent_ops/lifecycle.rs:139,369`

- [ ] **Step 1: Remove `agent_rx` field from `agent_comm.rs`**

In `peri-tui/src/app/agent_comm.rs`:
- Delete the field declaration: `pub agent_rx: Option<mpsc::Receiver<AgentEvent>>,`
- Delete the comment: `/// ACP notification receiver (new path, replaces agent_rx)`
- Delete the comment block mentioning `agent_rx` / BackgroundTaskCompleted (lines 63-67)
- Delete the `agent_rx: None,` line from the Default impl

- [ ] **Step 2: Simplify `polling.rs` — remove agent_rx check and legacy block**

In `peri-tui/src/app/agent_ops/polling.rs`:

Remove the `has_legacy_rx` variable and simplify the early-return check:
```rust
// BEFORE:
let has_legacy_rx = self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .agent_rx
    .is_some();
if !has_acp && !has_legacy_rx {
// AFTER:
if !has_acp {
```

Then remove the entire legacy block (lines 99-187): the comment `// Try legacy agent_rx channel (backward compat)` through the closing `}` of the match. The `loop` body should now only contain the ACP notification path.

The loop body becomes:
```rust
loop {
    let acp_result = self.session_mgr.sessions[self.session_mgr.active]
        .agent
        .acp_notification_rx
        .as_mut()
        .map(|rx| rx.try_recv());
    if let Some(Ok(notif)) = acp_result {
        let (ev_updated, should_break, should_return) = self.handle_acp_notification(notif);
        if ev_updated {
            updated = true;
        }
        if should_return {
            return true;
        }
        if should_break {
            break;
        }
        continue;
    }
    break;
}
```

- [ ] **Step 3: Remove all `agent_rx = None` cleanup lines**

In each of these files, delete the line `.agent_rx = None;`:
- `peri-tui/src/app/agent_events_bg.rs:343`
- `peri-tui/src/app/mod.rs:466`
- `peri-tui/src/app/thread_ops.rs:142`
- `peri-tui/src/app/agent_ops/lifecycle.rs:139`
- `peri-tui/src/app/agent_ops/lifecycle.rs:369`

Note: If the line is part of a multi-field struct update (e.g., `self.session_mgr.sessions[...].agent = AgentComm { agent_rx: None, ..Default::default() }`), remove only the `agent_rx: None,` field, not the whole expression.

- [ ] **Step 4: Build to verify**

Run: `cargo build -p peri-tui`
Expected: SUCCESS

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: remove dead agent_rx legacy fallback path from peri-tui
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Remove empty `alloc_collect()` function and calls

**Files:**
- Modify: `peri-tui/src/app/thread_ops.rs:8-11` (remove function + 2 calls)
- Modify: `peri-tui/src/app/agent_compact.rs:100` (remove call)

- [ ] **Step 1: Remove `alloc_collect()` calls**

In `peri-tui/src/app/thread_ops.rs`, delete the 2 call sites:
- Line ~257: `alloc_collect();`
- Line ~370: `alloc_collect();`

In `peri-tui/src/app/agent_compact.rs`, delete the call site:
- Line ~100: `alloc_collect();`

- [ ] **Step 2: Remove `alloc_collect()` function definitions**

In `peri-tui/src/app/thread_ops.rs`, delete both cfg-gated definitions:
```rust
#[cfg(not(target_os = "windows"))]
pub(crate) fn alloc_collect() {}

#[cfg(target_os = "windows")]
pub(crate) fn alloc_collect() {}
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p peri-tui`
Expected: SUCCESS

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: remove empty alloc_collect() function and calls
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Remove dead error enum variants

**Files:**
- Modify: `peri-agent/src/error.rs`
- Modify: `peri-middlewares/src/mcp/transport.rs`
- Modify: `peri-middlewares/src/plugin/installer/mod.rs`
- Modify: `peri-middlewares/src/plugin/config.rs`
- Modify: `peri-middlewares/src/mcp/oauth_flow.rs`
- Modify: `peri-middlewares/src/mcp/oauth_flow_test.rs`
- Modify: `peri-middlewares/src/mcp/client.rs`
- Modify: `peri-middlewares/src/mcp/auth_store.rs`

- [ ] **Step 1: Remove `AgentError::StateError(String)` from `peri-agent/src/error.rs`**

Delete the variant line: `StateError(String),`

- [ ] **Step 2: Remove 2 dead `TransportError` variants from `peri-middlewares/src/mcp/transport.rs`**

Delete:
```rust
StdioLaunchFailed(String),
HttpConfigFailed(String),
```

- [ ] **Step 3: Remove `InstallerError::ManifestInvalid` from `peri-middlewares/src/plugin/installer/mod.rs`**

Delete the variant:
```rust
ManifestInvalid { path: PathBuf, source: serde_json::Error },
```

- [ ] **Step 4: Remove `PluginConfigError::MissingField` from `peri-middlewares/src/plugin/config.rs`**

Delete the variant:
```rust
MissingField { field: String },
```

- [ ] **Step 5: Remove `OAuthFlowError::FlowFailed` from `peri-middlewares/src/mcp/oauth_flow.rs`**

Delete the variant: `FlowFailed(String),`

- [ ] **Step 6: Remove `FlowFailed` test from `peri-middlewares/src/mcp/oauth_flow_test.rs`**

Delete the test function that constructs `OAuthFlowError::FlowFailed("test".to_string())`.

- [ ] **Step 7: Remove `McpPoolError::CallTimeout` from `peri-middlewares/src/mcp/client.rs`**

Delete the variant:
```rust
CallTimeout { server: String },
```

- [ ] **Step 8: Remove `AuthStoreError::NotFound` from `peri-middlewares/src/mcp/auth_store.rs`**

Delete the variant:
```rust
NotFound { server: String },
```

- [ ] **Step 9: Build to verify**

Run: `cargo build --all`
Expected: SUCCESS (all variants are only matched by `_` wildcards or never matched at all)

- [ ] **Step 10: Commit**

```bash
git add -A && git commit -m "chore: remove dead error enum variants (7 variants across 7 enums)
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Remove dead `AgentEvent` variants (`StepDone`, `SessionEnded`)

**Files:**
- Modify: `peri-agent/src/agent/events.rs`
- Modify: `peri-agent/src/agent/events_test.rs`
- Modify: `peri-acp/src/event/mapper.rs`
- Modify: `peri-acp/src/event/mapper_test.rs`
- Modify: `peri-tui/src/app/agent.rs`

- [ ] **Step 1: Remove variants from `peri-agent/src/agent/events.rs`**

Delete:
```rust
StepDone { step: usize },
```
Delete:
```rust
SessionEnded,
```

- [ ] **Step 2: Remove `SessionEnded` test from `peri-agent/src/agent/events_test.rs`**

Delete the test that constructs `AgentEvent::SessionEnded` and asserts deserialization.

- [ ] **Step 3: Remove match arms from `peri-acp/src/event/mapper.rs`**

Delete the match arms:
```rust
ExecutorEvent::StepDone { .. } => ...
ExecutorEvent::SessionEnded => ...
```

- [ ] **Step 4: Remove tests from `peri-acp/src/event/mapper_test.rs`**

Delete test functions for `step_done` and `session_ended` mapping.

- [ ] **Step 5: Remove match arms from `peri-tui/src/app/agent.rs`**

Delete the match arms:
```rust
ExecutorEvent::StepDone { .. } => return None,
ExecutorEvent::SessionEnded => return None,
```

- [ ] **Step 6: Build to verify**

Run: `cargo build --all`
Expected: SUCCESS

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "chore: remove dead AgentEvent::StepDone and SessionEnded variants
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: Remove dead functions across crates

**Files:**
- Modify: `peri-middlewares/src/process/mod.rs`
- Modify: `langfuse-client/src/client.rs`
- Modify: `peri-lsp/src/diagnostics.rs`
- Modify: `peri-lsp/src/client.rs`
- Modify: `peri-lsp/src/protocol/notifications.rs`
- Modify: `peri-lsp/src/error.rs`

- [ ] **Step 1: Remove `spawn_shell()` and `spawn_shell_with_env()` from `peri-middlewares/src/process/mod.rs`**

Delete both function definitions. Keep `shell_command()` which IS used.

- [ ] **Step 2: Remove `LangfuseClient::ingest_native()` from `langfuse-client/src/client.rs`**

Delete the method.

- [ ] **Step 3: Remove `DiagnosticSummary::has_issues()` from `peri-lsp/src/diagnostics.rs`**

Delete the method.

- [ ] **Step 4: Remove `DiagnosticStore::clear_for_file()` from `peri-lsp/src/diagnostics.rs`**

Delete the method.

- [ ] **Step 5: Remove `LspClient::did_close()` from `peri-lsp/src/client.rs`**

Delete the method.

- [ ] **Step 6: Remove `did_close_notification()` from `peri-lsp/src/protocol/notifications.rs`**

Delete the function.

- [ ] **Step 7: Remove `#[from]` from `LspError::Io` and `LspError::Json` in `peri-lsp/src/error.rs`**

Change:
```rust
#[error("IO error: {0}")]
Io(#[from] std::io::Error),
```
To:
```rust
#[error("IO error: {0}")]
Io(std::io::Error),
```

Change:
```rust
#[error("JSON error: {0}")]
Json(#[from] serde_json::Error),
```
To:
```rust
#[error("JSON error: {0}")]
Json(serde_json::Error),
```

- [ ] **Step 8: Build to verify**

Run: `cargo build --all`
Expected: SUCCESS

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "chore: remove dead functions (spawn_shell, ingest_native, has_issues, clear_for_file, did_close)
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 8: Consolidate `StopReason::from_anthropic` → `from_display`

**Files:**
- Modify: `peri-agent/src/llm/types.rs` (remove `from_anthropic`)
- Modify: `peri-agent/src/llm/anthropic/invoke.rs` (change call)
- Modify: `peri-agent/src/llm/anthropic/stream.rs` (change call)
- Modify: `peri-agent/src/llm/types_test.rs` (update tests if needed)

- [ ] **Step 1: Remove `from_anthropic()` from `peri-agent/src/llm/types.rs`**

Delete the method:
```rust
pub fn from_anthropic(s: &str) -> Self {
    match s {
        "end_turn" => Self::EndTurn,
        "tool_use" => Self::ToolUse,
        "max_tokens" => Self::MaxTokens,
        other => Self::Other(other.to_string()),
    }
}
```

- [ ] **Step 2: Update call in `peri-agent/src/llm/anthropic/invoke.rs`**

Change:
```rust
StopReason::from_anthropic(...)
```
To:
```rust
StopReason::from_display(...)
```

- [ ] **Step 3: Update call in `peri-agent/src/llm/anthropic/stream.rs`**

Change:
```rust
StopReason::from_anthropic(...)
```
To:
```rust
StopReason::from_display(...)
```

- [ ] **Step 4: Update test in `peri-agent/src/llm/types_test.rs`**

If any test references `from_anthropic`, change it to `from_display`.

- [ ] **Step 5: Build to verify**

Run: `cargo build -p peri-agent`
Expected: SUCCESS

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: consolidate StopReason::from_anthropic into from_display
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 9: Remove dead `#![allow]` blocks

**Files:**
- Modify: `peri-tui/src/lib.rs:3-12`
- Modify: `peri-middlewares/src/lib.rs:7-11`

- [ ] **Step 1: Clean up peri-tui `#![allow]` block**

In `peri-tui/src/lib.rs`, the allow block currently suppresses 8 lints. Verified: only `collapsible_else_if` (1 warning) is live. Replace the block with only the live lint:

```rust
#![allow(clippy::collapsible_else_if)]
```

- [ ] **Step 2: Remove peri-middlewares `#![allow]` block entirely**

In `peri-middlewares/src/lib.rs`, all 3 lints (`type_complexity`, `empty_line_after_doc_comments`, `useless_conversion`) produce zero warnings. Delete the entire block:
```rust
#![allow(
    clippy::type_complexity,
    clippy::empty_line_after_doc_comments,
    clippy::useless_conversion
)]
```

- [ ] **Step 3: Build to verify**

Run: `cargo build --all`
Expected: SUCCESS

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: remove dead clippy allow directives (10 lints, only collapsible_else_if retained)
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 10: Clean up Cargo.toml dependencies

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: `peri-agent/Cargo.toml`
- Modify: `peri-acp/Cargo.toml`
- Modify: `peri-middlewares/Cargo.toml`
- Modify: `langfuse-client/Cargo.toml`
- Modify: `peri-lsp/Cargo.toml`
- Modify: `peri-widgets/Cargo.toml`
- Modify: `peri-tui/Cargo.toml`

- [ ] **Step 1: Remove `tokio-test` from workspace Cargo.toml**

In `Cargo.toml`, delete the line:
```
tokio-test = "0.4"
```

- [ ] **Step 2: Remove `tokio-test` from all 5 crate Cargo.tomls**

In each of these files, delete `tokio-test.workspace = true`:
- `peri-agent/Cargo.toml` (in `[dev-dependencies]`)
- `peri-acp/Cargo.toml` (in `[dev-dependencies]`)
- `peri-middlewares/Cargo.toml` (in `[dev-dependencies]`)
- `langfuse-client/Cargo.toml` (in `[dev-dependencies]`)
- `peri-lsp/Cargo.toml` (in `[dev-dependencies]`)

Note: `#[tokio::test]` comes from `tokio` with `features = ["full"]` which includes `macros`. No code imports `tokio_test::`.

- [ ] **Step 3: Remove `"blocking"` from reqwest features in `peri-agent/Cargo.toml`**

Change:
```toml
reqwest = { workspace = true, features = ["stream", "blocking"] }
```
To:
```toml
reqwest = { workspace = true, features = ["stream"] }
```

- [ ] **Step 4: Fix `lru` version in `peri-widgets/Cargo.toml`**

Change:
```toml
lru = "0.12"
```
To:
```toml
lru.workspace = true
```

- [ ] **Step 5: Fix `thiserror` version in `peri-tui/Cargo.toml`**

Change:
```toml
thiserror = "1"
```
To:
```toml
thiserror.workspace = true
```

- [ ] **Step 6: Build to verify**

Run: `cargo build --all`
Expected: SUCCESS

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "chore: remove tokio-test dependency, fix lru/thiserror versions, remove reqwest blocking
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 11: Run tests and verify clean state

**Files:** None (verification only)

- [ ] **Step 1: Run full build**

Run: `cargo build --all`
Expected: SUCCESS, 0 errors

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets -- -W clippy::all 2>&1 | grep "warning:" | grep -v "generated"`
Expected: Only `result_large_err` + `manual_async_fn` + `collapsible_else_if` warnings remain (pre-existing, not from this batch)

- [ ] **Step 3: Run tests**

Run: `cargo test --all 2>&1 | tail -30`
Expected: All tests pass

- [ ] **Step 4: Verify git status**

Run: `git status`
Expected: `working tree clean`

---

## Self-Review Checklist

### Spec coverage
- [x] All 71 verified findings from workflow scan addressed or scoped out
- [x] `alloc_collect`: verified empty body + 3 call sites → removing all
- [x] `tokio-test`: verified no `tokio_test::` imports → safe to remove
- [x] `StopReason::from_anthropic`: verified identical to `from_display` → consolidating
- [x] `#![allow]`: verified lint-by-lint with clippy → only `collapsible_else_if` retained
- [x] `agent_rx`: verified always `None` → removing entire legacy path
- [x] 10 dead enum variants: verified zero construction sites (production code) → removing

### Placeholder scan
- [x] No TBD/TODO/implement later
- [x] No "add error handling" / "handle edge cases"
- [x] All code changes show exact transformations
- [x] All commands include expected output

### Type consistency
- [x] `from_display` signature matches `from_anthropic` → drop-in replacement
- [x] `agent_rx` removal preserves `mpsc::Receiver<AgentEvent>` import (used by `bg_event_rx`)
- [x] All enum variant removals: no exhaustive match outside crate → safe

### Scoped out (future batches)
- Dead re-exports cleanup (9 findings, LOW priority)
- Duplicated function consolidation (4 findings, needs design decisions)
- Shared string constants (3 findings, cross-crate coordination)
- Clippy pedantic incremental adoption (3948 auto-fixable, separate effort)
