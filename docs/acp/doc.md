# Agent Client Protocol (ACP) 技术文档

> SDK 文档：<https://agentclientprotocol.com/libraries/rust>
> 协议规范：<https://agentclientprotocol.com>

## 1. 概述

Agent Client Protocol (ACP) 标准化代码编辑器/IDE 与 AI 编程 Agent 之间的通信。基于 JSON-RPC 2.0，复用 MCP 类型，类似 LSP 对语言服务器的标准化作用。

**架构**：编辑器（Client）启动 Agent 子进程，通过 stdin/stdout 的 JSON-RPC 2.0 通信。Agent 通过 `session/update` 通知流式推送更新，通过 `session/request_permission` 请求权限审批。

---

## 2. Rust SDK（`agent-client-protocol`）

### 2.1 Crate 清单

| Crate | 说明 |
|-------|------|
| `agent-client-protocol` | 核心：角色、连接、Handler、协议类型、MCP Server、会话管理 |
| `agent-client-protocol-schema` | Schema 类型定义（ContentBlock、SessionUpdate 等） |
| `agent-client-protocol-derive` | Derive 宏（`JsonRpcNotification`、`JsonRpcRequest`、`JsonRpcResponse`） |
| `agent-client-protocol-tokio` | Tokio 工具（启动 Agent 进程、stdio 传输） |
| `agent-client-protocol-rmcp` | `rmcp` MCP SDK 集成 |
| `agent-client-protocol-conductor` | 代理链编排（Proxy → Conductor） |

当前版本：`0.11.1`，Apache 2.0 许可。仓库：`https://github.com/agentclientprotocol/rust-sdk`

### 2.2 四种角色（Role）

```rust
pub trait Role: Debug + Clone + Send + Sync + 'static + Eq + Ord + Hash {
    type Counterpart: Role<Counterpart = Self>;
    // ...
}
```

| 角色 | 对端 | 用途 |
|------|------|------|
| `Client` | `Agent` | IDE/CLI 控制端 |
| `Agent` | `Client` | LLM Agent |
| `Proxy` | `Conductor` | 中间代理，拦截/修改消息 |
| `Conductor` | `Proxy` | 编排代理链，路由消息 |

### 2.3 Builder 模式

每个角色通过 `Role::builder()` 创建构建器：

```rust
// Agent 侧
Agent::builder()
    .name("my-agent")
    .on_receive_request(handler, on_receive_request!())
    .on_receive_dispatch(handler, on_receive_dispatch!())
    .connect_to(Stdio::new())
    .await

// Client 侧
Client::builder()
    .on_receive_notification(handler, on_receive_notification!())
    .on_receive_request(handler, on_receive_request!())
    .connect_with(agent_transport, |conn| async { /* 初始化流程 */ })
    .await
```

**必需宏**（解决 async trait 限制）：

| 宏 | 用途 |
|----|------|
| `on_receive_request!()` | Builder::on_receive_request 的返回类型 |
| `on_receive_notification!()` | Builder::on_receive_notification 的返回类型 |
| `on_receive_dispatch!()` | Builder::on_receive_dispatch 的返回类型 |

### 2.4 Agent 侧完整示例

```rust
use agent_client_protocol::prelude::*;

Agent::builder()
    .name("my-agent")
    .on_receive_request(
        |req: InitializeRequest, responder, _conn| {
            responder.respond(
                InitializeResponse::new(req.protocol_version())
                    .agent_capabilities(AgentCapabilities::new())
            )
        },
        on_receive_request!()
    )
    .on_receive_dispatch(
        |msg: Dispatch, cx| {
            // 处理 session/new, session/prompt 等请求
            Ok(Handled::No)
        },
        on_receive_dispatch!()
    )
    .connect_to(Stdio::new())
    .await
```

### 2.5 Client 侧完整示例

```rust
use agent_client_protocol::prelude::*;

let agent = AcpAgent::from_str("python my_agent.py")?;

Client::builder()
    .name("my-client")
    .on_receive_notification(
        |notif: SessionNotification, _cx| {
            // 处理 session/update 通知
            Ok(())
        },
        on_receive_notification!()
    )
    .on_receive_request(
        |req: RequestPermissionRequest, responder, _conn| {
            responder.respond(
                RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(
                        SelectedPermissionOutcome::new("allow-once".into())
                    )
                )
            )
        },
        on_receive_request!()
    )
    .connect_with(agent, |conn: ConnectionTo<Agent>| async move {
        // 初始化
        conn.send_request(InitializeRequest::new(ProtocolVersion::V1)).await?;
        // 创建会话
        let session = conn.build_session_cwd()?
            .with_mcp_server(mcp)?
            .block_task()
            .run_until(async |mut session| {
                session.send_prompt("Hello")?;
                let response = session.read_to_string().await?;
                Ok(())
            })
            .await?;
        Ok(conn)
    })
    .await
```

---

## 3. 会话管理（Session Management）

### 3.1 SessionBuilder

SDK 提供 `SessionBuilder` 抽象化会话创建流程，支持阻塞/非阻塞两种模式：

```rust
// 阻塞模式（await 直到 prompt 轮次结束）
cx.build_session_cwd()?
    .with_mcp_server(mcp)?
    .block_task()           // 切换到 Blocking 模式
    .run_until(async |mut session| {
        session.send_prompt("Hello")?;
        let response = session.read_to_string().await?;
        Ok(())
    })
    .await

// 非阻塞模式（后台任务）
cx.build_session_cwd()?
    .with_mcp_server(mcp)?
    .on_session_start(async |session| {
        // 在后台任务中处理
    })
```

**关键方法**：

| 方法 | 说明 |
|------|------|
| `build_session(cwd)` | 创建 SessionBuilder |
| `build_session_cwd()` | 从当前工作目录创建 |
| `build_session_from(request)` | 从自定义 NewSessionRequest 创建 |
| `attach_session(response, ...)` | 附加到已有会话 |

### 3.2 会话生命周期

```
initialize → session/new → [session/prompt × N] → session/close
                          ↘ session/load (回放历史)
                          ↘ session/resume (恢复，不回放)
```

| 方法 | Rust 类型 | 说明 |
|------|-----------|------|
| `session/new` | `NewSessionRequest` → `NewSessionResponse` | 创建新会话 |
| `session/load` | `LoadSessionRequest` → `LoadSessionResponse` | 加载并回放历史 |
| `session/resume` | `ResumeSessionRequest` → `ResumeSessionResponse` | 恢复（不回放） |
| `session/close` | `CloseSessionRequest` → `CloseSessionResponse` | 关闭会话 |
| `session/list` | `ListSessionsRequest` → `ListSessionsResponse` | 列出会话 |
| `session/prompt` | `PromptRequest` → `PromptResponse` | 发送提示 |
| `session/cancel` | `CancelNotification` | 取消当前轮次 |
| `session/set_mode` | `SetSessionModeRequest` | 切换模式 |
| `session/set_config_option` | `SetSessionConfigOptionRequest` | 设置配置 |

---

## 4. 协议方法总览

### 4.1 客户端 → Agent（Client 调用，Agent 处理）

| 方法 | Rust 类型 | 稳定 |
|------|-----------|------|
| `initialize` | `InitializeRequest` → `InitializeResponse` | ✓ |
| `authenticate` | `AuthenticateRequest` → `AuthenticateResponse` | ✓ |
| `session/new` | `NewSessionRequest` → `NewSessionResponse` | ✓ |
| `session/load` | `LoadSessionRequest` → `LoadSessionResponse` | ✓ |
| `session/resume` | `ResumeSessionRequest` → `ResumeSessionResponse` | ✓ |
| `session/prompt` | `PromptRequest` → `PromptResponse` | ✓ |
| `session/cancel` | `CancelNotification` | ✓ |
| `session/close` | `CloseSessionRequest` → `CloseSessionResponse` | ✓ |
| `session/list` | `ListSessionsRequest` → `ListSessionsResponse` | ✓ |
| `session/set_mode` | `SetSessionModeRequest` → `SetSessionModeResponse` | ✓ |
| `session/set_config_option` | `SetSessionConfigOptionRequest` | ✓ |
| `session/set_model` | `SetSessionModelRequest` | unstable |
| `session/fork` | `ForkSessionRequest` → `ForkSessionResponse` | unstable |
| `logout` | `LogoutRequest` → `LogoutResponse` | unstable |

### 4.2 Agent → 客户端（Agent 调用，Client 处理）

| 方法 | Rust 类型 |
|------|-----------|
| `session/update` | `SessionNotification`（通知） |
| `session/request_permission` | `RequestPermissionRequest` → `RequestPermissionResponse` |
| `fs/read_text_file` | `ReadTextFileRequest` → `ReadTextFileResponse` |
| `fs/write_text_file` | `WriteTextFileRequest` → `WriteTextFileResponse` |
| `terminal/create` | `CreateTerminalRequest` → `CreateTerminalResponse` |
| `terminal/output` | `TerminalOutputRequest` → `TerminalOutputResponse` |
| `terminal/wait_for_exit` | `WaitForTerminalExitRequest` → ... |
| `terminal/kill` | `KillTerminalRequest` → `KillTerminalResponse` |
| `terminal/release` | `ReleaseTerminalRequest` → `ReleaseTerminalResponse` |

---

## 5. 传输层（Transports）

### 5.1 Stdio（内置）

```rust
Agent::builder()
    .connect_to(Stdio::new())  // 读写 stdin/stdout
    .await
```

### 5.2 AcpAgent（外部进程）

```rust
// 命令字符串
let agent = AcpAgent::from_str("python my_agent.py --verbose")?;

// JSON 配置
let agent = AcpAgent::from_str(r#"{"type":"stdio","name":"my-agent","command":"python","args":["my_agent.py"]}"#)?;

// 内置预设
AcpAgent::zed_claude_code();
AcpAgent::zed_codex();
AcpAgent::google_gemini();

// 调试回调
agent.with_debug(|line, direction| { eprintln!("{:?}: {}", direction, line); });
```

### 5.3 ConnectTo trait（组件抽象）

```rust
pub trait ConnectTo<R: Role>: Send + 'static {
    fn connect_to(self, client: impl ConnectTo<R::Counterpart>)
        -> impl Future<Output = Result<()>> + Send;
}
```

实现者：`Stdio`、`AcpAgent`、`Channel`、`ByteStreams`、自定义组件。异构集合用 `DynConnectTo<R>`。

---

## 6. MCP Server 集成

### 6.1 McpTool trait

```rust
pub trait McpTool<R: Role>: Send + Sync {
    type Input: JsonSchema + DeserializeOwned + Send + 'static;
    type Output: JsonSchema + Serialize + Send + 'static;
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn title(&self) -> Option<String> { None }
    fn call_tool(&self, input: Self::Input, context: McpConnectionTo<R>)
        -> impl Future<Output = Result<Self::Output, Error>> + Send;
}
```

### 6.2 McpServer Builder

```rust
let server = McpServer::builder("my-server".to_string())
    .instructions("A helpful assistant")
    .tool(MyCustomTool)
    // 闭包式工具
    .tool_fn(
        "greet", "Greet someone by name",
        async |input: GreetInput, _cx| Ok(format!("Hello, {}!", input.name)),
        tool_fn!(),   // 必需宏
    )
    // 工具过滤
    .disable_tool("dangerous_tool")   // 或 .disable_all_tools()
    .enable_tool("safe_tool")
    .build();
```

`EnabledTools`：`DenyList(HashSet<String>)`（默认全启用）或 `AllowList(HashSet<String>)`（仅白名单）。

---

## 7. 关键协议类型

### 7.1 初始化能力协商

**InitializeRequest**：

```rust
InitializeRequest::new(ProtocolVersion::V1)
// 字段：protocolVersion, clientCapabilities, clientInfo
```

```json
{
  "protocolVersion": 1,
  "clientCapabilities": {
    "fs": { "readTextFile": true, "writeTextFile": true },
    "terminal": true
  },
  "clientInfo": { "name": "my-client", "title": "My Client", "version": "1.0.0" }
}
```

**InitializeResponse**：

```rust
InitializeResponse::new(protocol_version)
    .agent_capabilities(AgentCapabilities::new())
// 字段：protocolVersion, agentCapabilities, agentInfo, authMethods
```

```json
{
  "protocolVersion": 1,
  "agentCapabilities": {
    "loadSession": true,
    "promptCapabilities": { "image": true, "audio": true, "embeddedContext": true },
    "mcpCapabilities": { "http": true, "sse": true },
    "sessionCapabilities": { "close": {}, "list": {}, "resume": {} }
  },
  "agentInfo": { "name": "my-agent", "title": "My Agent", "version": "1.0.0" },
  "authMethods": []
}
```

**能力速查**：

| 能力 | 声明方 | 含义 |
|------|--------|------|
| `fs.readTextFile` / `fs.writeTextFile` | Client | 支持文件读写 |
| `terminal` | Client | 支持终端 |
| `loadSession` | Agent | 支持历史回放 |
| `promptCapabilities.*` | Agent | 支持的 prompt 类型（image/audio/embeddedContext） |
| `mcpCapabilities.*` | Agent | 支持的 MCP 传输（http/sse） |
| `sessionCapabilities.*` | Agent | 会话能力（close/list/resume） |

### 7.2 ContentBlock（5 种）

| type | Rust | 需声明能力 | 说明 |
|------|------|-----------|------|
| `text` | `TextContent` | — | Markdown 文本 |
| `image` | — | `promptCapabilities.image` | Base64 图片 |
| `audio` | — | `promptCapabilities.audio` | Base64 音频 |
| `resource` | — | `promptCapabilities.embeddedContext` | 嵌入资源 |
| `resource_link` | — | — | 资源链接（不嵌入） |

### 7.3 SessionUpdate（10 种）

`session/update` 通知的 `update` 字段，通过 `sessionUpdate` 鉴别器区分：

| sessionUpdate | 关键字段 | 说明 |
|------|------|------|
| `agent_message_chunk` | `content: ContentBlock` | Agent 文本流 |
| `agent_thought_chunk` | `content: ContentBlock` | Agent 推理流 |
| `user_message_chunk` | `content: ContentBlock` | 用户消息回放 |
| `tool_call` | `toolCallId, title, kind, status, content` | 创建工具调用 |
| `tool_call_update` | `toolCallId, status, content` | 更新工具调用 |
| `plan` | `entries: PlanEntry[]` | 任务计划 |
| `available_commands_update` | `availableCommands` | 斜杠命令 |
| `current_mode_update` | `modeId` | 模式变更 |
| `config_option_update` | `configOptions` | 配置变更 |
| `session_info_update` | `title, _meta` | 会话元数据 |

### 7.4 ToolCall

| 字段 | 类型 | 说明 |
|------|------|------|
| `toolCallId` | string | 唯一标识 |
| `title` | string | 显示标题 |
| `kind` | `ToolKind` | read/edit/delete/move/search/execute/think/fetch/switch_mode/other |
| `status` | `ToolCallStatus` | pending → in_progress → completed / failed |
| `content` | `ToolCallContent[]` | content / diff / terminal |
| `locations` | `Location[]` | 代码位置（path + line） |

### 7.5 StopReason

| 值 | 说明 |
|------|------|
| `end_turn` | 正常结束 |
| `max_tokens` | 达到 token 上限 |
| `max_turn_requests` | 达到最大轮次 |
| `refusal` | Agent 拒绝 |
| `cancelled` | 被取消 |

### 7.6 权限

```rust
// Agent 发起
RequestPermissionRequest { sessionId, toolCall, options: Vec<PermissionOption> }

// Client 响应
RequestPermissionOutcome::Selected(SelectedPermissionOutcome { optionId })
RequestPermissionOutcome::Cancelled
```

**PermissionOptionKind**：`allow_once` / `allow_always` / `reject_once` / `reject_always`

---

## 8. 能力扩展（MetaCapability）

```rust
pub trait MetaCapability {
    fn key(&self) -> &'static str;
    fn value(&self) -> serde_json::Value { Value::Bool(true) }
}

// 使用
capabilities.has_meta_capability(McpAcpTransport);
capabilities.add_meta_capability(MyCapability);
```

内置：`McpAcpTransport`（key: `"mcp_acp_transport"`）

---

## 9. Feature Flags

```toml
[dependencies]
agent-client-protocol = { version = "0.11", features = ["unstable"] }
# 或按需启用：
# features = ["unstable_auth_methods", "unstable_session_fork", ...]
```

| Feature | 说明 |
|---------|------|
| `unstable` | 启用所有 unstable 特性 |
| `unstable_auth_methods` | 认证方法 |
| `unstable_session_fork` | 会话分叉 |
| `unstable_session_model` | 设置会话模型 |
| `unstable_logout` | 登出 |
| `unstable_message_id` | 消息 ID |
| `unstable_boolean_config` | 布尔配置选项 |
| `unstable_session_additional_directories` | 附加目录 |
| `unstable_session_usage` | 会话用量 |

---

## 10. 与 Peri 的映射分析

### 概念对照

| ACP 概念 | Peri 对应 |
|----------|----------------|
| `Client` | `peri-tui`（TUI 应用） |
| `Agent` | `peri-agent`（ReAct Agent） |
| Session | Thread（SQLite 持久化） |
| `session/prompt` | `ReActAgent::execute()` |
| `SessionNotification` | `AgentEvent` 枚举 |
| `tool_call` | `ContentBlock::ToolUse` |
| `RequestPermissionRequest` | `HumanInTheLoopMiddleware` |
| `ContentBlock` | `ContentBlock`（结构已类似） |
| MCP Servers | 外部 MCP 服务器配置 |

### 事件映射

| ACP SessionUpdate | Peri AgentEvent |
|------|------|
| `agent_message_chunk` | `TextChunk` |
| `tool_call` / `tool_call_update` | `ToolStart` / `ToolEnd` |
| `plan` | `TodoWrite` |
| `agent_thought_chunk` | `AiReasoning` |

### 接入策略

使用 `agent-client-protocol` crate 后，SDK 自动处理：

1. **传输层**：`Stdio` / `AcpAgent` 自动处理 JSON-RPC 编解码
2. **会话管理**：`SessionBuilder` 封装 new/load/resume/close
3. **消息路由**：Builder 模式 + Handler 自动分发
4. **MCP 工具**：`McpTool` trait + `McpServer::builder` 简化工具注册

需要手动适配的部分：

1. **AgentEvent → SessionNotification** 转换层
2. **HITL → RequestPermissionResponse** 决策映射
3. **ContentBlock 补充** `resource` / `resource_link` 变体
4. **ReAct 循环集成**：将 ACP 的 `session/prompt` 阻塞模式与 ReAct 迭代循环对接

---

## 参考链接

- SDK 文档：<https://agentclientprotocol.com/libraries/rust>
- 协议规范：<https://agentclientprotocol.com>
- 完整规范文本：<https://agentclientprotocol.com/llms-full.txt>
- Rust SDK 源码：<https://github.com/agentclientprotocol/rust-sdk>
- crates.io：<https://crates.io/crates/agent-client-protocol>
- JSON Schema：<https://github.com/agentclientprotocol/agent-client-protocol/blob/main/schema/schema.json>
- ACP Registry：<https://github.com/agentclientprotocol/registry>
