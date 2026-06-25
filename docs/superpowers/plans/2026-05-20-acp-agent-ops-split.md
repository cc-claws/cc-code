# ACP Server + Agent Ops 拆分 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 acp_server.rs（929行）拆分为 5 个子模块，将 agent_ops.rs（1053行）拆分为 5 个子模块，所有子文件 ≤ 400 行。

**Architecture:** acp_server.rs 按请求处理阶段拆分：types（数据类型）、requests（handle_request 路由）、notify（通知推送）、prompt/compact（执行函数）。agent_ops.rs 按事件类型拆分：acp_bridge（ACP 消息桥接）、lifecycle（Agent 生命周期）、subagent（子 Agent）、polling（轮询）。纯文件拆分 + re-export，零行为改变。

**Tech Stack:** Rust 2021, tokio, ratatui, no new dependencies.

---

## 文件结构总览

**Create:**
```
peri-tui/src/acp_server/
├── mod.rs       (~250 lines: SessionState + AcpServerConfig + SharedSessions + run_acp_server)
├── requests.rs  (~370 lines: handle_request)
├── notify.rs    (~110 lines: handle_notification + extract_session_id + send_* helpers)
├── prompt.rs    (~125 lines: execute_prompt + execute_prompt helpers)
└── compact.rs   (~150 lines: execute_compact)

peri-tui/src/app/agent_ops/
├── mod.rs        (~370 lines: handle_agent_event dispatcher + re-exports)
├── acp_bridge.rs (~55 lines: handle_acp_notification)
├── lifecycle.rs  (~310 lines: cleanup_agent_state + handle_done + handle_interrupted + handle_error)
├── subagent.rs   (~130 lines: handle_token_usage_update + handle_subagent_start)
└── polling.rs    (~200 lines: poll_agent + poll_background_events + poll_cron_triggers)
```

**Modify:**
- `peri-tui/src/acp_server.rs` → 替换为 `pub mod acp_server; pub use acp_server::*;`（~3 lines）
- `peri-tui/src/app/agent_ops.rs` → 替换为 `pub mod agent_ops; pub use agent_ops::*;`（~3 lines）
- `peri-tui/src/app/mod.rs` → 更新 imports（若有必要）

---

### Task 1: Convert acp_server.rs → acp_server/ directory

**Files:**
- Create: `peri-tui/src/acp_server/mod.rs`
- Create: `peri-tui/src/acp_server/requests.rs`
- Create: `peri-tui/src/acp_server/notify.rs`
- Delete: `peri-tui/src/acp_server.rs`

- [ ] **Step 1: Create acp_server/ directory and move file**

```bash
mkdir peri-tui/src/acp_server
cp peri-tui/src/acp_server.rs peri-tui/src/acp_server/mod.rs
```

- [ ] **Step 2: Verify build with the copy**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Build succeeds. `pub mod acp_server;` in `main.rs` discovers `acp_server/mod.rs` automatically.

- [ ] **Step 3: Extract requests.rs — handle_request (lines 170-534 in original, ~366 lines)**

Cut the `async fn handle_request(...)` function (and its complete body covering all match arms: initialize, session/new, session/set_model, session/set_mode, session/set_config_option, session/set_thinking, session/load, session/list, session/close, session/resume, session/fork, default) from `acp_server/mod.rs` and paste into `acp_server/requests.rs`.

Required imports for `requests.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::Value;
use tracing::{debug, info};
use peri_acp::transport::types::AcpError;
use peri_agent::thread::{ThreadId, ThreadMeta};
use agent_client_protocol::schema::{
    AgentCapabilities, CloseSessionResponse, ForkSessionResponse, InitializeResponse,
    ListSessionsResponse, LoadSessionResponse, NewSessionResponse, ProtocolVersion,
    PromptCapabilities, ResumeSessionResponse, SessionCapabilities, SessionCloseCapabilities,
    SessionForkCapabilities, SessionId, SessionInfo, SessionListCapabilities,
    SessionResumeCapabilities, SetSessionConfigOptionResponse, SetSessionModeResponse,
    SetSessionModelResponse,
};
use crate::config::PeriConfig;
use crate::app::agent::LlmProvider;
use super::{AcpServerConfig, SessionState};
use super::notify::{send_config_option_update, send_available_commands_update, send_session_info_update};
use super::state_builders::*;
```

Add to `acp_server/mod.rs`:
```rust
mod requests;
mod notify;
pub(crate) use requests::handle_request;
```

Remove the `async fn handle_request` definition from `acp_server/mod.rs`.

- [ ] **Step 4: Extract notify.rs — handle_notification + helpers (~95 lines)**

Cut lines 536-663 from `acp_server/mod.rs` (`handle_notification`, `extract_session_id`, `send_config_option_update`, `send_available_commands_update`, `send_session_info_update`, `build_available_commands`) and paste into `acp_server/notify.rs`.

Add to `acp_server/mod.rs`:
```rust
pub(crate) use notify::{
    handle_notification, extract_session_id, send_config_option_update,
    send_available_commands_update, send_session_info_update,
};
```

- [ ] **Step 5: Extract prompt.rs — execute_prompt (~120 lines)**

Cut lines 665-783 from `acp_server/mod.rs` (`async fn execute_prompt`) and paste into `acp_server/prompt.rs`.

Required imports for `prompt.rs`:
```rust
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info};
use peri_acp::broker::AcpTransportBroker;
use peri_acp::session::event_sink::TransportEventSink;
use peri_acp::session::executor;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadMeta};
use peri_middlewares::prelude::*;
use crate::app::agent::LlmProvider;
use crate::config::PeriConfig;
use super::{AcpServerConfig, SessionState, SharedSessions};
```

Add to `acp_server/mod.rs`:
```rust
mod prompt;
pub(crate) use prompt::execute_prompt;
```

- [ ] **Step 6: Extract compact.rs — execute_compact (~145 lines)**

Cut lines 784-929 from `acp_server/mod.rs` (`async fn execute_compact`) and paste into `acp_server/compact.rs`.

Add to `acp_server/mod.rs`:
```rust
mod compact;
pub(crate) use compact::execute_compact;
```

- [ ] **Step 7: Finalize acp_server/mod.rs**

After all extractions, `acp_server/mod.rs` retains:
- Module doc comment
- Imports used by `run_acp_server` + type definitions + shared re-exports
- `SessionState` struct
- `AcpServerConfig` struct
- `type SharedSessions`
- `pub async fn run_acp_server(...)` 
- `mod` declarations for all sub-modules
- `pub use` of state_builders re-exports

Expected: ~250 lines.

- [ ] **Step 8: Delete old acp_server.rs**

```bash
rm peri-tui/src/acp_server.rs
```

- [ ] **Step 9: Build and run tests**

```bash
cargo build -p peri-tui 2>&1
cargo test -p peri-tui --lib 2>&1 | tail -10
```
Expected: Build succeeds. All tests pass.

- [ ] **Step 10: Check for external references to acp_server**

```bash
grep -r 'acp_server::' peri-tui/src/ --include='*.rs' | grep -v 'acp_server/'
grep -r 'use crate::acp_server' peri-tui/src/ --include='*.rs'
```
Verify all references still resolve through `acp_server/mod.rs` re-exports.

- [ ] **Step 11: Commit**

```bash
git add peri-tui/src/acp_server/
git rm peri-tui/src/acp_server.rs
git commit -m "refactor(acp): split acp_server.rs into sub-modules

acp_server.rs: 929 → acp_server/{mod.rs(250), requests.rs(370),
notify.rs(110), prompt.rs(125), compact.rs(150)}.
Extract: handle_request, notify helpers, execute_prompt, execute_compact."
```

---

### Task 2: Convert agent_ops.rs → agent_ops/ directory

**Files:**
- Create: `peri-tui/src/app/agent_ops/mod.rs`
- Create: `peri-tui/src/app/agent_ops/acp_bridge.rs`
- Create: `peri-tui/src/app/agent_ops/lifecycle.rs`
- Create: `peri-tui/src/app/agent_ops/subagent.rs`
- Create: `peri-tui/src/app/agent_ops/polling.rs`
- Delete: `peri-tui/src/app/agent_ops.rs`

- [ ] **Step 1: Create agent_ops/ directory and move file**

```bash
mkdir peri-tui/src/app/agent_ops
cp peri-tui/src/app/agent_ops.rs peri-tui/src/app/agent_ops/mod.rs
```

- [ ] **Step 2: Verify build**

```bash
cargo build -p peri-tui 2>&1
```
Expected: Build succeeds. `pub mod agent_ops;` in `app/mod.rs` discovers `agent_ops/mod.rs` automatically.

- [ ] **Step 3: Extract acp_bridge.rs — handle_acp_notification (~52 lines)**

Cut `fn handle_acp_notification` (lines 8-51 in original) from `agent_ops/mod.rs` and paste into `agent_ops/acp_bridge.rs`.

```rust
// peri-tui/src/app/agent_ops/acp_bridge.rs
use tracing::debug;
use crate::app::App;
use super::super::AcpNotification;
use super::super::AgentEvent;

impl App {
    pub(crate) fn handle_acp_notification(&mut self, notif: AcpNotification) -> (bool, bool, bool) {
        // ... verbatim body from agent_ops/mod.rs
    }
}
```

Add to `agent_ops/mod.rs`:
```rust
mod acp_bridge;
```

- [ ] **Step 4: Extract lifecycle.rs — cleanup + done + interrupted + error (~310 lines)**

Cut functions `cleanup_agent_state` (lines 58-83), `handle_done` (lines 187-284), `handle_interrupted` (lines 285-374), `handle_error` (lines 375-453) from `agent_ops/mod.rs` and paste into `agent_ops/lifecycle.rs`.

Add to `agent_ops/mod.rs`:
```rust
mod lifecycle;
```

- [ ] **Step 5: Extract subagent.rs — token_usage_update + subagent_start (~130 lines)**

Cut `handle_token_usage_update` (lines 84-140) and `handle_subagent_start` (lines 141-186) into `agent_ops/subagent.rs`.

Add to `agent_ops/mod.rs`:
```rust
mod subagent;
```

- [ ] **Step 6: Extract polling.rs — poll functions (~200 lines)**

Cut `poll_agent` (lines 816-976), `poll_background_events` (lines 977-1008), `poll_cron_triggers` (lines 1009+) into `agent_ops/polling.rs`.

Add to `agent_ops/mod.rs`:
```rust
mod polling;
```

- [ ] **Step 7: Finalize agent_ops/mod.rs**

After extractions, `agent_ops/mod.rs` retains:
- `use super::*;` import
- `impl App` block containing only `handle_agent_event` (the main event dispatcher)
- `mod` declarations for all sub-files

Expected: ~370 lines (dispatcher only).

- [ ] **Step 8: Delete old agent_ops.rs**

```bash
rm peri-tui/src/app/agent_ops.rs
```

- [ ] **Step 9: Build and test**

```bash
cargo build -p peri-tui 2>&1
cargo test -p peri-tui --lib 2>&1 | tail -10
```
Expected: Build succeeds, all tests pass.

- [ ] **Step 10: Verify line counts**

```bash
echo "=== agent_ops final sizes ==="
wc -l peri-tui/src/app/agent_ops/mod.rs
wc -l peri-tui/src/app/agent_ops/acp_bridge.rs
wc -l peri-tui/src/app/agent_ops/lifecycle.rs
wc -l peri-tui/src/app/agent_ops/subagent.rs
wc -l peri-tui/src/app/agent_ops/polling.rs
```

Expected: mod.rs ≤ 400 lines, all sub-files ≤ 350 lines.

- [ ] **Step 11: Commit**

```bash
git add peri-tui/src/app/agent_ops/
git rm peri-tui/src/app/agent_ops.rs
git commit -m "refactor(agent): split agent_ops.rs into sub-modules

agent_ops.rs: 1053 → agent_ops/{mod.rs(370), acp_bridge.rs(52),
lifecycle.rs(310), subagent.rs(130), polling.rs(200)}.
Extract by concern: ACP bridge, lifecycle, subagent, polling."
```

---

### Task 3: Verify & run pre-commit

**Files:** None created. Verification only.

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace 2>&1
```
Expected: All crates compile clean.

- [ ] **Step 2: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -20
```
Expected: All tests pass.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --workspace -- -D warnings 2>&1
```
Expected: No new warnings.

- [ ] **Step 4: Run rustfmt**

```bash
cargo fmt --all
```

- [ ] **Step 5: Verify no dead imports**

```bash
cargo check 2>&1 | grep -i 'unused\|warning'
```
Expected: Zero warnings.

- [ ] **Step 6: Final line count summary**

```bash
echo "=== ACP Server ===" && wc -l peri-tui/src/acp_server/*.rs
echo "=== Agent Ops ===" && wc -l peri-tui/src/app/agent_ops/*.rs
```

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "refactor: finalize ACP server + agent ops split

Summary:
ACP Server: 929 → 5 files, max 370 lines (was single 929-line file)
- acp_server/{mod.rs, requests.rs, notify.rs, prompt.rs, compact.rs}

Agent Ops: 1053 → 5 files, max 370 lines (was single 1053-line file)
- agent_ops/{mod.rs, acp_bridge.rs, lifecycle.rs, subagent.rs, polling.rs}

All tests pass, zero behavior change."
```
