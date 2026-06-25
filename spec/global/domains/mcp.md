# MCP 领域

## 领域综述

MCP（Model Context Protocol）领域负责 Peri 与外部 MCP 服务器的集成能力。作为 MCP Client，支持 stdio 和 Streamable HTTP 两种传输协议，将外部服务器的工具和资源动态注入到 ReAct 循环中。

核心职责：

- MCP 服务器连接池管理（McpClientPool）
- 工具桥接（mcp__{server}__{tool} 命名）
- 资源读取（mcp_read_resource 工具）
- 双层配置合并（全局 settings.json + 项目 .mcp.json）
- 运行时管理（/mcp 面板、状态查看、重连、删除）
- OAuth 2.0 认证（Authorization Code + PKCE、Token 持久化）

边界：不涉及内置工具（Filesystem、Terminal 等由各自中间件提供）、不涉及 LLM 适配。

## 核心流程

### MCP 连接池初始化

```
App::new() 创建
  → spawn_mcp_init() 后台 task
    → McpConfig::load_merged_config(cwd)
    → McpClientPool::run_initialize()
      → 对每个 server: stdio spawn / HTTP connect
      → 成功 → Ready + 工具发现
      → 失败 → Failed（不阻塞其他 server）
  → submit_message() 时如未就绪 → 异步等待（30s）
```

### 工具调用流程

```
LLM 生成 mcp__{server}__{tool} 调用
  → McpToolBridge.invoke()
    → McpClientPool.call_tool(server, tool, args)
      → MCP 协议 tools/call 请求
      → 返回 ToolResult
```

### OAuth 2.0 认证流程

```
MCP HTTP 请求 → 401 + WWW-Authenticate
  → OAuthFlowManager.handle_401()
    → discover_metadata() → 授权服务器信息
    → DCR（可选）→ 动态注册客户端
    → start_authorization() → PKCE + CSRF
    → TUI 展示授权 URL + 打开浏览器
    → 本地回调服务器 / 手动粘贴 code
    → token_exchange() → access_token + refresh_token
    → 持久化到 ~/.peri/oauth_tokens.json（0600）
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| MCP crate | rmcp 1.6.0（本地 patch 修复 UnexpectedContentType） |
| 传输层 | stdio（子进程）+ Streamable HTTP（远程） |
| 连接池 | McpClientPool，惰性初始化，stdio 10s/HTTP 30s 超时 |
| 工具命名 | `mcp__{server}__{tool}`，HITL 默认需审批 |
| 配置合并 | 全局 settings.json + 项目 .mcp.json，同名覆盖 |
| 环境变量 | `${VAR}` 占位符自动展开 |
| OAuth | rmcp auth feature + AuthClient，PKCE，Token 刷新 |
| Token 持久化 | ~/.peri/oauth_tokens.json（0600 权限） |
| 运行时管理 | /mcp 面板：状态查看、重连、删除、工具/资源详情 |
| 后台初始化 | spawn_mcp_init()，不阻塞 TUI |

## Feature 附录

### feature_20260502_F001_mcp-middleware

**摘要:** MCP Client 中间件实现（stdio/HTTP 传输、工具桥接、双层配置合并）
**关键决策:**

- McpMiddleware 实现 Middleware trait，collect_tools 注入 MCP 工具
- mcp__{server}**{tool} 命名格式，HITL 对 mcp** 前缀默认需审批
- 双层配置：全局 settings.json + 项目 .mcp.json，同名覆盖
- McpClientPool 连接池：惰性初始化，单服务器失败不影响其他
- mcp_read_resource 工具暴露 MCP 资源
- rmcp 1.6.0 本地 patch 修复 UnexpectedContentType
**归档:** [链接](../../archive/feature_20260502_F001_mcp-middleware/)
**归档日期:** 2026-05-04

### feature_20260502_F002_mcp-management

**摘要:** MCP 连接池后台初始化 + /mcp 运行时管理面板
**关键决策:**

- spawn_mcp_init() 后台 task，不阻塞 TUI 渲染和用户输入
- submit_message() 时异步等待 MCP 就绪（30s）
- /mcp 面板：服务器状态、工具/资源详情、重连、持久删除
- McpPanelView 枚举（Browse/Tools/Resources 三个视图）
**归档:** [链接](../../archive/feature_20260502_F002_mcp-management/)
**归档日期:** 2026-05-04

### feature_20260503_F001_mcp-oauth-auth

**摘要:** MCP HTTP 传输层集成 OAuth 2.0（PKCE + Token 持久化 + 混合回调）
**关键决策:**

- rmcp auth feature 启用，AuthClient 集成到 HTTP 传输层
- Authorization Code + PKCE 完整 OAuth 流程
- 401 + WWW-Authenticate 自动触发 OAuth 授权
- Token 持久化 ~/.peri/oauth_tokens.json（0600）
- 混合回调：本地 HTTP 回调服务器 → 回退 TUI 手动粘贴
- OAuthFlowManager + OAuthCallbackServer + OAuthConfig
- 已知 TODO：scope 升级处理、启动时文件权限检查
**归档:** [链接](../../archive/feature_20260503_F001_mcp-oauth-auth/)
**归档日期:** 2026-05-04

### feature_20260507_F001_plugin-mcp-injection

**摘要:** 插件 MCP 环境变量 per-plugin 展开，pluginSource 旁路表
**关键决策:**

- MCP env 展开时机：per-plugin 独立展开，先展开后合并（避免合并后同名 key 冲突）
- pluginSource 旁路表：`McpClientPool.plugin_sources: HashMap<String, String>` 记录 `"plugin@{marketplace}"` 来源
- load_merged_config() 重排：step 2 中每个 plugin server 先独立展开，step 6 仅处理 project/global
- 去重 hash 基于展开后的实际值（更准确）
- 零 breaking change：不改 `McpServerConfig` / `ConfigSource` / `LoadedPlugin`
**归档:** [链接](../../archive/feature_20260507_F001_plugin-mcp-injection/)
**归档日期:** 2026-05-13

---

## Issue 经验附录

### issue_2026-05-27-cross-platform-spawn-wrapper
**摘要:** Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装
**状态:** Fixed
**归档日期:** 2026-05-27
**关键词:** cross-platform spawn, shell_command, Windows cmd, MCP transport, hooks executor
**问题本质:** Windows 不能直接 spawn .cmd 批处理脚本（如 npx），需通过 shell 包裹。项目 3 处 spawn 调用点各自处理，无统一封装
**通用模式:** 所有子进程 spawn 必须通过平台感知的统一 wrapper：Windows 用 `cmd /C`，Unix 用 `bash -c`。不应让调用者自己处理平台差异
**技术决策:** 自己封装而非引入第三方 crate，提供 `shell_command()` builder + `spawn_shell()` 快捷函数两层 API
**涉及文件:** peri-middlewares/src/process/mod.rs, peri-middlewares/src/mcp/client.rs, peri-middlewares/src/middleware/terminal.rs, peri-middlewares/src/hooks/executor.rs
**CLAUDE.md 链接:** true

---

## 相关 Feature

- → [agent.md](./agent.md) — MCP 工具通过 BaseTool trait 注册到 ReAct 循环
- → [tui.md](./tui.md) — /mcp 面板 UI、OAuth 授权引导界面
