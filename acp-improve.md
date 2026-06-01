# ACP Stdio / TUI 代码冗余性分析与改进方案

## 一、已共享的部分 ✓

以下组件已妥善共享在 `peri-acp` crate 中：

| 组件 | 位置 | 共享方式 |
|------|------|----------|
| `execute_prompt()` | `peri-acp/src/session/executor.rs` | 纯函数，两条路径都调用 |
| `build_agent()` | `peri-acp/src/agent/builder.rs` | 同上 |
| `build_system_prompt()` | `peri-acp/src/prompt/mod.rs` | 同上 |
| `dispatch::*` 四个纯函数 | `peri-acp/src/dispatch/` | init / list / load / fork |
| `build_config_options()` / `build_mode_state()` / `build_model_state()` | `peri-acp/src/session/state_builders.rs` | 同上 |
| `event::mapper::map_event()` | `peri-acp/src/event/mapper.rs` | ExecutorEvent→SessionUpdate |
| Command 系统 | `peri-acp/src/session/command/` | compact/clear |

---

## 二、高度冗余的部分

### 1. Session 状态结构体 —— 两份几乎一致的 struct

| 字段 | `acp_stdio.rs:7-28` SessionInfo | `acp_server/mod.rs:38-57` SessionState |
|------|:---:|:---:|
| session_id | ✓ | ✓ |
| thread_id | ✓ | ✓ |
| cwd | ✓ | ✓ |
| history | ✓ | ✓ |
| cancel_token | ✓ | ✓ |
| frozen_system_prompt | ✓ | ✓ |
| frozen_claude_md | ✓ | ✓ |
| frozen_claude_local_md | ✓ | ✓ |
| frozen_skill_summary | ✓ | ✓ |
| frozen_date | ✓ | ✓ |
| frozen_language | ✓ | ✓ |
| agent_pool | ✓ | ✓ |
| recall_items | ✗ | ✓ (TUI 专属) |

**差异只有 `recall_items`**。应提取到 `peri-acp` 作为共享结构体。

### 2. `session/new` —— 冻结数据创建逻辑完全重复

位置：`acp_stdio.rs:255-351` vs `acp_server/requests.rs:56-142`（约 80 行）

重复的步骤：

1. `ThreadMeta::new` → `thread_store.create_thread` → thread_id
2. `chrono::Local::now()` → frozen_date
3. `peri_config.read().config.language` → frozen_language
4. `AgentsMdMiddleware::read_frozen_content` → frozen_claude_md / frozen_claude_local_md
5. `SkillsMiddleware::build_frozen_summary` → frozen_skill_summary
6. `build_system_prompt` → frozen_system_prompt
7. Insert session into map
8. `build_mode_state` / `build_model_state` / `build_config_options`
9. `resolve_dirs_static` + `list_skills` → `build_available_commands` → send notification
10. Respond `NewSessionResponse`

最大差异：TUI 用 `AcpTransport` trait，stdio 用 SDK `ConnectionTo<Client>`。

### 3. `session/set_config_option` / `session/set_model` / `session/set_mode`

| 方法 | stdio 位置 | TUI 位置 | TUI 独有差异 |
|------|------------|----------|------|
| `session/set_model` | `acp_stdio.rs:586-614` | `requests.rs:144-163` | `persist_config()` + 更新 `active_alias` |
| `session/set_mode` | `acp_stdio.rs:562-584` | `requests.rs:166-179` | — |
| `session/set_config_option` | `acp_stdio.rs:615-670` | `requests.rs:181-236` | `context_1m` 处理 + `persist_config()` |

核心差异：

- TUI 每条都调 `persist_config()` 持久化到磁盘（stdio 不需要——config 由 IDE client 持有）
- TUI 的 `set_config_option` 额外处理 `context_1m`
- "mode"/"model"/"thinking_effort" 三种 config 处理逻辑两边完全一致
- 两边都重复构造 `build_config_options → ConfigOptionUpdate → send_notification` 这个通知推送模式

### 4. `session/load` —— 逻辑完全一致

`acp_stdio.rs:744-820`（76 行） vs `requests.rs:238-292`（54 行）。

**差异**：TUI `session/load` **缺少 AvailableCommands 推送**（疑似遗漏 bug），其余完全一致。

### 5. `session/resume` / `session/fork` / `session/close`

这三种方法两边逻辑几乎逐行对应，区别仅在于 session map 类型不同（`HashMap` vs `tokio::sync::Mutex<HashMap>`）和 transport API 不同。

### 6. `session/list`

stdio 用 `dispatch::list_sessions_as_info()`，TUI 把相同逻辑 inline 了（`requests.rs:294-325`）。应统一使用 dispatch 函数。

### 7. Context 初始化

`acp_stdio.rs:96-219`（约 120 行）和 TUI 路径（分散在 `main.rs` + `acp_server/mod.rs`）都做了几乎一样的初始化：

| 初始化项 | stdio | TUI |
|----------|-------|-----|
| peri_config 加载 | `PeriConfig::load()` | 同 |
| provider 创建 | `LlmProvider::from_config` | 同 |
| cron_scheduler | `Arc<Mutex<CronScheduler>>` | 同 |
| MCP pool | `McpClientPool::run_initialize` 后台 | 同 |
| plugin 数据 | `load_enabled_plugins_aggregated` | 同 |
| hook_groups | 本地 hooks + plugin hooks | 同 |
| tool_search_index | `Arc<ToolSearchIndex::new>` | 同 |
| shared_tools | `Arc<RwLock<HashMap>>` | 同 |
| thread_store | `SqliteThreadStore::default_path` | 同 |
| langfuse | `LangfuseSession::from_env` | 同 |

差异仅在于最终包装类型：stdio 用 `StdioContext`，TUI 用 `AcpServerConfig`。

### 8. 其他重复点

- **`send_config_option_update`**：TUI 有 helper 函数，stdio 每次手写 5 行
- **`send_available_commands_update`**：同上
- **`push_done` 后的 `SessionInfoUpdate`**：`acp_stdio.rs:549-552` 手动做，TUI 在 `send_session_info_update`
- **`extract_session_id`**：TUI 有 helper，stdio 直接用 `.0.to_string()`

---

## 三、冗余度量

| 指标 | 数值 |
|------|------|
| 涉及文件 | `acp_stdio.rs`（913 行） + `acp_server/requests.rs`（464 行） |
| 方法级重复 | 9 个 ACP 方法中有 8 个高度重复（`update_config` 仅 TUI 有） |
| 估算重复代码行数 | **约 400-500 行**（占两条路径总体的 30-40%） |
| 结构体重复 | `SessionInfo` ↔ `SessionState`（14/15 字段相同） |
| 初始化重复 | stdio 约 120 行 ↔ TUI 约 100 行 |

---

## 四、根本原因

两条路径的 ACP handler 都在手写裸 handler（TUI 用 raw JSON params match，stdio 用 SDK 闭包），缺乏一个共享的请求处理层。当前 `dispatch/` 模块只抽了四个纯函数（init/list/load/fork），但不包含：

- session 创建时的冻结逻辑
- config 更新逻辑
- 通知推送逻辑

本质矛盾：TUI 用自研 `AcpTransport` trait（`send_notification`/`send_response`），stdio 用 SDK 的 `ConnectionTo<Client>`（`cx.send_notification`/`responder.respond`），两个 transport 接口不统一导致 handler 代码无法复用。

---

## 五、改进方案

### 方案 A（推荐，轻量）：扩展 dispatch 模块

在 `peri-acp/src/dispatch/` 下新增一组纯函数，封装每个 ACP 方法的 session 操作逻辑：

```
dispatch/
├── mod.rs
├── commands.rs          # build_available_commands (已有)
├── init.rs              # build_initialize_response (已有)
├── list_sessions.rs     # list_sessions_as_info (已有)
├── session_load.rs      # load_session_messages (已有)
├── session_fork.rs      # fork_session (已有)
├── session_new.rs       # [新增] create_frozen_session(cwd, ...) → FrozenSessionState
├── session_close.rs     # [新增] close_session(sessions, session_id)
├── session_resume.rs    # [新增] resume_session(sessions, session_id, cwd)
├── config_option.rs     # [新增] apply_config_option(config_id, value, ...) → Vec<SessionConfigOption>
```

每条路径的 handler 只负责 transport 适配（parsing → call dispatch → respond/notify），业务逻辑全部在 dispatch 层。

**优点**：
- 改动小，风险低
- 已验证 dispatch 模式可行（现有 4 个函数已在两条路径复用）
- 不改变 transport 抽象层

**缺点**：
- handler 中 transport 适配代码仍有少量残留

### 方案 B（完整）：统一 Transport 抽象

抽象 `AcpSessionHandler` trait，统一 transport 接口：

```rust
trait SessionHandler {
    async fn send_response(&self, id: RequestId, result: Value);
    async fn send_session_notification(&self, session_id: &str, update: SessionUpdate);
}
```

将 8 个 handler 逻辑统一到 `peri-acp` crate。两条路径只提供 trait 实现。

**优点**：完全消除 handler 重复
**缺点**：需要抽象 stdio SDK 的 `responder`/`ConnectionTo` != TUI 的 `AcpTransport`，有一定设计复杂度

### 建议顺序

1. **第一步**（低风险）：实施方案 A，补齐 dispatch 函数
2. **第二步**（bugfix）：修复 TUI `session/load` 缺少 AvailableCommands 推送
3. **第三步**（可选，后续迭代）：实施方案 B 统一 transport 抽象

---

## 六、Bug 发现：TUI `session/load` 缺少 AvailableCommands 推送

```rust
// acp_stdio.rs:805-815 — stdio session/load 有
let skill_dirs = peri_middlewares::SkillsMiddleware::resolve_dirs_static(
    &cwd_for_skills,
    &ctx.plugin_skill_dirs,
);
let skills = peri_middlewares::skills::list_skills(&skill_dirs);
let cmds = dispatch::build_available_commands(&skills);
let ac_notif = SessionNotification::new(
    SessionId::new(&*sid),
    SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(cmds)),
);
let _ = cx.send_notification(ac_notif);

// requests.rs:238-292 — TUI session/load 没有！只做了 respond(LoadSessionResponse)
```

影响：IDE client 通过 stdio load session 后会收到命令列表更新，但 TUI internal client load session 后不会。建议在方案 A 中一并修复。
