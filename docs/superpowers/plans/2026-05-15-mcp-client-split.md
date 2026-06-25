# MCP Client Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `peri-middlewares/src/mcp/client.rs` (1309 lines) into 4 focused files: `client.rs` (struct + core methods), `initialize.rs` (connection bootstrap), `reconnect.rs` (reconnection), `client_oauth.rs` (McpClientPool OAuth methods).

**Architecture:** Pure mechanical extraction — no logic changes. `client.rs` retains type definitions and shared pub(crate) helpers. Three sibling modules add `impl McpClientPool` blocks for specific lifecycle phases. All external callers use `peri_middlewares::mcp::McpClientPool` — zero API breakage.

**Tech Stack:** Rust, `rmcp` crate, `parking_lot`, `tokio`, `thiserror`.

---

## Problem Summary

`client.rs` has 4 distinct responsibility zones mixed in one file:

| Zone | Lines | Method/Function |
|------|-------|-----------------|
| Connection bootstrap | 130–358 | `run_initialize()` |
| Connection bootstrap (alt) | 904–1081 | `initialize()` |
| Reconnection | 411–589 | `reconnect()` |
| OAuth flow | 591–734 | `start_oauth_flow()`, `clear_oauth()` |
| Core + helpers | 1–129, 360–409, 736–902, 1084–1188 | structs, simple methods, transport builders, tests |

**Shared helpers** (used by `run_initialize`, `initialize`, and `reconnect`):
- `insert_failed()` (line 360)
- `insert_needs_auth()` (line 383)
- `is_auth_required_error()` (line 407)
- `spawn_stdio_transport()` (line 1084)
- `build_http_transport()` (line 1123)
- `build_authed_transport()` (line 1151)
- `STDIO_CONNECT_TIMEOUT` / `HTTP_CONNECT_TIMEOUT` / `SHUTDOWN_TIMEOUT` constants

**Decision:** All helpers stay in `client.rs` as `pub(crate)`. The split modules import them via `super::client::*`.

---

## File Structure After Split

```
mcp/
├── mod.rs               # adds pub mod initialize, reconnect, client_oauth
├── client.rs            # ~620 lines: structs + core methods + pub(crate) helpers + tests
├── initialize.rs        # ~250 lines: run_initialize() + initialize()
├── reconnect.rs         # ~180 lines: reconnect()
├── client_oauth.rs      # ~150 lines: start_oauth_flow() + clear_oauth()
├── config.rs            # unchanged
├── oauth_flow.rs        # unchanged (different concern: OAuth state machine)
├── ...                  # unchanged
```

**Naming note:** `client_oauth.rs` avoids collision with existing `oauth_flow.rs`.

---

### Task 1: Create `initialize.rs` — extract `run_initialize()` and `initialize()`

**Files:**
- Create: `peri-middlewares/src/mcp/initialize.rs`
- Modify: `peri-middlewares/src/mcp/client.rs` (remove lines 130–358, 904–1081)
- Modify: `peri-middlewares/src/mcp/mod.rs` (add `pub mod initialize;`)

- [ ] **Step 1: Create `initialize.rs` with full content**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/mcp/initialize.rs`:

```rust
use std::path::Path;
use std::sync::Arc;

use super::auth_store::FileCredentialStore;
use super::client::{
    ClientStatus, McpClientHandle, McpClientPool, McpInitStatus, OAuthStatus,
    HTTP_CONNECT_TIMEOUT, STDIO_CONNECT_TIMEOUT,
    build_authed_transport, build_http_transport, insert_failed, insert_needs_auth,
    is_auth_required_error, spawn_stdio_transport,
};
use super::config::{OAuthConfig, McpServerConfig};
use super::oauth_flow::{OAuthFlowEvent, OAuthFlowManager};
use super::transport::TransportConfig;

impl McpClientPool {
    pub async fn run_initialize(
        pool: Arc<Self>,
        cwd: &Path,
        claude_home: &Path,
        status_tx: tokio::sync::watch::Sender<McpInitStatus>,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) {
        // [COPY lines 137–358 from client.rs verbatim]
        // NOTE: Self::insert_failed → insert_failed, Self::insert_needs_auth → insert_needs_auth,
        //       Self::is_auth_required_error → is_auth_required_error
        //       spawn_stdio_transport, build_http_transport, build_authed_transport are direct calls
    }

    pub async fn initialize(
        cwd: &Path,
        claude_home: &Path,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) -> Self {
        // [COPY lines 909–1081 from client.rs verbatim]
        // Same import adjustments as run_initialize
    }
}
```

To produce the final file, copy the exact bodies of `run_initialize` (lines 130–358) and `initialize` (lines 904–1081) from `client.rs`. Replace:
- `Self::insert_failed(&pool, ...)` → `insert_failed(&pool, ...)`
- `Self::insert_needs_auth(&pool, ...)` → `insert_needs_auth(&pool, ...)`
- `Self::is_auth_required_error(...)` → `is_auth_required_error(...)`

- [ ] **Step 2: Add `pub(crate)` visibility to shared helpers in `client.rs`**

In `client.rs`, change these declarations:

```rust
// Line 105: change these constants
pub(crate) const STDIO_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub(crate) const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub(crate) const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

// Line 360: change
pub(crate) fn insert_failed(pool: &Arc<Self>, name: &str, reason: String) { ... }

// Line 383: change
pub(crate) fn insert_needs_auth(pool: &Arc<Self>, name: &str, reason: String) { ... }

// Line 407: change
pub(crate) fn is_auth_required_error(error: &str, transport_is_http: bool) -> bool { ... }

// Line 1084: change
pub(crate) fn spawn_stdio_transport(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> std::io::Result<rmcp::transport::child_process::TokioChildProcess> { ... }

// Line 1123: change
pub(crate) fn build_http_transport(
    url: &str,
    headers: &HashMap<String, String>,
) -> rmcp::transport::StreamableHttpClientTransport<reqwest::Client> { ... }

// Line 1151: change
pub(crate) fn build_authed_transport(
    url: &str,
    headers: &HashMap<String, String>,
    auth_manager: rmcp::transport::auth::AuthorizationManager,
) -> rmcp::transport::StreamableHttpClientTransport<
    rmcp::transport::auth::AuthClient<reqwest::Client>,
> { ... }
```

Also add `use std::collections::HashMap;` to client.rs if not already imported (check line 1 — it already is).

- [ ] **Step 3: Remove `run_initialize` and `initialize` from `client.rs`**

Delete lines 130–358 (the entire `run_initialize` method body) and lines 904–1082 (the `initialize` method + closing `}` of the `impl McpClientPool` block).

The `impl McpClientPool` block in client.rs now starts at line 109 and ends at line 902 (after `shutdown()`). Add a closing `}` if needed.

- [ ] **Step 4: Update `mcp/mod.rs` to register the new module**

Add below line 3 (`pub mod client;`):
```rust
pub mod client_oauth;
pub mod initialize;
pub mod reconnect;
```

- [ ] **Step 5: Build to verify compilation**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: `Compiling peri-middlewares...` then success (only warnings allowed).

If compilation fails with "unresolved import" errors:
- Verify `pub(crate)` on all 7 helpers in client.rs
- Verify the `use super::client::...` imports in initialize.rs list all needed symbols
- Check that `use std::collections::HashMap` is present in client.rs

- [ ] **Step 6: Commit**

```bash
git add peri-middlewares/src/mcp/initialize.rs peri-middlewares/src/mcp/client.rs peri-middlewares/src/mcp/mod.rs
git commit -m "refactor(mcp): extract initialize module from client.rs

Move run_initialize() and initialize() to mcp/initialize.rs.
Make transport helpers and status inserters pub(crate) in client.rs.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 2: Create `reconnect.rs` — extract `reconnect()`

**Files:**
- Create: `peri-middlewares/src/mcp/reconnect.rs`
- Modify: `peri-middlewares/src/mcp/client.rs` (remove lines 411–589)

- [ ] **Step 1: Create `reconnect.rs` with full content**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/mcp/reconnect.rs`:

```rust
use std::sync::Arc;

use super::auth_store::FileCredentialStore;
use super::client::{
    ClientStatus, McpClientHandle, McpClientPool, McpPoolError, OAuthStatus,
    HTTP_CONNECT_TIMEOUT, SHUTDOWN_TIMEOUT, STDIO_CONNECT_TIMEOUT,
    build_authed_transport, build_http_transport, insert_failed, insert_needs_auth,
    is_auth_required_error, spawn_stdio_transport,
};
use super::config::OAuthConfig;
use super::oauth_flow::{OAuthFlowEvent, OAuthFlowManager};
use super::transport::TransportConfig;

impl McpClientPool {
    pub async fn reconnect(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) -> Result<(), McpPoolError> {
        // [COPY lines 416–589 from client.rs verbatim]
        // Replace Self::insert_failed → insert_failed
        // Replace Self::insert_needs_auth → insert_needs_auth
        // Replace Self::is_auth_required_error → is_auth_required_error
        // transport builder calls are direct: build_http_transport, build_authed_transport, spawn_stdio_transport
    }
}
```

Copy lines 416–589 (the body of `reconnect`) from `client.rs`. Apply the same `Self::` → direct call replacements.

- [ ] **Step 2: Remove `reconnect()` from `client.rs`**

Delete lines 411–589 from `client.rs`.

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success.

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/mcp/reconnect.rs peri-middlewares/src/mcp/client.rs
git commit -m "refactor(mcp): extract reconnect module from client.rs

Move reconnect() to mcp/reconnect.rs.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 3: Create `client_oauth.rs` — extract `start_oauth_flow()` and `clear_oauth()`

**Files:**
- Create: `peri-middlewares/src/mcp/client_oauth.rs`
- Modify: `peri-middlewares/src/mcp/client.rs` (remove lines 591–734)

- [ ] **Step 1: Create `client_oauth.rs` with full content**

Create `/Users/konghayao/code/ai/perihelion/peri-middlewares/src/mcp/client_oauth.rs`:

```rust
use std::sync::Arc;

use super::auth_store::FileCredentialStore;
use super::client::{
    ClientStatus, McpClientHandle, McpClientPool, McpPoolError, OAuthStatus,
    HTTP_CONNECT_TIMEOUT, SHUTDOWN_TIMEOUT,
    build_authed_transport, insert_failed, insert_needs_auth, is_auth_required_error,
};
use super::config::OAuthConfig;
use super::oauth_flow::{OAuthFlowEvent, OAuthFlowManager};

impl McpClientPool {
    pub async fn start_oauth_flow(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Box<dyn Fn(OAuthFlowEvent) + Send + Sync>,
    ) -> Result<(), McpPoolError> {
        // [COPY lines 596–699 from client.rs verbatim]
    }

    pub async fn clear_oauth(self: &Arc<Self>, server_name: &str) -> Result<(), McpPoolError> {
        // [COPY lines 703–734 from client.rs verbatim]
    }
}
```

Copy the body of `start_oauth_flow` (lines 596–699) and `clear_oauth` (lines 703–734) from `client.rs`.

- [ ] **Step 2: Remove `start_oauth_flow()` and `clear_oauth()` from `client.rs`**

Delete lines 591–734 from `client.rs`.

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-middlewares 2>&1
```
Expected: Success.

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/mcp/client_oauth.rs peri-middlewares/src/mcp/client.rs
git commit -m "refactor(mcp): extract client_oauth module from client.rs

Move start_oauth_flow() and clear_oauth() to mcp/client_oauth.rs.
Named client_oauth.rs to avoid collision with existing oauth_flow.rs.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code.win>"
```

---

### Task 4: Full verification

- [ ] **Step 1: Run full project build**

```bash
cargo build 2>&1
```
Expected: All crates compile. No errors.

- [ ] **Step 2: Run MCP-related tests**

```bash
cargo test -p peri-middlewares mcp 2>&1
```
Expected: All tests pass. No regressions.

- [ ] **Step 3: Run full test suite**

```bash
cargo test 2>&1
```
Expected: All tests pass.

- [ ] **Step 4: Verify file sizes**

```bash
wc -l peri-middlewares/src/mcp/client.rs peri-middlewares/src/mcp/initialize.rs peri-middlewares/src/mcp/reconnect.rs peri-middlewares/src/mcp/client_oauth.rs
```

Expected approximate: `client.rs` ~620 lines, `initialize.rs` ~250 lines, `reconnect.rs` ~180 lines, `client_oauth.rs` ~150 lines. (Combined should roughly equal original 1309 minus removed whitespace.)

---

## Self-Review Checklist

1. **Spec coverage:** All 4 files from the issue's target structure are created ✓
2. **Placeholder scan:** No TODOs, no "implement later" — every step has exact code paths or copy instructions ✓
3. **Type consistency:** `McpClientPool` struct unchanged, external callers use `peri_middlewares::mcp::McpClientPool` — zero API breakage ✓
4. **Import completeness:** Each new module lists all needed `use super::client::...` imports ✓
