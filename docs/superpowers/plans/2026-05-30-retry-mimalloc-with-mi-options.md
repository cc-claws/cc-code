# Re-introduce mimalloc with MI_OPTION Tuning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the system default allocator with mimalloc, configured with three MI_OPTION env vars (PAGE_RESET, DECOMMIT, BACKGROUND_THREAD) to reduce long-session RSS growth.

**Architecture:** Add `mimalloc` as `#[global_allocator]` in `main.rs`, set MI_OPTION env vars before first allocation via an `init_mimalloc_conf()` function, and restore `alloc_collect()` using `mi_collect(true)` for post-compact/clear memory reclamation.

**Tech Stack:** Rust, mimalloc crate, libmimalloc-sys

**Issue reference:** `spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add mimalloc to workspace dependencies |
| `peri-tui/Cargo.toml` | Modify | Add mimalloc crate dependency (non-Windows) |
| `peri-tui/src/main.rs` | Modify | Add `#[global_allocator]` + call `init_mimalloc_conf()` |
| `peri-tui/src/lib.rs` | Modify | Add `pub mod mimalloc_config;` module declaration |
| `peri-tui/src/mimalloc_config.rs` | Create | `init_mimalloc_conf()` + `alloc_collect()` functions |
| `peri-tui/src/mimalloc_config_test.rs` | Create | Unit tests for config + collect |
| `peri-tui/src/app/thread_ops.rs` | Modify | Call `alloc_collect()` after clear in `new_thread()` and `open_thread()` |

---

### Task 1: Add mimalloc workspace and crate dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `peri-tui/Cargo.toml`

- [ ] **Step 1: Add mimalloc to workspace dependencies**

In `Cargo.toml`, append to the `[workspace.dependencies]` section (after the `tempfile` line):

```toml
# --- Allocator ---
mimalloc = "0.1"
```

- [ ] **Step 2: Add mimalloc to peri-tui crate dependencies**

In `peri-tui/Cargo.toml`, add a platform-specific dependency section at the end of the file:

```toml
[target.'cfg(not(target_os = "windows"))'.dependencies]
mimalloc.workspace = true
```

- [ ] **Step 3: Build to verify dependency resolution**

Run: `cargo check -p peri-tui`
Expected: Build succeeds, mimalloc and libmimalloc-sys are fetched.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml peri-tui/Cargo.toml
git commit -m "chore: add mimalloc workspace dependency for peri-tui

Re-introducing mimalloc with MI_OPTION tuning to address long-session
RSS growth. Non-Windows only (system allocator on Windows).

Refs: spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md"
```

---

### Task 2: Create mimalloc_config module

**Files:**
- Create: `peri-tui/src/mimalloc_config.rs`
- Create: `peri-tui/src/mimalloc_config_test.rs`
- Modify: `peri-tui/src/lib.rs`

- [ ] **Step 1: Create `mimalloc_config.rs`**

Create `peri-tui/src/mimalloc_config.rs`:

```rust
//! mimalloc allocator tuning for high-churn workloads.
//!
//! Two functions:
//! 1. `init_mimalloc_conf()` — sets MI_OPTION env vars BEFORE mimalloc init.
//!    Must be called at the very first line of `main()`.
//! 2. `alloc_collect()` — triggers `mi_collect(true)` to force memory reclamation.
//!    Called after `/clear`, `/compact`, and session switches.

/// Set mimalloc environment variables before the allocator initializes.
///
/// mimalloc reads these env vars during its one-time init (triggered by
/// the first allocation through `#[global_allocator]`). Must be called
/// before any significant allocation — ideally line 1 of `main()`.
///
/// Options configured:
/// - `MIMALLOC_PAGE_RESET=1` — reset freed pages immediately (more aggressive than default)
/// - `MIMALLOC_DECOMMIT=1` — decommit (return to OS) freed virtual address space
/// - `MIMALLOC_BACKGROUND_THREAD=1` — enable background thread for memory reclamation
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    // Only set if not already configured by the user externally.
    if std::env::var("MIMALLOC_PAGE_RESET").is_err() {
        std::env::set_var("MIMALLOC_PAGE_RESET", "1");
    }
    if std::env::var("MIMALLOC_DECOMMIT").is_err() {
        std::env::set_var("MIMALLOC_DECOMMIT", "1");
    }
    if std::env::var("MIMALLOC_BACKGROUND_THREAD").is_err() {
        std::env::set_var("MIMALLOC_BACKGROUND_THREAD", "1");
    }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn init_mimalloc_conf() {
    // No-op on Windows (system allocator used instead)
}

/// Force mimalloc to reclaim freed memory and return it to the OS.
///
/// Call after `/clear`, `/compact`, or session switches where large
/// amounts of memory have been freed.
#[cfg(not(target_os = "windows"))]
pub fn alloc_collect() {
    unsafe {
        libmimalloc_sys::mi_collect(true);
    }
}

#[cfg(target_os = "windows")]
pub fn alloc_collect() {
    // No-op on Windows (system allocator used instead)
}
```

- [ ] **Step 2: Create `mimalloc_config_test.rs`**

Create `peri-tui/src/mimalloc_config_test.rs`:

```rust
use super::*;

#[test]
fn test_init_mimalloc_conf_sets_env_vars() {
    // Clear any existing vars to test fresh behavior
    std::env::remove_var("MIMALLOC_PAGE_RESET");
    std::env::remove_var("MIMALLOC_DECOMMIT");
    std::env::remove_var("MIMALLOC_BACKGROUND_THREAD");

    init_mimalloc_conf();

    assert_eq!(
        std::env::var("MIMALLOC_PAGE_RESET").unwrap(),
        "1",
        "MIMALLOC_PAGE_RESET should be set to 1"
    );
    assert_eq!(
        std::env::var("MIMALLOC_DECOMMIT").unwrap(),
        "1",
        "MIMALLOC_DECOMMIT should be set to 1"
    );
    assert_eq!(
        std::env::var("MIMALLOC_BACKGROUND_THREAD").unwrap(),
        "1",
        "MIMALLOC_BACKGROUND_THREAD should be set to 1"
    );

    // Cleanup
    std::env::remove_var("MIMALLOC_PAGE_RESET");
    std::env::remove_var("MIMALLOC_DECOMMIT");
    std::env::remove_var("MIMALLOC_BACKGROUND_THREAD");
}

#[test]
fn test_init_mimalloc_conf_respects_existing() {
    std::env::set_var("MIMALLOC_PAGE_RESET", "0");
    std::env::set_var("MIMALLOC_DECOMMIT", "0");
    std::env::set_var("MIMALLOC_BACKGROUND_THREAD", "0");

    init_mimalloc_conf();

    // Should NOT overwrite user-set values
    assert_eq!(
        std::env::var("MIMALLOC_PAGE_RESET").unwrap(),
        "0",
        "Should not overwrite user-set MIMALLOC_PAGE_RESET"
    );
    assert_eq!(
        std::env::var("MIMALLOC_DECOMMIT").unwrap(),
        "0",
        "Should not overwrite user-set MIMALLOC_DECOMMIT"
    );
    assert_eq!(
        std::env::var("MIMALLOC_BACKGROUND_THREAD").unwrap(),
        "0",
        "Should not overwrite user-set MIMALLOC_BACKGROUND_THREAD"
    );

    // Cleanup
    std::env::remove_var("MIMALLOC_PAGE_RESET");
    std::env::remove_var("MIMALLOC_DECOMMIT");
    std::env::remove_var("MIMALLOC_BACKGROUND_THREAD");
}

#[test]
fn test_alloc_collect_does_not_panic() {
    // alloc_collect should be safe to call, even multiple times
    alloc_collect();
    alloc_collect();
    alloc_collect();
}
```

- [ ] **Step 3: Add module declaration to `lib.rs`**

In `peri-tui/src/lib.rs`, add `mimalloc_config` after the existing module declarations:

```rust
pub mod mimalloc_config;
```

- [ ] **Step 4: Build and test**

Run: `cargo build -p peri-tui && cargo test -p peri-tui --lib -- mimalloc_config`
Expected: Build succeeds, 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/mimalloc_config.rs peri-tui/src/mimalloc_config_test.rs peri-tui/src/lib.rs
git commit -m "feat(tui): add mimalloc_config module with MI_OPTION tuning and alloc_collect

init_mimalloc_conf() sets PAGE_RESET/DECOMMIT/BACKGROUND_THREAD env vars
before first allocation. alloc_collect() wraps mi_collect(true) for
post-clear/compact memory reclamation.

Refs: spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md"
```

---

### Task 3: Register mimalloc as global allocator and call init

**Files:**
- Modify: `peri-tui/src/main.rs`

- [ ] **Step 1: Add `#[global_allocator]` and call `init_mimalloc_conf()`**

In `peri-tui/src/main.rs`, add the global allocator declaration and the init call. The `init_mimalloc_conf()` must be the **very first thing** in `main()` — before any allocation.

Add at the top of the file (after the existing `use` statements, before `mod acp_stdio;`):

```rust
#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

Then find the `main()` function. Add `init_mimalloc_conf()` as the first line of `main()`. The function signature `fn main() -> Result<()>` stays unchanged. Find the existing first line inside `main()` and prepend the init call:

```rust
fn main() -> Result<()> {
    // Set mimalloc env vars BEFORE any allocation.
    // Must be the very first line — mimalloc reads these during init.
    peri_tui::mimalloc_config::init_mimalloc_conf();

    // ... rest of main unchanged ...
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p peri-tui`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "feat(tui): register mimalloc as global allocator with MI_OPTION init

init_mimalloc_conf() called as first line of main() to set PAGE_RESET,
DECOMMIT, and BACKGROUND_THREAD before mimalloc initializes.

Refs: spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md"
```

---

### Task 4: Add `alloc_collect()` calls after session clear/switch

**Files:**
- Modify: `peri-tui/src/app/thread_ops.rs`

- [ ] **Step 1: Add `alloc_collect()` call in `new_thread()`**

In `peri-tui/src/app/thread_ops.rs`, in the `new_thread()` method, after the ACP `new_session` call completes and before the final `render_tx.send` block, add the `alloc_collect()` call.

Find the closing brace of this block (approximately line 346):

```rust
                })
            });
        }
```

After it, before the `let _ = self.session_mgr...` render event, add:

```rust
        // 回收释放的内存给 OS
        peri_tui::mimalloc_config::alloc_collect();
```

- [ ] **Step 2: Add `alloc_collect()` call in `open_thread()`**

In the same file, in the `open_thread()` method, after `self.reset_agent_session();` (approximately line 211), add:

```rust
        // 回收释放的内存给 OS
        peri_tui::mimalloc_config::alloc_collect();
```

- [ ] **Step 3: Build and test**

Run: `cargo build -p peri-tui && cargo test -p peri-tui --lib`
Expected: Build succeeds, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/thread_ops.rs
git commit -m "feat(tui): add alloc_collect() after session clear and thread switch

Call mi_collect(true) after new_thread() and open_thread() to force
mimalloc to reclaim freed memory and return it to the OS.

Refs: spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md"
```

---

### Task 5: Full workspace build and smoke test

**Files:** None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build`
Expected: Build succeeds, no warnings about mimalloc.

- [ ] **Step 2: Run all tests**

Run: `cargo test -p peri-tui --lib`
Expected: All tests pass (including the 3 new mimalloc_config tests).

- [ ] **Step 3: Manual smoke test — start TUI and observe RSS**

1. Start TUI: `cargo run -p peri-tui`
2. Send a few messages
3. Observe RSS growth behavior over 5-10 turns
4. Run `/clear` and observe whether RSS drops

---

## Self-Review Checklist

### 1. Spec Coverage

| Spec Requirement | Task |
|---|---|
| Add mimalloc as global allocator | Task 3 (`#[global_allocator]`) |
| Configure MI_OPTION env vars (PAGE_RESET, DECOMMIT, BACKGROUND_THREAD) | Task 2 (`init_mimalloc_conf()`) |
| Restore alloc_collect() with mi_collect(true) | Task 2 (`alloc_collect()`) |
| Call alloc_collect() after session clear/switch | Task 4 (thread_ops.rs) |
| Minimal introduction — no /heapdump restoration | ✅ No heapdump code included |

### 2. Placeholder Scan

No TBD/TODO/fill-in-later patterns found. All code blocks contain complete implementations.

### 3. Type Consistency

- `mimalloc::MiMalloc` is the standard type from the `mimalloc` crate — matches `#[global_allocator]` requirements (implements `GlobalAlloc`)
- `libmimalloc_sys::mi_collect(true)` — `bool` parameter for force collection, matches mimalloc C API
- `peri_tui::mimalloc_config::init_mimalloc_conf()` and `alloc_collect()` — module path matches `lib.rs` declaration and `main.rs` usage
- Platform gates `#[cfg(not(target_os = "windows"))]` consistently applied across global_allocator, config module, and Cargo.toml
