### Task 4: 注册 dispatch 模块 + 重构 TUI 调用点

**背景：** 新 dispatch 模块需要在 `mod.rs` 中注册并重导出。同时 TUI 路径的 `requests.rs`、`notify.rs` 和 stdio 路径的 `acp_stdio.rs` 需要改用新的 dispatch 函数替代旧的本地实现。

#### 执行步骤

- [ ] **Step 4.1**: 修改 `peri-acp/src/dispatch/mod.rs`

在现有 `list_sessions` 行后追加新模块声明和重导出：

```rust
pub mod commands;
pub mod session_fork;
pub mod session_load;

pub use commands::build_available_commands;
pub use init::build_initialize_response;
pub use list_sessions::list_sessions_as_info;
pub use session_fork::fork_session;
pub use session_load::load_session_messages;
```

- [ ] **Step 4.2**: 修改 `peri-tui/src/acp_server/requests.rs`

**4.2a** — 在文件顶部 import 区域添加 `dispatch::load_session_messages` 和 `dispatch::fork_session` 的引用（注意该文件已有 `use peri_acp::dispatch;`，直接使用 `dispatch::` 前缀即可）：

```
// 无需新增 use 语句，dispatch:: 前缀已可用
```

**4.2b** — 替换 `session/load` handler（行 233-293）中 `thread_store.load_messages()` 调用：

将行 241-251：
```rust
let history = match cfg
    .thread_store
    .load_messages(&ThreadId::from(req_session_id.to_string()))
    .await
{
    Ok(msgs) => msgs,
    Err(e) => {
        tracing::warn!(session_id = %req_session_id, error = %e, "session/load: thread not found, creating empty session");
        Vec::new()
    }
};
```

替换为：
```rust
let history = dispatch::load_session_messages(
    cfg.thread_store.as_ref(),
    req_session_id,
).await;
```

**4.2c** — 替换 `session/fork` handler（行 389-441）中 thread_store 创建+复制逻辑：

将行 403-418：
```rust
let meta = ThreadMeta::new(cwd);
let new_thread_id = cfg
    .thread_store
    .create_thread(meta)
    .await
    .map_err(|e| AcpError::new(-32603, format!("Thread creation failed: {e}")))?;

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
```

替换为：
```rust
let source_history = sessions
    .get(source_id)
    .map(|s| s.history.clone())
    .ok_or_else(|| {
        AcpError::new(-32602, format!("source session not found: {source_id}"))
    })?;

let (new_thread_id, copied_history) = dispatch::fork_session(
    cfg.thread_store.as_ref(),
    source_id,
    &source_history,
    cwd,
).await
.map_err(|e| AcpError::new(-32603, e))?;

let new_session_id = new_thread_id.clone();
```

然后修改后续的 `sessions.insert()` 使用 `copied_history` 替代 `source_history`：

```rust
sessions.insert(
    new_session_id.clone(),
    SessionState {
        session_id: new_session_id.clone(),
        thread_id: new_thread_id.clone(),
        cwd: cwd.to_string(),
        history: copied_history,
        cancel_token: None,
        frozen_system_prompt: None,
        frozen_claude_md: None,
        frozen_claude_local_md: None,
        frozen_skill_summary: None,
        frozen_date: None,
    },
);
```

- [ ] **Step 4.3**: 修改 `peri-tui/src/acp_server/notify.rs`

删除 `build_available_commands()` 函数定义（行 121-156），替换调用点。

**4.3a** — 修改 `send_available_commands_update()` 函数（行 79-98），将：
```rust
let commands = build_available_commands(skills);
```
替换为：
```rust
let commands = peri_acp::dispatch::build_available_commands(skills);
```

**4.3b** — 删除 `build_available_commands()` 函数定义。移除不再需要的 import：`use agent_client_protocol::schema::AvailableCommand;`（确认该类型是否在其他地方使用后决定是否删除）。

- [ ] **Step 4.4**: 修改 `peri-tui/src/acp_stdio.rs`

删除 `build_stdio_available_commands()` 函数定义（行 89-128）。

在 `session/new` handler（行 354）和任何其他使用 `build_stdio_available_commands` 的地方，将：
```rust
let cmds = build_stdio_available_commands(&skills);
```
替换为：
```rust
let cmds = dispatch::build_available_commands(&skills);
```

确认不再需要的 import（`use agent_client_protocol::schema::AvailableCommand;` 是否在别处使用）。

#### 检查步骤

- [ ] `cargo build -p peri-acp` 编译通过
- [ ] `cargo build -p peri-tui` 编译通过
- [ ] `cargo clippy -p peri-acp` 通过
- [ ] `cargo clippy -p peri-tui` 通过
- [ ] 确认 `notify.rs` 和 `acp_stdio.rs` 中没有残留的 `build_available_commands` 或 `build_stdio_available_commands` 定义
- [ ] `cargo test -p peri-acp --lib` 通过

---
