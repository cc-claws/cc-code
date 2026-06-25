# ACP 未实现 MCP-over-ACP 传输（mcp/connect、mcp/message、mcp/disconnect）

**状态**：Open
**优先级**：低
**创建日期**：2026-05-16

## 问题描述

MCP-over-ACP 允许 IDE 端托管 MCP 服务器，通过 ACP 通道将 MCP 消息中继给 Agent 端。当前 perihelion 的 MCP 连接完全在 Agent 内部发起（`McpMiddleware` 直连 stdio/HTTP/SSE），不支持通过 ACP 隧道承载 MCP。

## 症状详情

三种方法均未实现（`unstable_mcp_over_acp` feature gate）：

| 方法 | 方向 | 用途 |
|------|------|------|
| `mcp/connect` | Client → Agent | IDE 告知 Agent 可用的 MCP 服务器列表 |
| `mcp/message` | 双向 | 隧道中继 MCP JSON-RPC 消息 |
| `mcp/disconnect` | Client → Agent | IDE 断开指定 MCP 连接 |

## 现状

- perihelion 的 `McpMiddleware` 直接管理 MCP 连接（stdio/HTTP/SSE）
- `build_bare_agent()` 中 MCP pool 通过 `mcp_pool` 参数传入
- ACP 路径当前 `mcp_pool: None`，无 MCP 工具

## 设计考量

MCP-over-ACP 的数据流方向与当前 `McpMiddleware` 相反：

```
当前 McpMiddleware（Agent 端直连）:
  Agent → 直接连接 → MCP Server (stdio/HTTP/SSE)
  Agent 管理连接生命周期，收 tool list

MCP-over-ACP（IDE 端托管）:
  IDE → 托管 MCP Server → ACP 隧道 → Agent
  IDE 管理连接，通过 mcp/connect 告知 Agent 有哪些工具
```

**两种方案**：

| 方案 | 描述 | 复杂度 |
|------|------|--------|
| A. 新 `McpOverAcpMiddleware` | 独立中间件，收 `mcp/connect` → 动态注册工具到 executor，`mcp/message` 转发到 ACP conn | 中 |
| B. 扩展 `McpMiddleware` | 给现有中间件加 "acp" transport 模式，复用工具注册/生命周期逻辑 | 高 |
| C. ACP 层直接处理 | `handle_mcp_connect` 直接调用 `executor.register_tool()`，不经过中间件 | 低（但破坏抽象） |

**推荐方案 A**：新增 `McpOverAcpMiddleware`（约 150 行），实现 `Middleware` trait，在 `before_agent` 时持有 `conn: ConnectionTo<Client>` 引用用于 `mcp/message` 转发。

**关键接口**：
- `McpOverAcpMiddleware::new(conn: ConnectionTo<Client>)` 
- `handle_mcp_connect` → 解析 server list → 调用 mw 注册工具
- `handle_mcp_message` → 转发到 mw → conn.send_request(mcp/message)
- `handle_mcp_disconnect` → mw 移除工具

**与 build_bare_agent 集成**：ACP 路径的 `assemble_agent()` 需要在构建 `BareAgentConfig` 时提供 `mcp_pool: None`（不使用直连 pool），然后在 `ReActAgent` 构建完成后通过 `.add_middleware(Box::new(McpOverAcpMiddleware::new(conn)))` 注入。
