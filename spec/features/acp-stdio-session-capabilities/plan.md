# ACP Dispatch 统一到 peri-acp 层 — 实施计划

## 目标

将 `acp_server/requests.rs`（TUI）和 `acp_stdio.rs`（stdio）中重复的 ACP 请求处理逻辑提取到 `peri-acp/src/dispatch/`，消除双份实现，同时修复 Zed 客户端 "Loading or resuming sessions is not supported" 错误。

## 当前架构问题

```
peri-acp/src/dispatch/mod.rs   ← 只有 TODO 注释，无实现
peri-tui/src/acp_server/requests.rs  ← TUI 路径：match method 手工分发
peri-tui/src/acp_stdio.rs        ← stdio 路径：agent_client_protocol builder 分发
```

两条路径用不同分发框架实现了相同业务逻辑：

| 方法 | TUI 路径 | stdio 路径 |
|------|---------|-----------|
| `initialize` | ✅ 含 session 能力 | ❌ 只有 promptCapabilities |
| `session/new` | ✅ | ✅ |
| `session/load` | ✅ | ❌ 缺失 |
| `session/list` | ✅ | ❌ 缺失 |
| `session/close` | ✅ | ❌ 缺失 |
| `session/resume` | ✅ | ❌ 缺失 |
| `session/fork` | ✅ | ❌ 缺失 |
| `session/set_model` | ✅ | ✅ |
| `session/set_mode` | ✅ | ✅ |
| `session/set_config_option` | ✅ | ✅ |
| `session/cancel` | ✅ | ✅ |

## 统一策略

**业务逻辑下移**：将 session CRUD 和 frozen data 构建等无状态纯逻辑提取到 `peri-acp/src/dispatch/`。**传输适配保留**：JSON-RPC 解析、session HashMap 管理、通知推送留在各自传输层。

```
peri-acp/src/dispatch/
├── mod.rs          ← pub use 导出
├── init.rs         ← build_initialize_response()
├── new_session.rs  ← build_new_session_data(...)
├── load_session.rs ← load_session_history(...)
├── list_sessions.rs← list_sessions_as_info(...)
├── fork_session.rs ← fork_session_thread(...)
└── test.rs         ← 单元测试

peri-tui/src/acp_server/requests.rs  ← 调用 peri_acp::dispatch::*
peri-tui/src/acp_stdio.rs            ← 调用 peri_acp::dispatch::*
```

**保留在传输层**（状态管理依赖特定锁类型/通知机制）：
- session HashMap 增删查（两种传输用不同锁：`tokio::sync::Mutex` vs `parking_lot::RwLock`）
- `set_model`/`set_mode`/`set_config_option`（直接 mutate 传输层状态）
- `session/cancel`（操作 CancelToken，已在 session map 中）
- 通知推送（`AvailableCommandsUpdate`/`ConfigOptionUpdate`）
- JSON-RPC 参数解析和响应序列化

---

## Task 1: `peri-acp/src/dispatch/init.rs` — build_initialize_response

**新增文件**：`peri-acp/src/dispatch/init.rs`

纯函数，无依赖，返回完整的 `InitializeResponse`（对齐当前 TUI 路径的能力声明）。

```rust
use agent_client_protocol::schema::{
    AgentCapabilities, InitializeResponse, PromptCapabilities, ProtocolVersion,
    SessionCapabilities, SessionCloseCapabilities, SessionForkCapabilities,
    SessionListCapabilities, SessionResumeCapabilities,
};

pub fn build_initialize_response() -> InitializeResponse {
    let caps = AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(PromptCapabilities::new())
        .session_capabilities(
            SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .close(SessionCloseCapabilities::new())
                .resume(SessionResumeCapabilities::new())
                .fork(SessionForkCapabilities::new()),
        );
    InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(caps)
}
```

**验证**：`cargo check -p peri-acp`

---

## Task 2: `peri-acp/src/dispatch/new_session.rs` — build_new_session_data

**新增文件**：`peri-acp/src/dispatch/new_session.rs`

提取 `session/new` 的核心业务逻辑：创建 thread、构建 frozen data、扫描 skills、构建 modes/models/configOptions。返回 `NewSessionData` 结构体，由调用方存入其 session map 并推送通知。

```rust
use std::path::PathBuf;
use std::sync::Arc;
use peri_agent::thread::{ThreadId, ThreadMeta, ThreadStore};
use peri_middlewares::prelude::SharedPermissionMode;
use peri_middlewares::skills::SkillMetadata;
use agent_client_protocol::schema::{AgentMode, AgentModel, ConfigOption};
use crate::provider::{LlmProvider, PeriConfig};
use crate::prompt::{build_system_prompt, PromptFeatures};
use crate::session::state_builders::{build_config_options, build_mode_state, build_model_state};

pub struct NewSessionData {
    pub thread_id: String,
    pub frozen_system_prompt: String,
    pub frozen_claude_md: Option<String>,
    pub frozen_claude_local_md: Option<String>,
    pub frozen_skill_summary: Option<String>,
    pub frozen_date: String,
    pub modes: Vec<AgentMode>,
    pub models: Vec<AgentModel>,
    pub config_options: Vec<ConfigOption>,
    pub skills: Vec<SkillMetadata>,
}

pub async fn build_new_session_data(
    thread_store: &dyn ThreadStore,
    cwd: &str,
    plugin_skill_dirs: &[PathBuf],
    plugin_agent_dirs: &[PathBuf],
    permission_mode: &Arc<SharedPermissionMode>,
    provider: &LlmProvider,
    peri_config: &PeriConfig,
) -> anyhow::Result<NewSessionData> {
    let meta = ThreadMeta::new(cwd);
    let thread_id = thread_store.create_thread(meta).await?;
    let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (frozen_claude_md, frozen_claude_local_md) =
        peri_middlewares::AgentsMdMiddleware::read_frozen_content(cwd);
    let frozen_skill_summary =
        peri_middlewares::SkillsMiddleware::build_frozen_summary(cwd, plugin_skill_dirs);
    let features = PromptFeatures::detect();
    let frozen_system_prompt =
        build_system_prompt(None, cwd, features, plugin_agent_dirs, Some(&frozen_date));
    let skill_dirs =
        peri_middlewares::SkillsMiddleware::resolve_dirs_static(cwd, plugin_skill_dirs);
    let skills = peri_middlewares::skills::list_skills(&skill_dirs);
    let modes = build_mode_state(permission_mode);
    let models = build_model_state(provider, peri_config);
    let config_options = build_config_options(peri_config, provider, permission_mode.load());

    Ok(NewSessionData {
        thread_id: thread_id.to_string(),
        frozen_system_prompt,
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
        modes,
        models,
        config_options,
        skills,
    })
}
```

**注意**：`build_model_state` 当前签名是 `build_model_state(&LlmProvider, &PeriConfig)`，TUI 路径已直接传入。确认两边的 `LlmProvider` 是同类型（peri-acp re-export，peri-tui re-export peri-acp 的）。

**验证**：`cargo check -p peri-acp`

---

## Task 3: `peri-acp/src/dispatch/load_session.rs` — load_session_history

**新增文件**：`peri-acp/src/dispatch/load_session.rs`

```rust
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadId, ThreadStore};

pub async fn load_session_history(
    thread_store: &dyn ThreadStore,
    session_id: &str,
) -> Vec<BaseMessage> {
    match thread_store.load_messages(&ThreadId::from(session_id.to_string())).await {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::warn!(session_id = %session_id, error = %e,
                "session/load: thread not found, returning empty history");
            Vec::new()
        }
    }
}
```

**验证**：`cargo check -p peri-acp`

---

## Task 4: `peri-acp/src/dispatch/list_sessions.rs` — list_sessions_as_info

**新增文件**：`peri-acp/src/dispatch/list_sessions.rs`

```rust
use agent_client_protocol::schema::{SessionId, SessionInfo};
use peri_agent::thread::ThreadStore;

pub async fn list_sessions_as_info(
    thread_store: &dyn ThreadStore,
    cwd_filter: Option<&str>,
) -> Result<Vec<SessionInfo>, String> {
    let threads = thread_store
        .list_threads()
        .await
        .map_err(|e| format!("Failed to list sessions: {e}"))?;
    Ok(threads
        .into_iter()
        .filter(|t| {
            if let Some(cwd) = cwd_filter {
                t.cwd == cwd
            } else {
                true
            }
        })
        .map(|t| {
            SessionInfo::new(SessionId::new(&t.id), std::path::PathBuf::from(&t.cwd))
                .title(t.title.clone())
                .updated_at(t.updated_at.to_rfc3339())
        })
        .collect())
}
```

**验证**：`cargo check -p peri-acp`

---

## Task 5: `peri-acp/src/dispatch/fork_session.rs` — fork_session_thread

**新增文件**：`peri-acp/src/dispatch/fork_session.rs`

```rust
use peri_agent::messages::BaseMessage;
use peri_agent::thread::{ThreadMeta, ThreadStore};

pub async fn fork_session_thread(
    thread_store: &dyn ThreadStore,
    cwd: &str,
    source_history: &[BaseMessage],
) -> anyhow::Result<String> {
    let meta = ThreadMeta::new(cwd);
    let new_thread_id = thread_store.create_thread(meta).await?;
    if !source_history.is_empty() {
        if let Err(e) = thread_store
            .append_messages(&new_thread_id, source_history)
            .await
        {
            tracing::warn!(error = %e, "session/fork: failed to copy messages to new thread");
        }
    }
    Ok(new_thread_id.to_string())
}
```

**验证**：`cargo check -p peri-acp`

---

## Task 6: `peri-acp/src/dispatch/mod.rs` — 注册模块

**修改文件**：`peri-acp/src/dispatch/mod.rs`

```rust
//! ACP method dispatch — shared business logic.
//!
//! Provides pure/async functions that implement ACP session lifecycle
//! operations. Both TUI (MpscTransport) and stdio transports call these
//! functions, keeping only JSON-RPC framing and session-state management
//! in their respective transport layers.

pub mod fork_session;
pub mod init;
pub mod list_sessions;
pub mod load_session;
pub mod new_session;

pub use fork_session::fork_session_thread;
pub use init::build_initialize_response;
pub use list_sessions::list_sessions_as_info;
pub use load_session::load_session_history;
pub use new_session::{build_new_session_data, NewSessionData};
```

**验证**：`cargo check -p peri-acp`

---

## Task 7: `peri-tui/src/acp_stdio.rs` — 接入统一 dispatch

**修改文件**：`peri-tui/src/acp_stdio.rs`

### 7a. 替换 `initialize` handler（行 264-276）

```rust
// Before:
AgentCapabilities::new()
    .prompt_capabilities(PromptCapabilities::new()),

// After:
use peri_acp::dispatch::build_initialize_response;
// ...
async move |_req: InitializeRequest, responder, _cx| {
    tracing::info!("ACP initialize");
    responder.respond(build_initialize_response())
},
```

### 7b. 替换 `session/new` handler 的 frozen data 构建（行 294-320）

将 frozen data 构建、skills 扫描、modes/models/configOptions 构建替换为调用 `build_new_session_data()`，然后自行存入 session HashMap 并发送通知。

### 7c. 新增 `session/load` handler

调用 `load_session_history()` + `build_mode_state/build_model_state/build_config_options`，自行管理 session HashMap。

### 7d. 新增 `session/list` handler

调用 `list_sessions_as_info()`。

### 7e. 新增 `session/close` handler

纯 session HashMap 操作 + cancel token，无需 dispatch 函数。

### 7f. 新增 `session/resume` handler

纯 session HashMap 操作，无需 dispatch 函数。

### 7g. 新增 `session/fork` handler

调用 `fork_session_thread()` + 自行管理 session HashMap。

**验证**：`cargo check -p peri-tui`

---

## Task 8: `peri-tui/src/acp_server/requests.rs` — 接入统一 dispatch

**修改文件**：`peri-tui/src/acp_server/requests.rs`

将 `initialize`、`session/new`、`session/load`、`session/list`、`session/fork` 的 handler 中的业务逻辑替换为 `peri_acp::dispatch::*` 调用。

### 8a. `initialize`（行 39-58）

```rust
// Before: 本地构建 AgentCapabilities
// After:
use peri_acp::dispatch::build_initialize_response;
let resp = build_initialize_response();
serde_json::to_value(resp)
    .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
```

### 8b. `session/new`（行 60-140）

替换 frozen data 构建 + skills 扫描 + modes/models/configOptions 构建为 `build_new_session_data()`。

### 8c. `session/load`（行 245-305）

替换 `thread_store.load_messages()` 调用为 `load_session_history()`。

### 8d. `session/list`（行 307-338）

替换为 `list_sessions_as_info()`。

### 8e. `session/fork`（行 401-453）

替换 thread 创建 + 消息复制为 `fork_session_thread()`。

**验证**：`cargo build -p peri-tui --lib`

---

## Task 9: 清理 dead code & imports

两边接入后，移除不再需要的 import 和局部函数。

**验证**：`cargo build -p peri-tui --lib && cargo test -p peri-acp --lib`

---

## Task 10: `peri-acp/src/dispatch/test.rs` — 单元测试

**新增文件**：`peri-acp/src/dispatch/test.rs`

### 10a. `test_build_initialize_response_has_all_capabilities`

```rust
#[test]
fn test_build_initialize_response_has_all_capabilities() {
    let resp = init::build_initialize_response();
    let caps = resp.agent_capabilities.unwrap();
    assert!(caps.load_session.unwrap_or(false));
    assert!(caps.prompt_capabilities.is_some());
    // 验证 sessionCapabilities 四个子能力均存在
    assert!(caps.session_capabilities.is_some());
    let sc = caps.session_capabilities.unwrap();
    assert!(sc.list.is_some());
    assert!(sc.close.is_some());
    assert!(sc.resume.is_some());
    assert!(sc.fork.is_some());
}
```

### 10b. `test_list_sessions_as_info`

使用 mock ThreadStore 验证 cwd 过滤和 SessionInfo 映射。

### 10c. `test_build_new_session_data`

使用 tempfile + mock 验证 frozen data 构建。

**验证**：`cargo test -p peri-acp --lib -- dispatch`

---

## 执行顺序与依赖

| Task | 依赖 | 可并行 |
|------|------|--------|
| T1 (init.rs) | 无 | ✅ |
| T2 (new_session.rs) | 无 | ✅ |
| T3 (load_session.rs) | 无 | ✅ |
| T4 (list_sessions.rs) | 无 | ✅ |
| T5 (fork_session.rs) | 无 | ✅ |
| T6 (mod.rs 注册) | T1-T5 | — |
| T7 (acp_stdio.rs) | T6 | — |
| T8 (acp_server/requests.rs) | T6 | ✅ (与 T7 并行) |
| T9 (清理) | T7, T8 | — |
| T10 (测试) | T6 | — |

T1-T5 完全独立，可并行开发。T7 和 T8 可并行。

---

## 风险

| 风险 | 缓解 |
|------|------|
| `build_model_state` 的 `LlmProvider` 类型不一致 | 确认 peri-tui 已 re-export peri-acp 的 LlmProvider（CLAUDE.md 记载已统一） |
| `session/resume` 在 stdio 模式缺少 frozen data | 接受空 frozen data，或后续 prompt 时懒填充 |
| `ThreadStore` trait 的 async 方法在 `dispatch` 函数签名中 | 使用 `&dyn ThreadStore`（已有 `#[async_trait]`） |
| `agent_client_protocol_schema` 版本与 peri-tui 一致 | 当前两者均用 workspace 版本管理，Cargo.toml 无冲突 |

## 变更文件

| 文件 | 操作 | 预计行数 |
|------|------|---------|
| `peri-acp/src/dispatch/init.rs` | 新增 | ~20 |
| `peri-acp/src/dispatch/new_session.rs` | 新增 | ~50 |
| `peri-acp/src/dispatch/load_session.rs` | 新增 | ~15 |
| `peri-acp/src/dispatch/list_sessions.rs` | 新增 | ~25 |
| `peri-acp/src/dispatch/fork_session.rs` | 新增 | ~20 |
| `peri-acp/src/dispatch/mod.rs` | 修改 | ~15（替换 TODO） |
| `peri-acp/src/dispatch/test.rs` | 新增 | ~60 |
| `peri-tui/src/acp_stdio.rs` | 修改 | +60 -120（接入 dispatch → 净减 ~60 行） |
| `peri-tui/src/acp_server/requests.rs` | 修改 | +15 -50（接入 dispatch → 净减 ~35 行） |

**净效果**：`peri-acp` +~205 行，`peri-tui` -~95 行，消除 ~150 行重复逻辑。
