> 归档于 2026-05-24，原路径 spec/issues/2026-05-21-acp-stdio-missing-session-capabilities.md

# ACP Stdio InitializeResponse 缺少 session 能力声明，Zed 客户端报错

**状态**：Fixed  
**优先级**：高  
**创建日期**：2026-05-21  
**修复日期**：2026-05-21

## 问题描述

当 Zed IDE 客户端通过 ACP stdio 协议连接 peri 时，Zed 显示错误信息："Loading or resuming sessions is not supported by this agent."。Relay 客户端侧边栏调用 `session/list` 时返回 "Method not found"。

## 根因

`peri-tui/src/acp_stdio.rs` 的 `initialize` 处理器只声明了 `promptCapabilities`，完全遗漏了 `load_session` 和 `session_capabilities`。

`session/list` handler 在 stdio 路径也缺失，导致 Relay 客户端侧边栏无法加载会话列表。

## 修复

提取 `build_initialize_response()` 到 `peri-acp/src/dispatch/init.rs`，stdlib/TUI 双路径复用：

```rust
AgentCapabilities::new()
    .load_session(true)
    .prompt_capabilities(PromptCapabilities::new())
    .session_capabilities(
        SessionCapabilities::new()
            .list(SessionListCapabilities::new())
            .close(SessionCloseCapabilities::new())
            .resume(SessionResumeCapabilities::new())
            .fork(SessionForkCapabilities::new()),
    )
```

新增 `peri-acp/src/dispatch/list_sessions.rs`，在 stdio 路径注册 `session/list` handler。

## 涉及文件

| 文件 | 操作 |
|------|------|
| `peri-acp/src/dispatch/init.rs` | 新增 — `build_initialize_response()` |
| `peri-acp/src/dispatch/list_sessions.rs` | 新增 — `list_sessions_as_info()` |
| `peri-acp/src/dispatch/mod.rs` | 修改 — 注册两个模块 |
| `peri-tui/src/acp_stdio.rs` | 修改 — `initialize` 使用 dispatch 函数；新增 `session/list` handler |
| `peri-tui/src/acp_server/requests.rs` | 修改 — `initialize` 使用 dispatch 函数 |
