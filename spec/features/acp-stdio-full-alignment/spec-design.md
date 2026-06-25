# ACP Stdio 全量方法对齐 设计方案

## 目标

将 ACP stdio 路径缺失的 8 个 session 生命周期方法补齐，并将可复用的业务逻辑提取到 `peri-acp/src/dispatch/` 层，使 TUI 和 stdio 双路径共享同一套 dispatch 函数。

## 背景

ACP 协议在 `initialize` 阶段声明了完整的 session 能力（loadSession、sessionCapabilities.list/close/resume/fork），但 stdio 路径实际只实现了 `session/new`、`session/list`、`session/prompt`、`session/cancel` 四个核心方法，其余 8 个方法仅在 TUI 路径（`acp_server/requests.rs` + `compact.rs`）有实现。

此外，`build_available_commands()` 函数在 TUI (`notify.rs:121`) 和 stdio (`acp_stdio.rs:89`) 中各自独立实现，代码完全相同。

## 当前状态

### stdio 路径已实现的方法（`acp_stdio.rs`）

| 方法 | 方式 | 行号 |
|------|------|------|
| initialize | 通过 dispatch::build_initialize_response() | 266-268 |
| session/new | 内联（含 freeze 逻辑 + commands 通知） | 273-364 |
| session/list | 通过 dispatch::list_sessions_as_info() | 366-388 |
| session/prompt | 内联（调用 executor::execute_prompt） | 390-504 |
| session/set_mode | 内联 | 506-528 |
| session/set_model | 内联 | 530-557 |
| session/set_config_option | 内联 | 559-613 |
| session/cancel | 通知处理器 | 615-631 |

### stdio 路径缺失的方法

| 方法 | TUI 实现位置 | 复杂度 | 可提取到 dispatch 的业务逻辑 |
|------|-------------|--------|--------------------------|
| `session/set_thinking` | `requests.rs:204-231` | 低 | `apply_thinking_effort()` 已存在于 `state_builders.rs`；需额外处理 `enabled` 标志 |
| `session/load` | `requests.rs:233-293` | 中 | `thread_store.load_messages()` → `Vec<BaseMessage>` |
| `session/close` | `requests.rs:328-343` | 低 | 取消 token + 从 sessions map 移除 |
| `session/clear` | `requests.rs:345-353` | 低 | 清空 history |
| `session/resume` | `requests.rs:356-387` | 低 | 检查 session 是否存在 + 创建 entry |
| `session/fork` | `requests.rs:389-441` | 中 | `thread_store.create_thread()` + `thread_store.append_messages()` |
| `session/compact` | `compact.rs` | 高 | `full_compact()` + `re_inject()` — 强依赖 EventSink/provider/transport |
| `$/cancel_request` | `notify.rs:24-38` | 低 | `cancel_token.cancel()` |

### 现有 dispatch 模块

- `dispatch/init.rs` — `build_initialize_response()`
- `dispatch/list_sessions.rs` — `list_sessions_as_info()`

### 架构约束

1. **两种 transport 的 session 类型不同**：TUI 用 `SessionState`（`acp_server/mod.rs:39`），stdio 用 `SessionInfo`（`acp_stdio.rs:7`）。两者字段相同但无法互通。dispatch 函数不能直接操作 sessions map，只能操作 `ThreadStore` 并返回纯数据。
2. **stdio 使用 `agent_client_protocol` builder 模式**：通过 `.on_receive_request()` 注册强类型 handler（含 `responder` + `cx: ConnectionTo<Client>`），与 TUI 的 `match method` 模式不同。
3. **session/compact 强依赖 transport**：需要 `EventSink`（发送 CompactStarted/CompactCompleted 事件）、`provider`（LLM 模型）、`peri_config`（compact 配置），不适合提取为纯数据函数。
4. **重复代码**：`build_available_commands()` 在 TUI（`notify.rs:121`）和 stdio（`acp_stdio.rs:89`）完全重复。

## 方案设计

### 原则

1. **dispatch 层只做纯业务逻辑**——操作 ThreadStore + 返回纯数据，不依赖 session map 类型
2. **简单逻辑保持内联**——close/clear/resume/cancel 业务逻辑 1-2 行，不值得为纯数据函数包装
3. **消除重复**——`build_available_commands()` 提取到 dispatch

### 新增 dispatch 模块

#### 1. `dispatch/session_load.rs`

```
load_session_messages(thread_store, thread_id) -> Vec<BaseMessage>
```

- 调用 `thread_store.load_messages()`
- 线程不存在时返回空 Vec（带 warn 日志）

#### 2. `dispatch/session_fork.rs`

```
fork_session(thread_store, source_id, cwd) -> Result<(new_thread_id, copied_messages)>
```

- 调用 `thread_store.create_thread(ThreadMeta::new(cwd))` 创建新线程
- 调用 `thread_store.append_messages(&new_id, &source_messages)` 复制消息
- 返回 `(new_thread_id, source_messages)` 供 transport 层存入 sessions map

#### 3. `dispatch/commands.rs`

```
build_available_commands(skills: &[SkillMetadata]) -> Vec<AvailableCommand>
```

- 合并 TUI 的 `build_available_commands()` 和 stdio 的 `build_stdio_available_commands()`（内容完全相同）

### stdio 路径新增 handler（按复杂度排序）

| 序号 | 方法 | 实现策略 |
|------|------|---------|
| 1 | session/set_thinking | 使用现有 `apply_thinking_effort()` + 直接操作 `ctx.peri_config.write()` |
| 2 | $/cancel_request | 通知处理器，从 `ctx.sessions.read()` 找 token 并 cancel |
| 3 | session/close | 从 `ctx.sessions.write()` 移除并 cancel token |
| 4 | session/clear | 从 `ctx.sessions.write()` 清空 history |
| 5 | session/resume | 检查 `ctx.sessions`，不存在则插入空 entry |
| 6 | session/load | 调用 `dispatch::load_session_messages()` + 插入 sessions map + 返回 LoadSessionResponse |
| 7 | session/fork | 调用 `dispatch::fork_session()` + 插入 sessions map + 返回 ForkSessionResponse |
| 8 | session/compact | 需要 provider + config + event_sink。实现方式：从 sessions map 读取 history，调用 `full_compact()` + `re_inject()`，通过 `StdioEventSink` 推送事件，更新 history。**不提取到 dispatch**（强依赖 transport） |

### TUI 路径重构

将 `requests.rs` 中的以下 handler 改用统一的 dispatch 函数：

- `session/load`（行 233）→ 使用 `dispatch::load_session_messages()`
- `session/fork`（行 389）→ 使用 `dispatch::fork_session()`
- `notify.rs:build_available_commands()` → 使用 `dispatch::build_available_commands()`
- `acp_stdio.rs:build_stdio_available_commands()` → 使用 `dispatch::build_available_commands()`

### session/compact 的 stdio 实现差异

TUI 路径的 `compact.rs` 使用 `TransportEventSink` 推送 `CompactStarted`/`CompactCompleted` 事件。stdio 路径可直接使用 `StdioEventSink`（已用于 prompt path），但需注意：

- compact 不需要完整的 `execute_prompt()` 循环，只需一次性的 `full_compact() + re_inject()`
- 事件 sink 不绑定到特定 session/prompt 生命周期
- 结果直接更新 `ctx.sessions` 中的 history

### version_info 常量重复

`dispatch/init.rs` 中 `ProtocolVersion::V1` 用于 InitializeResponse。无重复问题。

## 影响范围

### 新增文件

| 文件 | 内容 |
|------|------|
| `peri-acp/src/dispatch/session_load.rs` | `load_session_messages()` |
| `peri-acp/src/dispatch/session_fork.rs` | `fork_session()` |
| `peri-acp/src/dispatch/commands.rs` | `build_available_commands()` |

### 修改文件

| 文件 | 变更 |
|------|------|
| `peri-acp/src/dispatch/mod.rs` | 注册 3 个新模块 |
| `peri-tui/src/acp_stdio.rs` | 新增 8 个 handler + 使用 dispatch::build_available_commands |
| `peri-tui/src/acp_server/requests.rs` | session/load、session/fork 改用 dispatch 函数 |
| `peri-tui/src/acp_server/notify.rs` | 删除 `build_available_commands()`，改用 dispatch |
| `docs/ACP_COMPATIBLE.csv` | 更新 stdio 列状态（8 项 NA → ✅） |

## 风险

1. **session/compact** 在 stdio 路径首次实现，没有 TUI 的 `handle_compact_completed` UI 刷新逻辑（stdio 不需要 UI），但事件推送可能触发未知的 client 行为
2. **session/fork** 的 dispatch 函数返回 `(thread_id, Vec<BaseMessage>)`，TUI 路径需要额外处理 `SessionState` 的其他字段（frozen_* 为 None），与现有 TUI 实现一致
3. **build_available_commands() 重复消除**——修改位置涉及 TUI notify.rs 和 acp_stdio.rs 两处调用点，需同步更新
