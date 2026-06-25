# ACP Session 生命周期路由与能力声明 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 TUI ACP Server 缺失的 5 个 session 生命周期方法路由、初始化能力声明、以及 2 个 `session/update` 通知变体映射

**Architecture:** 纯路由层补全——SessionManager / ThreadStore 基础设施已就绪，仅需在 `acp_server.rs` 的 `handle_request()` 和 `handle_notification()` 中添加新方法匹配分支，在 `mapper.rs` 中添加通知变体映射，以及在 `initialize` 响应中声明能力。无需新增 trait、新类型或新的后台任务模式。

**Tech Stack:** `agent-client-protocol` v0.11 (`unstable` umbrella feature 已启用，所有 session 类型可用)、ThreadStore trait、现有 HashMap<String, SessionState>

---

### Task 1: 初始化能力声明

**Files:**
- Modify: `peri-tui/src/acp_server.rs:166-176`

- [ ] **Step 1: 在 initialize 处理器中声明完整 AgentCapabilities**

将 `AgentCapabilities::new()` 替换为包含所有已实现能力的完整声明：

```rust
"initialize" => {
    let version = params
        .get("protocolVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    info!(protocol_version = %version, "ACP initialize");
    let caps = AgentCapabilities::new()
        .load_session(true)
        .session_capabilities(
            agent_client_protocol::schema::SessionCapabilities::new()
                .list(agent_client_protocol::schema::SessionListCapabilities::new())
                .close(agent_client_protocol::schema::SessionCloseCapabilities::new())
                .resume(agent_client_protocol::schema::SessionResumeCapabilities::new())
                .fork(agent_client_protocol::schema::SessionForkCapabilities::new()),
        );
    let resp = InitializeResponse::new(ProtocolVersion::V1)
        .agent_capabilities(caps);
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 更新 imports，添加新的能力类型**

在 `acp_server.rs` 顶部的 import 块中加入：

```rust
use agent_client_protocol::schema::{
    AgentCapabilities, InitializeResponse, LoadSessionResponse, CloseSessionResponse,
    ResumeSessionResponse, ListSessionsResponse, ForkSessionResponse, ProtocolVersion,
    SessionCapabilities, SessionCloseCapabilities, SessionForkCapabilities,
    SessionListCapabilities, SessionResumeCapabilities, SessionInfo,
};
```

- [ ] **Step 3: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

预期：编译通过，无新增错误。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): declare session capabilities in initialize response

- Declare loadSession, sessionCapabilities.list/close/resume/fork
- Add imports for new capability types

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: session/load 路由 — 恢复历史会话并重放消息

**Files:**
- Modify: `peri-tui/src/acp_server.rs:316` (在 `_` 通配符前插入)

- [ ] **Step 1: 添加 session/load 处理分支**

在 `handle_request()` 的 match 中，`"session/set_thinking"` 分支之后、`_ => Err(...)` 之前插入：

```rust
"session/load" => {
    let req_session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
    let cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    // 从 ThreadStore 加载历史消息
    let history = match cfg
        .thread_store
        .load_messages(&peri_agent::thread::ThreadId::from(req_session_id.to_string()))
        .await
    {
        Ok(msgs) => msgs,
        Err(e) => {
            tracing::warn!(session_id = %req_session_id, error = %e, "session/load: thread not found, creating empty session");
            Vec::new()
        }
    };

    // 将加载的消息存入 session state（如 session 已存在则追加）
    if let Some(state) = sessions.get_mut(req_session_id) {
        if state.history.is_empty() {
            state.history = history;
        }
    } else {
        sessions.insert(
            req_session_id.to_string(),
            SessionState {
                session_id: req_session_id.to_string(),
                thread_id: req_session_id.to_string(),
                cwd: cwd.to_string(),
                history,
                cancel_token: None,
            },
        );
    }

    // 构建响应（包含 modes/models/configOptions）
    let modes = build_mode_state(&cfg.permission_mode);
    let models = {
        let p = cfg.provider.read();
        let c = cfg.peri_config.read();
        build_model_state(&p, &c)
    };
    let config_options = {
        let c = cfg.peri_config.read();
        let p = cfg.provider.read();
        build_config_options(&c, &p, cfg.permission_mode.load())
    };
    let resp = LoadSessionResponse::new()
        .modes(modes)
        .models(models)
        .config_options(config_options);
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): add session/load route handler

- Load messages from ThreadStore by session_id
- Idempotent: skip insert if session already active
- Return modes/models/configOptions in response

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: session/list 路由 — 列出所有历史会话

**Files:**
- Modify: `peri-tui/src/acp_server.rs` (在 session/load 分支后插入)

- [ ] **Step 1: 添加 session/list 处理分支**

```rust
use std::path::PathBuf;
// ...

"session/list" => {
    let threads = cfg
        .thread_store
        .list_threads()
        .await
        .map_err(|e| AcpError::new(-32603, format!("Failed to list sessions: {e}")))?;

    // 按 cwd 过滤（如果请求中指定了 cwd）
    let cwd_filter = params
        .get("cwd")
        .and_then(|v| v.as_str());

    let entries: Vec<SessionInfo> = threads
        .into_iter()
        .filter(|t| {
            if let Some(cwd) = cwd_filter {
                t.cwd == cwd
            } else {
                true
            }
        })
        .map(|t| {
            SessionInfo::new(
                SessionId::new(t.id.clone()),
                PathBuf::from(t.cwd.clone()),
            )
            .title(t.title.clone())
            .updated_at(t.updated_at.to_rfc3339())
        })
        .collect();

    let resp = ListSessionsResponse::new(entries);
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 检查 ThreadMeta 结构体字段确认字段名**

```bash
grep -A10 'pub struct ThreadMeta' /Users/konghayao/code/ai/perihelion/peri-agent/src/thread/mod.rs
```

- [ ] **Step 3: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -30
```

预期：如果 ThreadMeta 字段名与代码中不一致，根据实际字段名修正后再编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): add session/list route handler

- Delegate to ThreadStore::list_threads()
- Support optional cwd filter parameter
- Return ListSessionEntry with session metadata

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: session/close 路由 — 关闭会话释放资源

**Files:**
- Modify: `peri-tui/src/acp_server.rs` (在 session/list 分支后插入)

- [ ] **Step 1: 添加 session/close 处理分支**

```rust
"session/close" => {
    let req_session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;

    if let Some(state) = sessions.remove(req_session_id) {
        // Cancel any ongoing agent execution
        if let Some(ref token) = state.cancel_token {
            token.cancel();
        }
        info!(session_id = %req_session_id, "Session closed");
    }
    let resp = CloseSessionResponse::new();
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): add session/close route handler

- Remove session from map and cancel any ongoing execution
- Idempotent: no-op if session not found

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 5: session/resume 路由 — 断线重连（不重放历史）

**Files:**
- Modify: `peri-tui/src/acp_server.rs` (在 session/close 分支后插入)

- [ ] **Step 1: 添加 session/resume 处理分支**

```rust
"session/resume" => {
    let req_session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
    let cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    // 如果 session 不存在则创建空 session（不加载历史，区别于 session/load）
    if !sessions.contains_key(req_session_id) {
        sessions.insert(
            req_session_id.to_string(),
            SessionState {
                session_id: req_session_id.to_string(),
                thread_id: req_session_id.to_string(),
                cwd: cwd.to_string(),
                history: Vec::new(),
                cancel_token: None,
            },
        );
        info!(session_id = %req_session_id, "Session resumed (new)");
    } else {
        info!(session_id = %req_session_id, "Session resumed (existing)");
    }

    let resp = ResumeSessionResponse::new();
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): add session/resume route handler

- Create empty session if not exists (no history replay, unlike session/load)
- Idempotent if session already active

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: session/fork 路由 — 复制会话上下文创建分支

**Files:**
- Modify: `peri-tui/src/acp_server.rs` (在 session/resume 分支后插入)

- [ ] **Step 1: 添加 session/fork 处理分支**

```rust
"session/fork" => {
    let source_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AcpError::new(-32602, "missing sessionId"))?;
    let cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    // 查找源 session
    let source_history = sessions
        .get(source_id)
        .map(|s| s.history.clone())
        .ok_or_else(|| AcpError::new(-32602, format!("source session not found: {source_id}")))?;

    // 创建新 Thread 并复制历史消息
    let meta = peri_agent::thread::ThreadMeta::new(cwd);
    let new_thread_id = cfg
        .thread_store
        .create_thread(meta)
        .await
        .map_err(|e| AcpError::new(-32603, format!("Thread creation failed: {e}")))?;

    // 复制消息到新 Thread
    if !source_history.is_empty() {
        if let Err(e) = cfg
            .thread_store
            .append_messages(&new_thread_id, &source_history)
            .await
        {
            tracing::warn!(error = %e, "session/fork: failed to copy messages to new thread");
        }
    }

    let new_session_id = new_thread_id.clone();
    sessions.insert(
        new_session_id.clone(),
        SessionState {
            session_id: new_session_id.clone(),
            thread_id: new_thread_id.clone(),
            cwd: cwd.to_string(),
            history: source_history,
            cancel_token: None,
        },
    );

    info!(source = %source_id, new = %new_session_id, "Session forked");
    let resp = ForkSessionResponse::new(SessionId::new(new_session_id));
    serde_json::to_value(resp)
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 2: 构建验证编译**

```bash
cargo build -p peri-tui 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/acp_server.rs
git commit -m "feat(acp): add session/fork route handler

- Create new thread with copied message history
- Persist forked messages via ThreadStore

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 7: UserMessageChunk / AvailableCommandsUpdate — 低优先级通知变体

**Files:** 本期不修改代码，仅更新 issue 文档

- [ ] **Step 1: 分析实现前提**

`UserMessageChunk` 的用途是在 `session/load` 时将历史消息逐条流式发送给 Client，以模拟实时对话回放。本期 Task 2 的 `session/load` 仅将消息加载到内存中的 `SessionState.history`，未实现通知重放机制。

`AvailableCommandsUpdate` 用于通知 Client 可用命令列表变更。本期无命令变更事件源，留待后续命令系统增强时实现。

- [ ] **Step 2: 更新 issue 说明**

在 `spec/issues/2026-05-19-acp-missing-capabilities.md` 底部追加：

```
> **2026-05-19 更新**: 核心 5 个 session 路由已在本期完成后实现。`UserMessageChunk` 依赖 session/load 通知重放机制，`AvailableCommandsUpdate` 依赖命令变更事件源，两项均属低优先级增强，留待后续。
```

- [ ] **Step 3: Commit（可选）**

```bash
git add spec/issues/2026-05-19-acp-missing-capabilities.md
git commit -m "docs: clarify UserMessageChunk/AvailableCommandsUpdate deferred status

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

### Task 8: 构建验证与清理

**Files:** 不变

- [ ] **Step 1: 全量编译检查**

```bash
cargo build -p peri-tui 2>&1
```

预期：编译通过，无新增 warning。

- [ ] **Step 2: 运行现有测试**

```bash
cargo test -p peri-acp --lib 2>&1 | tail -10
cargo test -p peri-tui --lib 2>&1 | tail -10
```

预期：所有已有测试通过。

- [ ] **Step 3: 代码规范检查**

```bash
lefthook run pre-commit
```

- [ ] **Step 4: 更新 issue 状态**

修改 `spec/issues/2026-05-19-acp-missing-capabilities.md`，将状态从 `Open` 改为 `Fixed`，添加完成日期和实现记录。

---

## Self-Review

**1. Spec coverage:**
- `session/load` ✅ Task 2
- `session/resume` ✅ Task 5
- `session/close` ✅ Task 4
- `session/list` ✅ Task 3
- `session/fork` ✅ Task 6
- 能力声明缺失 ✅ Task 1
- `user_message_chunk` ✅ Task 7 (低优先级，本期不实施)
- `available_commands_update` ✅ Task 7 (低优先级，本期不实施)

**2. Placeholder scan:** 无 TBD/TODO/implement later 类占位符。

**3. Type consistency:**
- 所有 ACP 类型均来自 `agent-client-protocol` v0.11 `unstable` umbrella（已验证 Cargo.toml）
- `ThreadId::from(String)` / `ThreadMeta::new(cwd)` 已在现有代码中使用
- `SessionState` 结构体字段名与现有定义一致
- `ThreadStore::list_threads()` 返回 `Vec<ThreadMeta>`，`load_messages()` 返回 `Vec<BaseMessage>`，`append_messages()` 签名已确认
