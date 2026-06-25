# Feature: 20260502_F001 - mcp-middleware

## 需求背景

Peri agent 目前仅使用内置工具（Filesystem、Terminal、SubAgent 等），工具集固定且有限。MCP（Model Context Protocol）生态中已有大量第三方服务器（filesystem、database、web scraper、GitHub 等），支持 MCP Client 能力可以让 Peri 无限扩展工具集，与 Claude Code、Cursor 等 AI 工具站在同一生态位。

项目已有 ACP（Agent Client Protocol）实现用于 IDE-Agent 通信，但 ACP 与 MCP 是两个不同维度的协议。ACP 解决编辑器如何驱动 Agent，MCP 解决 Agent 如何连接外部工具服务器。两者互补，不重叠。

## 目标

- 作为 MCP Client 连接多个外部 MCP 服务器，支持 stdio / Streamable HTTP 两种传输
- 将 MCP 服务器的 Tools 动态注册为 BaseTool，命名格式 `mcp__{server}__{tool}`
- 将 MCP 服务器的 Resources 通过 `mcp_read_resource` 工具暴露
- 支持 `.mcp.json`（项目级）+ `settings.json`（全局）双层配置合并
- 集中生命周期管理（连接、发现、调用、断开），单个服务器失败不影响其他服务器

**v1 不包含：** Prompts 集成、Sampling 能力——留给后续迭代。

## 方案设计

### 架构总览

```
┌─────────────────────────────────────────────────┐
│                  ReActAgent                      │
│  ┌───────────────────────────────────────────┐   │
│  │          Middleware Chain                  │   │
│  │  ... FilesystemMiddleware ...              │   │
│  │  ... TerminalMiddleware ...                │   │
│  │  ┌─────────────────────────────────────┐  │   │
│  │  │      McpMiddleware                  │  │   │
│  │  │  ┌─────────────┐  ┌─────────────┐  │  │   │
│  │  │  │ McpConfig   │  │ McpClientPool│  │  │   │
│  │  │  │ .mcp.json   │  │ server→client│  │  │   │
│  │  │  │ settings.json│ │              │  │  │   │
│  │  │  └─────────────┘  └─────────────┘  │  │   │
│  │  └─────────────────────────────────────┘  │   │
│  └───────────────────────────────────────────┘   │
│                                                   │
│  Tool Registry:                                   │
│  Read, Write, Edit, Bash, ...                    │
│  mcp__filesystem__read_file                      │
│  mcp__filesystem__write_file                     │
│  mcp__github__create_issue                       │
│  mcp__database__query                            │
│  mcp_read_resource                               │
└─────────────────────────────────────────────────┘
```

MCP middleware 遵循项目现有的 Middleware Chain 模式，在 `collect_tools()` 时将 MCP 服务器的工具注入工具注册表。MCP 连接池在 agent 启动时一次性初始化，在整个 agent 生命周期内复用。

### 核心组件

#### 1. McpConfig（配置加载与合并）

**职责**：解析 `.mcp.json`（项目级）和 `settings.json` 中的 `mcpServers` 字段，合并去重。

**配置格式（`.mcp.json`）**：

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "env": { "DEBUG": "1" }
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_TOKEN": "xxx" }
    },
    "remote-api": {
      "url": "https://mcp.example.com/sse",
      "headers": { "Authorization": "Bearer xxx" }
    }
  }
}
```

**`settings.json` 同构格式**：合并到 `mcpServers` 字段。

**合并规则**：

- 先加载全局 `settings.json`，再加载项目级 `.mcp.json`
- 同名 server 以项目级覆盖全局
- 环境变量中的 `${VAR}` 占位符在加载时展开（如 `${GITHUB_TOKEN}` → 实际值）

**传输配置判断**：

- 有 `command` 字段 → stdio 传输（通过 `TokioChildProcess::new(command)` 创建）
- 有 `url` 字段 → Streamable HTTP 传输（通过 `StreamableHttpClientTransport::from_uri(url)` 创建）

#### 2. McpClientPool（连接池管理）

**职责**：维护所有 MCP 服务器连接的活跃状态。在 agent 启动时一次性初始化，整个生命周期复用。

```rust
struct McpClientPool {
    clients: HashMap<String, Arc<McpClientHandle>>,
}

struct McpClientHandle {
    name: String,
    peer: Peer<RoleClient>,      // rmcp 的 Peer 实例，用于调用远程工具/资源
    tools: Vec<Tool>,            // 缓存的工具列表
    resources: Vec<Resource>,    // 缓存的资源列表
    status: ClientStatus,        // Connected / Failed / Disconnected
}
```

**连接管理策略**：

- **一次性初始化**：agent 启动时建立所有连接（在 `new()` 或显式 `initialize()` 中触发），而非每次 `before_agent` 重复连接
- 连接超时：stdio 10s、HTTP 30s
- 工具调用超时：单次 120s（与 Bash 工具对齐）
- 连接失败：跳过该 server，`tracing::warn!` 记录，不影响其他 server
- 子进程异常退出：标记 `Failed`，后续调用返回错误

**生命周期集成**：

- McpClientPool 在 App 层创建并持久化，通过 `Arc` 共享给每次 `run_universal_agent()` 调用
- 每次 `run_universal_agent()` 创建的 McpMiddleware 引用同一个 pool
- App 退出时统一调用 `pool.shutdown()` 关闭所有连接和子进程

#### 3. McpToolBridge（工具桥接）

**职责**：将单个 MCP tool 包装为 `BaseTool` 实现。

```rust
struct McpToolBridge {
    server_name: String,
    tool_name: String,
    full_name: String,            // "mcp__{server}__{tool}"
    description: String,          // "[MCP:{server}] {原始 description}"
    input_schema: serde_json::Value,
    client: Arc<McpClientHandle>,
}
```

- `name()` → `mcp__{server}__{tool}`
- `description()` → `[MCP:{server}] {原始 description}`（前缀让 LLM 识别工具来源）
- `parameters()` → 直接返回 MCP `Tool.inputSchema`（JSON Schema 无需转换）
- `invoke(input)` → 调用 `peer.call_tool(CallToolRequestParam { name, arguments })`，将 `CallToolResult.content` 格式化为字符串返回

#### 4. McpResourceTool（资源读取工具）

**职责**：统一资源读取入口。

```rust
struct McpResourceTool {
    client_pool: Arc<McpClientPool>,
}
```

- `name()` → `mcp_read_resource`
- `parameters()` → `{ server_name: string, uri: string }`
- `description()` → 动态注入已连接 server 的可用 resource URI 列表，格式如：

  ```
  Read a resource from an MCP server. Available resources:
  - server "filesystem": file:///tmp/... (3 resources)
  - server "database": pg://localhost/mydb/schema (5 resources)
  ```

- `invoke(input)` → 从 `client_pool` 获取指定 server 的 peer，调用 `peer.read_resource(ReadResourceRequestParam { uri })`

### HITL 审批集成

**策略**：非 YOLO 模式下，所有 MCP 工具调用均需 HITL 审批。

**实现方式**：在 `HumanInTheLoopMiddleware` 的拦截规则中增加 `mcp__` 前缀通配匹配。当工具名以 `mcp__` 开头时，走标准审批流程（Approve / Edit / Reject / Respond），与内置敏感工具行为一致。

**权限模式映射**：

| 权限模式 | MCP 工具行为 |
|---------|-------------|
| `Default` | 需审批 |
| `AcceptEdits` | 需审批 |
| `Auto` | LLM 分类器判断 |
| `BypassPermissions` | 直接放行 |
| `DontAsk` | 跳过审批 |

### SubAgent 继承

**策略**：MCP 工具默认被 SubAgent 继承。

**实现方式**：MCP 工具通过 `McpToolBridge` 实现了 `BaseTool`，包含在 `collect_tools()` 返回的工具列表中。`run_universal_agent()` 构建 `SubAgentMiddleware` 的 `parent_tools` 时，MCP 工具自然被包含。

**连接生命周期**：子 agent 的 `after_agent()` 不关闭 MCP 连接。连接由父级 `McpClientPool` 统一管理，通过 `Arc<McpClientHandle>` 引用计数共享。子 agent 完成后 `Arc` 引用释放，但连接保持活跃供后续使用。

### 中间件生命周期

```
App 启动 / 首次 execute():
  1. McpClientPool::new(config)
     → 读取 ~/.peri/settings.json 的 mcpServers
     → 读取 {cwd}/.mcp.json 的 mcpServers
     → 合并（项目级覆盖全局）
     → 环境变量 ${VAR} 展开
  2. 遍历配置 → 为每个 server 创建 Transport 并连接
     → rmcp::serve_client(handler, transport).await 获取 Peer<RoleClient>
     → peer.initialize().await 握手
     → peer.list_tools(Default::default()).await 发现工具 → 缓存
     → peer.list_resources(Default::default()).await 发现资源 → 缓存
     → 连接失败的 server: warn 日志，跳过

每次 run_universal_agent():
  → McpMiddleware::new(Arc::clone(&pool))  // 引用共享 pool
  → collect_tools(cwd):
      1. 遍历 pool 中所有已连接的 peer
      2. 为每个 peer 的每个 tool 创建 McpToolBridge 实例
      3. 如果任何 peer 有 resources → 创建一个 McpResourceTool（description 动态注入可用资源列表）
      4. 返回所有 McpToolBridge + 可选的 McpResourceTool
  → before_agent() / after_agent(): 空操作（连接已建立，无需重复初始化/关闭）

App 退出:
  → pool.shutdown()
     → 遍历所有 client → close() 连接
     → 清理子进程资源（stdio transport）
```

### 传输层适配

```rust
enum McpTransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    StreamableHttp {
        url: String,
        headers: HashMap<String, String>,
    },
}
```

| 传输 | 触发条件 | 连接方式 | 超时 |
|------|---------|---------|------|
| stdio | 配置含 `command` 字段 | 启动子进程，stdin/stdout JSON-RPC | 10s |
| Streamable HTTP | 配置含 `url` | HTTP POST，双向流 | 30s |

> **注意**：rmcp v0.14.0 已移除 SSE 客户端传输（`transport-sse-client` feature 不再存在）。仅支持 stdio 和 Streamable HTTP 两种客户端传输。如需连接仅支持 SSE 的旧版服务器，可通过 Streamable HTTP 兼容或等待后续版本支持。

### 工具调用流程

```
LLM 选择 mcp__filesystem__read_file 工具
  → HITL 拦截（非 YOLO 模式下需审批）
  → 审批通过后 ReAct 执行器调用 tool.invoke({"path": "/tmp/file.txt"})
  → McpToolBridge.invoke():
      1. 从 Arc<McpClientHandle> 获取 peer（Peer<RoleClient>）
      2. peer.call_tool(CallToolRequestParam { name: "read_file", arguments: {"path": "/tmp/file.txt"} })
      3. rmcp 通过 Transport 发送 JSON-RPC 请求
      4. MCP Server 执行工具，返回 CallToolResult
      5. 将 content 格式化为字符串
      6. 返回结果（is_error=true 时包含错误信息）
```

### 错误处理策略

| 场景 | 处理方式 |
|------|---------|
| 配置文件不存在/格式错误 | `tracing::warn!` 日志，跳过，不中断 agent |
| 服务器连接超时/失败 | `tracing::warn!` 日志，跳过该 server，其他正常 |
| 工具调用超时（120s） | 返回超时错误给 LLM，由 LLM 决定重试 |
| 服务器返回 `is_error: true` | 透传错误内容给 LLM |
| 服务器断开连接 | 标记 `Failed`，返回连接错误 |
| 环境变量 `${VAR}` 不存在 | 替换为空字符串，`tracing::warn!` 提示 |

### 模块结构

```
peri-middlewares/src/
├── mcp/
│   ├── mod.rs              # pub mod 声明 + McpMiddleware 重导出
│   ├── config.rs           # McpConfig、McpServerConfig、合并逻辑、环境变量展开
│   ├── client.rs           # McpClientHandle（封装 rmcp Peer）、McpClientPool、ClientStatus
│   ├── transport.rs        # McpTransportConfig → rmcp Transport 构建工厂（TokioChildProcess / StreamableHttpClientTransport）
│   ├── tool_bridge.rs      # McpToolBridge（BaseTool impl）
│   ├── resource_tool.rs    # McpResourceTool（BaseTool impl）
│   └── middleware.rs       # McpMiddleware（Middleware<S> trait impl）
```

### 中间件注册位置

在 TUI 的 `run_universal_agent()` 中，McpMiddleware 应注册在 SubAgentMiddleware **之后**，确保内置工具有优先级：

```
1. AgentDefineMiddleware
2. AgentsMdMiddleware
3. SkillsMiddleware
4. SkillPreloadMiddleware
5. FilesystemMiddleware
6. TerminalMiddleware
7. TodoMiddleware
8. HumanInTheLoopMiddleware
9. SubAgentMiddleware
10. McpMiddleware           ← 新增
```

手动注册工具（`register_tool`）优先级最高，可覆盖同名 MCP 工具。

### TUI 集成点

```
App 结构体新增:
  mcp_pool: Option<Arc<McpClientPool>>    // agent 启动时初始化，退出时 shutdown

run_universal_agent() 修改:
  1. 如果 app.mcp_pool 为 None → McpClientPool::initialize(cwd) 创建并缓存到 app
  2. McpMiddleware::new(Arc::clone(&app.mcp_pool)) 引用共享 pool
  3. parent_tools 构建时自然包含 MCP 工具（从 collect_tools 返回）

AgentRunConfig 新增:
  mcp_pool: Option<Arc<McpClientPool>>    // 从 App 传入共享 pool
```

## 实现要点

### 依赖引入

`peri-middlewares/Cargo.toml` 新增：

```toml
[dependencies]
rmcp = { version = "0.14", features = [
    "client",                                     # MCP Client 角色
    "transport-child-process",                    # stdio 传输（子进程）
    "transport-streamable-http-client-reqwest",   # Streamable HTTP 传输
] }
```

> `rmcp` 即官方 MCP Rust SDK 的 crates.io 包名（仓库：<https://github.com/modelcontextprotocol/rust-sdk/），当前版本> 0.14.0，下载量 3.17M+，Apache-2.0 许可。
>
> **Edition 兼容性**：rmcp 使用 Rust edition 2024，编译要求 Rust toolchain >= 1.85。本项目其他 crate 使用 edition 2021 不受影响（edition 仅影响 crate 自身语法解析，跨 crate 调用无影响），但 CI 和开发环境需确保 Rust 版本 >= 1.85。

### 关键技术难点

1. **连接池生命周期**：MCP 连接在 agent 启动时一次性初始化，通过 `Arc<McpClientPool>` 在 App 层持久化。每次 `run_universal_agent()` 通过 `Arc::clone` 共享同一个 pool，避免重复连接开销。App 退出时统一 `shutdown()`。

2. **Transport 进程管理**：stdio transport 通过 rmcp 的 `TokioChildProcess` 启动子进程，子进程的生命周期由 `McpClientPool` 统一管理。进程异常退出需要正确检测和清理。`serve_client()` 返回的 `JoinHandle` 需要在 `shutdown()` 中 abort。

3. **并发工具调用**：ReAct 循环中多个工具可能并发执行，rmcp 的 `Peer<RoleClient>` 内部已处理并发安全（通过 channel 通信），通过 `Arc<McpClientHandle>` 共享即可。

4. **配置合并**：`.mcp.json` 可能包含敏感信息（API Key），需注意日志中不泄露。

5. **工具名冲突**：`mcp__` 前缀有效避免与内置工具冲突，但多个 MCP server 可能提供同名工具。按 server name 命名空间隔离，不同 server 的同名工具独立存在。

6. **HITL 前缀匹配**：`HumanInTheLoopMiddleware` 的拦截规则需扩展支持 `mcp__` 前缀通配，使所有 MCP 工具在非 YOLO 模式下走审批流程。

### 测试策略

采用**内存 MCP 服务器**方案：实现一个 minimal JSON-RPC MCP 服务器，在单元测试中启动并连接，覆盖完整工具调用链路。

```
测试矩阵:
├── config.rs     → 配置加载/合并/环境变量展开/格式错误容错
├── client.rs     → 连接池创建/连接失败容错/并发访问
├── tool_bridge.rs → 工具桥接/参数透传/结果格式化/超时
├── resource_tool.rs → 资源读取/动态 description 生成
├── middleware.rs → collect_tools 注册/与中间件链集成
└── integration   → 内存 MCP 服务器 + McpToolBridge 完整调用链路
```

内存 MCP 服务器实现：

- 监听 localhost 随机端口，支持 `initialize` / `tools/list` / `tools/call` / `resources/list` / `resources/read` 方法
- 预定义工具和资源，返回可预测的结果
- 支持模拟超时和错误场景

### 与 ACP 的关系

ACP（Agent Client Protocol）和 MCP（Model Context Protocol）是两个独立协议：

- **ACP**：IDE/编辑器如何驱动 Agent（会话管理、权限审批、UI 更新）
- **MCP**：Agent 如何连接外部工具/资源服务器

两者互补，不冲突。ACP 实现位于 `peri-tui/src/acp/`，MCP middleware 位于 `peri-middlewares/src/mcp/`。

## 约束一致性

### 与 constraints.md 一致性

| 约束 | 一致性 | 说明 |
|------|--------|------|
| Workspace 多 crate 分层 | ✓ 一致 | MCP middleware 在 middlewares 层，依赖核心层 BaseTool/Middleware trait，不依赖 TUI 层 |
| 异步优先 | ✓ 一致 | MCP 连接/调用全部 async，通过 rmcp 的 async API |
| Middleware Chain 模式 | ✓ 一致 | 遵循 `Middleware<S>` trait，横切关注点不侵入核心 |
| 工具系统 BaseTool trait | ✓ 一致 | MCP tool 通过 McpToolBridge 适配为 BaseTool |
| 错误处理库 crate 用 thiserror | ✓ 一致 | MCP 错误类型用 thiserror 定义 |
| 日志用 tracing | ✓ 一致 | 所有日志通过 tracing 宏 |
| 文件组织每模块一目录 | ✓ 一致 | `src/mcp/` 目录 + mod.rs 入口 |

### 架构偏离

无偏离。新增 MCP middleware 完全遵循现有 Middleware Chain 模式和工具系统。连接池生命周期由 App 层管理（通过 `Arc` 共享），符合项目"事件驱动通信、禁止共享可变状态"的原则。

## 验收标准

- [ ] McpMiddleware 实现 `Middleware<S>` trait，正确注册到中间件链
- [ ] 支持从 `.mcp.json` 和 `settings.json` 加载并合并 MCP 服务器配置
- [ ] 支持 stdio 传输（启动子进程连接 MCP 服务器）
- [ ] 支持 Streamable HTTP 传输
- [ ] MCP 连接池在 agent 启动时一次性初始化，跨多次 execute 复用
- [ ] MCP 服务器的每个 tool 注册为独立的 `mcp__{server}__{tool}` BaseTool
- [ ] LLM 能发现并调用 MCP 工具，调用结果正确返回
- [ ] MCP Resources 通过 `mcp_read_resource` 工具暴露，description 动态注入可用资源列表
- [ ] 非 YOLO 模式下所有 MCP 工具调用需 HITL 审批（`mcp__` 前缀通配匹配）
- [ ] MCP 工具默认被 SubAgent 继承，子 agent 的 after_agent 不关闭连接
- [ ] 单个 MCP 服务器连接失败不影响其他服务器和内置工具
- [ ] 工具调用超时 120s 后正确返回错误
- [ ] App 退出时正确清理所有 MCP 连接和子进程
- [ ] 环境变量 `${VAR}` 占位符在配置加载时正确展开
- [ ] tracing 日志不泄露敏感信息（API Key 等）
- [ ] 单元测试覆盖：配置加载/合并、工具桥接、错误处理
- [ ] 内存 MCP 服务器用于集成测试，覆盖完整工具调用链路
