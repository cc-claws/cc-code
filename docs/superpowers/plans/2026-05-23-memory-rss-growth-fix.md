# Memory RSS Growth Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce per-turn RSS growth from ~40 MB to <15 MB by fixing jemalloc `background_thread` setup failure, limiting reqwest connection pool retention, and reducing tokio thread stack overhead.

**Architecture:** Three independent fixes targeting the two root causes identified in issue 现象 6: (1) jemalloc arena fragmentation (~17 MB/turn) caused by `background_thread` not activating, and (2) non-jemalloc runtime overhead (~20 MB/turn) from unbounded reqwest connection pools and tokio thread stacks. Each fix is self-contained and testable independently.

**Tech Stack:** Rust, tikv-jemalloc-ctl, reqwest, tokio

**Issue reference:** `spec/issues/2026-05-22-memory-linear-growth-no-compact.md` (现象 6)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `peri-tui/src/main.rs` | Modify | Move jemalloc config to `MALLOC_CONF` env var before any allocation |
| `peri-tui/src/jemalloc_config.rs` | Modify | Replace `raw::write` with `MALLOC_CONF` env var + keep `raw::write` as fallback |
| `peri-tui/src/jemalloc_config_test.rs` | Modify | Update tests for new approach |
| `peri-agent/src/llm/openai/mod.rs` | Modify | Accept external `reqwest::Client` + pool-limited builder |
| `peri-agent/src/llm/anthropic/mod.rs` | Modify | Accept external `reqwest::Client` + pool-limited builder |
| `peri-acp/src/provider/mod.rs` | Modify | Create shared pool-limited `reqwest::Client`, pass to LLM constructors |
| `peri-tui/src/main.rs` (tokio runtime) | Modify | Add `thread_stack_size(4 << 20)` to all runtime builders |

---

## Task 1: Fix jemalloc `background_thread` via `MALLOC_CONF` Environment Variable

**Why:** Heapdump confirms `background_thread: false` despite `raw::write` call. The `raw::write("background_thread", true)` fails silently when arenas already exist (after tokio runtime spawns threads). The `MALLOC_CONF` env var is read by jemalloc during init, before any arena allocation — this is the only reliable way to enable `background_thread`.

**Files:**
- Modify: `peri-tui/src/main.rs:1-10`
- Modify: `peri-tui/src/jemalloc_config.rs`
- Test: `peri-tui/src/jemalloc_config_test.rs`

- [ ] **Step 1: Update `jemalloc_config.rs` — add `init_malloc_conf()` function**

Add a new function that sets `MALLOC_CONF` env var before jemalloc initializes. Keep existing `configure_jemalloc()` as a runtime fallback for the settings it can still apply (epoch refresh, diagnostic logging).

```rust
//! jemalloc allocator tuning for high-churn workloads.
//!
//! Two-phase configuration:
//! 1. `init_malloc_conf()` — sets `MALLOC_CONF` env var BEFORE jemalloc init.
//!    This is the only reliable way to enable `background_thread`.
//! 2. `configure_jemalloc()` — runtime mallctl writes as fallback/diagnostics.
//!
//! Must be called in `main()` before any significant allocation.

/// Set `MALLOC_CONF` environment variable before jemalloc initializes.
///
/// jemalloc reads `MALLOC_CONF` once at process startup (during the first
/// allocation). Setting it via `env::set_var` before the `#[global_allocator]`
/// triggers init ensures the settings take effect. This is the only reliable
/// way to enable `background_thread` — runtime `raw::write` fails silently
/// once arenas have been created.
///
/// Call this at the very first line of `main()`.
// Clippy: dead_code in lib targets; used by bin target main.rs.
#[allow(dead_code)]
pub fn init_malloc_conf() {
    // Only set if not already configured by the user externally.
    if std::env::var("MALLOC_CONF").is_ok() {
        return;
    }
    // dirty_decay_ms:200 — purge freed arena pages after 200ms (default: 10000ms)
    // background_thread:true — enable background purge thread (default: disabled)
    // lg_tcache_max:16 — limit thread cache to objects ≤64KB (default: unlimited)
    std::env::set_var(
        "MALLOC_CONF",
        "dirty_decay_ms:200,background_thread:true,lg_tcache_max:16",
    );
}

/// Configure jemalloc for aggressive memory reclamation via runtime mallctl.
///
/// This is a best-effort fallback that applies settings at runtime.
/// `background_thread` may not take effect if arenas already exist;
/// use `init_malloc_conf()` for reliable configuration.
// Called from main.rs (bin target) via peri_tui::jemalloc_config::configure_jemalloc().
// Clippy's dead_code lint fires on lib targets even when used by the bin target.
#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
pub fn configure_jemalloc() {
    use tracing::{debug, warn};

    // Advance epoch to ensure stats are fresh
    let _ = tikv_jemalloc_ctl::epoch::advance();

    // 1. dirty_decay_ms — time before freed dirty pages are purged
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.dirty_decay_ms\0", 200i64) } {
        Ok(()) => debug!("jemalloc: arenas.dirty_decay_ms = 200"),
        Err(e) => warn!("jemalloc: failed to set dirty_decay_ms: {}", e),
    }

    // 2. background_thread — enables a background thread per arena that
    //    proactively purges dirty pages.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"background_thread\0", true) } {
        Ok(()) => debug!("jemalloc: background_thread = true"),
        Err(e) => warn!("jemalloc: failed to enable background_thread: {}", e),
    }

    // 3. lg_tcache_max — log2 of max cached allocation size in thread caches.
    match unsafe { tikv_jemalloc_ctl::raw::write(b"arenas.lg_tcache_max\0", 16usize) } {
        Ok(()) => debug!("jemalloc: arenas.lg_tcache_max = 16 (64KB)"),
        Err(e) => warn!("jemalloc: failed to set lg_tcache_max: {}", e),
    }
}

#[cfg(target_os = "windows")]
pub fn configure_jemalloc() {
    // jemalloc not used on Windows (system allocator instead)
}

#[cfg(target_os = "windows")]
pub fn init_malloc_conf() {
    // No-op on Windows (system allocator)
}
```

- [ ] **Step 2: Update `main.rs` — call `init_malloc_conf()` as the very first line**

The `init_malloc_conf()` must run before the `#[global_allocator]` triggers jemalloc init (which happens on the first allocation). Move it to line 1 of `main()`.

In `peri-tui/src/main.rs`, add the call before `configure_jemalloc()`:

```rust
fn main() -> Result<()> {
    // Set MALLOC_CONF env var BEFORE jemalloc initializes.
    // This is the first thing we do — before any allocation, tracing, or parsing.
    // jemalloc reads this env var during its one-time init (triggered by the
    // first allocation through the #[global_allocator]).
    peri_tui::jemalloc_config::init_malloc_conf();

    // Runtime mallctl writes as fallback/diagnostics (may not fully take effect
    // if arenas already exist, but harmless to call).
    peri_tui::jemalloc_config::configure_jemalloc();

    // ... rest of main unchanged ...
```

- [ ] **Step 3: Update tests**

In the test file `peri-tui/src/jemalloc_config_test.rs` (or inline `#[cfg(test)]` module), add a test for `init_malloc_conf`:

```rust
#[test]
fn test_init_malloc_conf_sets_env() {
    // Clear any existing MALLOC_CONF to test fresh behavior
    std::env::remove_var("MALLOC_CONF");
    init_malloc_conf();
    let val = std::env::var("MALLOC_CONF").expect("MALLOC_CONF should be set");
    assert!(
        val.contains("background_thread:true"),
        "MALLOC_CONF should contain background_thread:true, got: {}",
        val
    );
    assert!(
        val.contains("dirty_decay_ms:200"),
        "MALLOC_CONF should contain dirty_decay_ms:200, got: {}",
        val
    );
    assert!(
        val.contains("lg_tcache_max:16"),
        "MALLOC_CONF should contain lg_tcache_max:16, got: {}",
        val
    );
}

#[test]
fn test_init_malloc_conf_respects_existing() {
    std::env::set_var("MALLOC_CONF", "custom:true");
    init_malloc_conf();
    let val = std::env::var("MALLOC_CONF").expect("MALLOC_CONF should be set");
    assert_eq!(val, "custom:true", "Should not overwrite user-set MALLOC_CONF");
    std::env::remove_var("MALLOC_CONF");
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build -p peri-tui && cargo test -p peri-tui --lib -- jemalloc_config`
Expected: All tests pass, no warnings.

- [ ] **Step 5: Manual verification — start TUI, run `/heapdump`, confirm `background_thread: true`**

Run: `cargo run -p peri-tui`, then type `/heapdump` and check the output file in `.tmp/`.
Expected: `background_thread: true` in the dump.

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/jemalloc_config.rs peri-tui/src/main.rs
git commit -m "fix(tui): set MALLOC_CONF env var before jemalloc init to enable background_thread

background_thread: true was not taking effect via runtime mallctl write
because arenas were already created by tokio threads. Setting MALLOC_CONF
env var before the first allocation ensures jemalloc reads it during init.

Refs: spec/issues/2026-05-22-memory-linear-growth-no-compact.md (现象 6)"
```

---

## Task 2: Limit reqwest Connection Pool Retention

**Why:** `reqwest::Client::new()` creates a connection pool with default `pool_max_idle_per_host: usize::MAX` and no idle timeout. Each LLM HTTP connection holds a TLS session (~50-100 KB). After N turns with streaming responses, the pool accumulates connections whose TLS buffers are never released. Setting `pool_max_idle_per_host(1)` + `pool_idle_timeout(30s)` limits retention to 1 connection per host with 30s idle eviction.

**Files:**
- Modify: `peri-agent/src/llm/openai/mod.rs`
- Modify: `peri-agent/src/llm/anthropic/mod.rs`

- [ ] **Step 1: Create a shared pool-limited `reqwest::Client` builder helper**

In `peri-agent/src/llm/openai/mod.rs`, update `ChatOpenAI::new()` to use a pool-limited client:

```rust
/// Build a reqwest client with connection pool limits to prevent TLS session
/// accumulation. Default reqwest pool is unbounded — each idle connection
/// holds ~50-100 KB of TLS state that is never released.
fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

impl ChatOpenAI {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".to_string(),
            reasoning_effort: None,
            thinking_enabled: false,
            supports_thinking_content: Self::detect_thinking_content_support(&model),
            max_tokens: 32000,
            model,
            client: build_reqwest_client(),
        }
    }
```

- [ ] **Step 2: Apply same change to `ChatAnthropic`**

In `peri-agent/src/llm/anthropic/mod.rs`:

```rust
fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

impl ChatAnthropic {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            extended_thinking: false,
            thinking_budget: 10000,
            thinking_effort: "medium".to_string(),
            enable_cache: true,
            base_url: None,
            max_tokens: 32000,
            client: build_reqwest_client(),
        }
    }
```

- [ ] **Step 3: Build and test**

Run: `cargo build -p peri-agent && cargo test -p peri-agent`
Expected: Build succeeds, all existing tests pass.

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/openai/mod.rs peri-agent/src/llm/anthropic/mod.rs
git commit -m "perf(agent): limit reqwest connection pool to prevent TLS session accumulation

Default reqwest pool_max_idle_per_host is usize::MAX with no idle timeout.
Each idle connection holds ~50-100KB of TLS state. Limiting to 1 connection
per host with 30s idle timeout prevents unbounded TLS buffer retention.

Refs: spec/issues/2026-05-22-memory-linear-growth-no-compact.md (现象 6)"
```

---

## Task 3: Reduce Tokio Thread Stack Size

**Why:** Tokio's default worker thread stack size is 8 MB. With `num_cpus` workers (e.g., 8 on a MacBook Pro), that's 64 MB of pure stack allocation that RSS counts but never uses (our async tasks don't need deep recursion). Reducing to 4 MB per thread is safe for all our async workloads and saves ~32 MB on an 8-core machine.

**Files:**
- Modify: `peri-tui/src/main.rs` (all 6 runtime builder sites)

- [ ] **Step 1: Add `thread_stack_size` to all tokio runtime builders**

In `peri-tui/src/main.rs`, update all `tokio::runtime::Builder::new_multi_thread()` calls. There are 6 sites (lines ~265, ~304, ~306, ~310, ~325, ~341, ~396).

For each site, change:
```rust
let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
```
to:
```rust
let rt = tokio::runtime::Builder::new_multi_thread()
    .thread_stack_size(4 * 1024 * 1024) // 4 MB (default: 8 MB)
    .enable_all()
    .build()?;
```

- [ ] **Step 2: Build and run**

Run: `cargo build -p peri-tui`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "perf(tui): reduce tokio worker thread stack from 8MB to 4MB

Default 8MB × num_cpus threads = 64MB on 8-core. Our async workloads
don't need deep stacks. 4MB is sufficient and saves ~32MB RSS.

Refs: spec/issues/2026-05-22-memory-linear-growth-no-compact.md (现象 6)"
```

---

## Task 4: Verification — Memory Comparison

**Why:** Need to confirm the three fixes together produce measurable RSS improvement.

- [ ] **Step 1: Build release binary**

Run: `cargo build -p peri-tui --release`

- [ ] **Step 2: Baseline measurement**

1. Start TUI: `./target/release/peri-tui`
2. Immediately run `/heapdump` → save as baseline
3. Run 5 turns of conversation
4. Run `/heapdump` → save as "after 5 turns"
5. Run `/clear`
6. Run `/heapdump` → save as "after clear"

- [ ] **Step 3: Compare with pre-fix heapdump**

Compare the three dumps with the pre-fix data from 现象 6. Expected improvements:

| Metric | Pre-fix (现象 6) | Expected Post-fix |
|--------|------------------|-------------------|
| `background_thread` | false | true |
| RSS after /clear | 81.8 MB | <50 MB |
| arena active - allocated | 17.5 MB | <8 MB |
| non_arena (mapped-active) | 89.4 MB | <50 MB |

- [ ] **Step 4: Update issue with results**

Append results to `spec/issues/2026-05-22-memory-linear-growth-no-compact.md` as 现象 7.

- [ ] **Step 5: Final commit with issue update**

```bash
git add spec/issues/2026-05-22-memory-linear-growth-no-compact.md
git commit -m "docs(issue): update memory issue with post-fix verification results"
```
