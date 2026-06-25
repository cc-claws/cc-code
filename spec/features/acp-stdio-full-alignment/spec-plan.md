# ACP Stdio 全量方法对齐 实施计划

> **Goal:** 将 ACP stdio 路径缺失的 8 个 session 生命周期方法补齐，并将 `build_available_commands()`、`load_session_messages()`、`fork_session()` 等可复用业务逻辑提取到 `peri-acp/src/dispatch/` 层。

**Architecture:** dispatch 层只做纯数据操作（ThreadStore 查询、消息复制、命令列表构建），不依赖 transport 特定的 session map 类型。简单 handler（close/clear/resume/cancel）保持内联。

**Tech Stack:** Rust (2021 edition), `agent_client_protocol` builder 模式, `peri-agent` ThreadStore, tokio async

**Design Doc:** `spec/features/acp-stdio-full-alignment/spec-design.md`

---

## 改动总览

- **3 个新增 dispatch 模块**：`session_load.rs`、`session_fork.rs`、`commands.rs`
- **8 个新增 stdio handler**：session/set_thinking、$/cancel_request、session/close、session/clear、session/resume、session/load、session/fork、session/compact（在 `acp_stdio.rs` 中）
- **3 处 TUI 重构**：`requests.rs` 中 session/load、session/fork 改用 dispatch 函数；`notify.rs` 中 `build_available_commands()` 替换为 dispatch 版本；`acp_stdio.rs` 中 `build_stdio_available_commands()` 替换为 dispatch 版本
- **1 个 CSV 更新**：`docs/ACP_COMPATIBLE.csv` 中 8 项 NA → ✅

---

## 任务索引

### Task 1: 新增 dispatch/commands.rs — 统一 build_available_commands
📄 详情见: `spec-plan-task-1.md`

提取 `build_available_commands()` 到 dispatch 层，消除 TUI（`notify.rs`）和 stdio（`acp_stdio.rs`）中的重复实现。

### Task 2: 新增 dispatch/session_load.rs — load_session_messages
📄 详情见: `spec-plan-task-2.md`

提取 `thread_store.load_messages()` 调用为纯数据函数。

### Task 3: 新增 dispatch/session_fork.rs — fork_session
📄 详情见: `spec-plan-task-3.md`

提取 `thread_store.create_thread()` + `append_messages()` 为纯数据函数。

### Task 4: 注册 dispatch 模块 + 重构 TUI 调用点
📄 详情见: `spec-plan-task-4.md`

在 `dispatch/mod.rs` 注册新模块；重构 `requests.rs` session/load、session/fork handler；重构 `notify.rs` 和 `acp_stdio.rs` 的 commands 构建。

### Task 5: stdio 新增 session/set_thinking + $/cancel_request handler
📄 详情见: `spec-plan-task-5.md`

在 `acp_stdio.rs` 中添加两个简单 handler：set_thinking 使用现有 `apply_thinking_effort()`；cancel_request 发送通知取消 token。

### Task 6: stdio 新增 session/close + session/clear + session/resume handler
📄 详情见: `spec-plan-task-6.md`

三个简单 handler 的内联实现。

### Task 7: stdio 新增 session/load + session/fork handler
📄 详情见: `spec-plan-task-7.md`

使用 dispatch 函数的 handler 实现。

### Task 8: stdio 新增 session/compact handler
📄 详情见: `spec-plan-task-8.md`

使用 `full_compact()` + `re_inject()` + `StdioEventSink` 的 compact handler 实现。

### Task 9: 更新 ACP_COMPATIBLE.csv
📄 详情见: `spec-plan-task-9.md`

将 8 个修复项的 stdio_transport 列从 NA 更新为 ✅。

### Acceptance Task
📄 详情见: `spec-plan-acceptance.md`

构建验证 + 检查 ACP 兼容性矩阵 + 确认 handler 注册完整性。
