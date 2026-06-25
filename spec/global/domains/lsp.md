# LSP 领域

## 领域综述

LSP 客户端库（peri-lsp）和 LSP 中间件，提供代码智能功能（定义跳转、引用查找、hover 等）及文件变更同步。

## 核心流程

（后续通过 issue 归档逐步填充）

## 技术方案总结

| 维度 | 选型 |
|------|------|
| LSP 客户端 | peri-lsp 独立 crate，基于 jsonrpc/stdio transport |
| 连接管理 | LspServerPool 按文件扩展名路由，支持多服务器并行 |
| 中间件集成 | LspMiddleware 注册 LSP 工具，after_tool 自动同步文件变更 |

---

## Issue 经验附录

### issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit

**摘要:** LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** on_error 回调, LSP 重连, parking_lot::MutexGuard !Send, transport 断开
**问题本质:** 三个缺陷：(1) run_dispatch_loop 退出时未调用 on_error 回调，ServerState 永远停在 Running；(2) LspTool 无自动重连逻辑，ensure_server_for_file 因 initialized 集合短路；(3) try_restart 持有 parking_lot::MutexGuard 跨 .await 导致 !Send
**通用模式:** 外部进程管理的连接必须处理进程退出通知（stdout EOF），更新连接状态。重连逻辑需要清理"已初始化"缓存，否则会短路跳过重启。parking_lot::MutexGuard 不能跨 .await 持有——这是 Rust 异步编程的经典陷阱，应提取同步方法持有锁
**技术决策:** transport 层新增 invoke_on_error；LspTool 检测 Error/Stopped 状态自动重连；提取 check_and_increment_restart() 同步方法消除 !Send
**涉及文件:** peri-lsp/src/jsonrpc/transport.rs, peri-lsp/src/client.rs, peri-lsp/src/pool.rs, peri-lsp/src/error.rs, peri-middlewares/src/lsp/tool.rs
**CLAUDE.md 链接:** true

## 相关 Feature
