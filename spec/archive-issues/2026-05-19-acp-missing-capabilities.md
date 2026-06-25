> 归档于 2026-05-20，原路径 spec/issues/2026-05-19-acp-missing-capabilities.md

# ACP 协议能力缺失：Session 生命周期路由与通知变体

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-19
**调查日期**：2026-05-19
**完成日期**：2026-05-19

## 调查摘要

通过并行 explore 对 `peri-acp`、`peri-tui`、`peri-agent` 三个 crate 进行代码级验证。以下结论基于源码分析（具体文件和行号见各节）。

**已排除的误报**：
- `thought_message_chunk` — 已实现。`mapper.rs:25` 从 `AiReasoning` 事件映射为 `AgentThoughtChunk`
- ExecutorEvent 缺少 `plan`/`config_option_update`/`mode_update` — 这些通过其他机制传递（Todo rx channel、同步 ACP 请求），**无需新增事件变体**
- `fs/read_text_file` / `write_text_file` — 架构设计选择，非 bug。perihelion 是本地 Agent，通过 FilesystemMiddleware 直接操作 `std::fs`，不代理给 Client
- `terminal/*` — 同上。TerminalMiddleware 直接 `tokio::process::Command` 执行
- `authenticate` — 合理省略。perihelion 通过 peri config 直接配置 API Key

## 问题描述

TUI ACP Server（`peri-tui/src/acp_server.rs`）缺少 5 个 session 生命周期方法的路由处理，且初始化响应中的能力声明为空。同时 stdio 的 `SessionUpdate` 映射缺少 `user_message_chunk` 和 `available_commands_update` 变体。

`peri-acp/src/session/` 中的 SessionManager 已有 `new_session_with_id()`、`close_session()`、`list_sessions()` 等完整基础设施，但 **TUI ACP Server 的路由层未接通**。

## 症状详情

### 缺失的 Agent 方法（C→A）— TUI ACP Server 路由层

SessionManager 已有基础设施，仅缺 `acp_server.rs` 路由：

| 方法 | SessionManager 基础设施 | 路由状态 |
|------|----------------------|---------|
| `session/load` | `new_session_with_id()` + `load_thread_messages()` | ❌ 无路由 |
| `session/resume` | `new_session_with_id()` | ❌ 无路由 |
| `session/close` | `close_session()` | ❌ 无路由 |
| `session/list` | `list_sessions()` | ❌ 无路由 |
| `session/fork` | `new_session_with_settings()` | ❌ 无路由 |

### 能力声明缺失

`acp_server.rs:173` — `InitializeResponse` 使用 `AgentCapabilities::new()` 空构造，所有 session 能力默认 `false`。即使部分能力已通过 SessionManager 支持，也未声明。

### 缺失的 `session/update` 通知变体

当前已实现（`peri-acp/src/event/mapper.rs:map_executor_to_updates()`）：

| 变体 | 映射自 | 行号 |
|------|--------|------|
| `AgentMessageChunk` | `TextChunk` | :20 |
| `AgentThoughtChunk` | `AiReasoning` | :25 |
| `ToolCall` | `ToolStart` | :35 |
| `ToolCallUpdate` | `ToolEnd` | :52 |
| `UsageUpdate` | `LlmCallEnd`, `ContextWarning` | :64,74 |
| `SessionInfoUpdate` | `LlmRetrying` | :85 |

**ACP 标准中缺失的**：

| 变体 | 用途 | 优先级 |
|------|------|--------|
| `user_message_chunk` | 流式回显用户消息（session/load 重放时需要） | 依赖 session/load |
| `available_commands_update` | 斜杠命令变更通知 | 低 |

### fs/terminal 代理 — 远期增强（非当前缺失）

ACP 规范中 `fs/read_text_file`、`fs/write_text_file`、`terminal/*` 是面向**远程 Agent** 场景的 — Agent 运行在远端无法访问 Client 文件系统时，通过 Client 代理文件/终端操作。

perihelion 作为**本地 Agent**，FileSystemMiddleware 直接调用 `std::fs`，TerminalMiddleware 直接调用 `tokio::process::Command`，绕过 proxy 层是正确选择。**仅在通过 stdio 连接外部 IDE（如 JetBrains/Cursor）时才有实现意义** — IDE 期望 Agent 通过 ACP fs/terminal 请求代理，才能使文件变更反映到 IDE 的文件系统视图或终端面板中。

若后续支持此场景，需要：
1. 新建 `FsProxyTool` / `TerminalProxyTool` 替代工具，将 `invoke()` 改为通过 transport 发送 `AgentRequest` 并等待 Client 响应
2. EventSink 或新建 `ToolProxySink` 增加双向通信机制（当前 EventSink 单向 Agent→Client）
3. `acp_server.rs` 处理 Client 回执

## 涉及文件

- `peri-tui/src/acp_server.rs` — ACP Server 请求路由，添加 `session/load`/`resume`/`close`/`list`/`fork` 路由
- `peri-acp/src/event/mapper.rs` — `ExecutorEvent → SessionUpdate` 映射，添加 `user_message_chunk`/`available_commands_update`
- `peri-acp/src/session/state_builders.rs` — 初始化响应能力声明

## 参考

- [ACP Protocol Overview](https://agentclientprotocol.com/protocol/overview) — 方法/通知总表
- [ACP Session Setup](https://agentclientprotocol.com/protocol/session-setup) — session 生命周期
- [ACP Prompt Turn](https://agentclientprotocol.com/protocol/prompt-turn) — session/update 变体

## 实现记录

- **2026-05-19**: 完成 5 个 session 生命周期路由（load/list/close/resume/fork）和初始化能力声明
  - `a87a8c3` feat(acp): declare session capabilities in initialize response
  - `c9c8390` feat(acp): add session/load route handler
  - `156943f` feat(acp): add session/list, close, resume, fork route handlers

> **2026-05-19 更新**: 核心 5 个 session 路由已在本期完成后实现。`UserMessageChunk` 依赖 session/load 通知重放机制，`AvailableCommandsUpdate` 依赖命令变更事件源，两项均属低优先级增强，留待后续。
