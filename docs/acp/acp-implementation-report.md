# ACP（Agent Client Protocol）实现报告

> 分析目标：`/Users/konghayao/code/knowledgebase`（协议文档 + Rust schema crate）
> + `/Users/konghayao/code/ai/perihelion/peri-tui/src/acp/`（perihelion 的 ACP 服务端实现）
> 分析日期：2026-05-16

---

## 1. 项目结构总览

仓库包含三部分资源：

| 层级 | 路径 | 说明 |
|------|------|------|
| 协议源码 | `origin/acp/agent-client-protocol/` | Rust crate `agent-client-protocol-schema` v0.13.0，由 Zed 团队维护 |
| 知识文档 | `knowledge/acp/` | 中文技术笔记，涵盖协议概述、初始化、会话管理、工具调用等 |
| 参考文档 | `origin/acp/acp-in-cursor.md` | Cursor IDE 中 ACP 的使用说明 |

---

## 2. 协议源码分析（Rust crate）

### 2.1 架构一览

```
agent-client-protocol-schema (v0.13.0)
├── src/
│   ├── lib.rs         ← crate 根，导出 v1（稳定）和可选 v2（不稳定）模块
│   ├── rpc.rs         ← JSON-RPC 2.0 信封类型
│   ├── serde_util.rs  ← 序列化辅助工具（MaybeUndefined 等）
│   ├── version.rs     ← ProtocolVersion（V0/V1/V2）
│   ├── generate.rs    ← Schema + Markdown 文档生成 CLI
│   ├── v1/
│   │   ├── mod.rs         ← SessionId 定义 + 子模块再导出
│   │   ├── agent.rs       ← Agent 侧路由（ClientRequest/AgentResponse 等）
│   │   ├── client.rs      ← Client 侧路由（AgentRequest/ClientResponse 等）
│   │   ├── content.rs     ← 内容块结构（Text/Image/Audio/Resource）
│   │   ├── tool_call.rs   ← 工具调用生命周期（ToolCall/ToolCallUpdate）
│   │   ├── plan.rs        ← 执行计划（Plan/PlanEntry）
│   │   ├── error.rs       ← 错误定义（ErrorCode/Error）
│   │   ├── ext.rs         ← 扩展机制（_meta + _method）
│   │   ├── protocol_level.rs  ← 协议级通知（$/cancel_request）
│   │   ├── mcp.rs         ← MCP-over-ACP（unstable）
│   │   ├── elicitation.rs ← 结构化用户输入（unstable）
│   │   └── nes.rs         ← Next Edit Suggestions（unstable）
│   └── v2/
│       ├── mod.rs         ← v2 类型（结构同 v1，语义不同）
│       └── conversion.rs  ← v1 ↔ v2 类型互转
├── schema/
│   ├── schema.json          ← v1 稳定版 JSON Schema
│   ├── schema.unstable.json ← v1 + 不稳定特性 JSON Schema
│   └── schema.v2.unstable.json ← v2 JSON Schema
├── Cargo.toml
└── bin/generate.rs          ← schema 生成 CLI
```

### 2.2 协议基础：JSON-RPC 2.0 信封（rpc.rs）

```rust
// 请求
Request<Params>      → { jsonrpc:"2.0", id, method, params }
// 响应
Response<Result, Error> → { jsonrpc:"2.0", id, result|error }
// 通知
Notification<Params> → { jsonrpc:"2.0", method, params }
// 消息路由
JsonRpcMessage<M>    → Request<…> | Response<…> | Notification<…>
// 请求 ID
RequestId            → String | i64
```

### 2.3 核心路由枚举

协议定义了 **6 个路由枚举**，覆盖双向通信：

| 枚举 | 方向 | 说明 |
|------|------|------|
| `ClientRequest` | Client → Agent | prompt/session 管理/初始化/认证 |
| `AgentResponse` | Agent → Client | 对应 ClientRequest 的响应 |
| `ClientNotification` | Client → Agent | cancel/文档事件/扩展 |
| `AgentRequest` | Agent → Client | 权限申请/文件读写/终端管理 |
| `ClientResponse` | Client → Agent | 对应 AgentRequest 的响应 |
| `AgentNotification` | Agent → Client | session 更新/扩展/elicitation/MCP |

**Agent 侧方法**（`ClientRequest` 变体）：

| 方法 | 说明 | 稳定性 |
|------|------|--------|
| `initialize` | 初始化协商，交换能力和版本 | 稳定 |
| `authenticate` | 用户认证 | 稳定 |
| `session/new` | 创建新会话 | 稳定 |
| `session/load` | 加载已有会话 | 稳定 |
| `session/resume` | 恢复中断的会话 | 稳定 |
| `session/close` | 关闭会话 | 稳定 |
| `session/list` | 列出所有会话 | 稳定 |
| `session/fork` | 分叉会话 | unstable |
| `session/prompt` | 发送提示词 | 稳定 |
| `session/cancel` | 取消当前处理 | 稳定（通知） |
| `session/set_mode` | 设置会话模式 | 稳定 |
| `session/set_model` | 设置模型 | unstable |
| `session/set_config_option` | 设置配置项 | 稳定 |
| `providers/list` | 列出 LLM 提供商 | unstable |
| `providers/set` | 配置提供商 | unstable |
| `providers/disable` | 禁用提供商 | unstable |
| `logout` | 登出 | unstable |

**Client 侧方法**（`AgentRequest` 变体）：

| 方法 | 说明 | 稳定性 |
|------|------|--------|
| `session/update` | Agent 推送会话更新 | 稳定（通知） |
| `session/request_permission` | 请求用户授权 | 稳定 |
| `client/available_commands` | 推送可用命令列表 | 稳定 |
| `fs/write_text_file` | 请求写入文件 | 稳定 |
| `fs/read_text_file` | 请求读取文件 | 稳定 |
| `terminal/create` | 创建终端 | 稳定 |
| `terminal/output` | 终端输出推送 | 稳定（通知） |
| `terminal/release` | 释放终端 | 稳定 |
| `terminal/kill` | 终止终端进程 | 稳定 |
| `terminal/wait_for_exit` | 等待终端退出 | 稳定 |

### 2.4 会话更新变体（11 种）

`SessionUpdate` 枚举是 Agent 向 Client 推送的核心通道：

1. **AgentMessageChunk** — 流式文本块，通常包含 ContentChunk
2. **ToolCall** — 新工具调用开始
3. **ToolCallUpdate** — 工具调用状态变更（InProgress/Failed/Completed）
4. **Plan** — 执行计划
5. **AvailableCommandsUpdate** — 可用命令列表变更
6. **CurrentModeUpdate** — 当前模式变更
7. **ConfigOptionUpdate** — 配置项变更
8. **SessionInfoUpdate** — 会话元信息变更
9. **UsageUpdate** — Token/费用统计（unstable）
10. **AgentReasoningChunk** — Agent 推理过程流式输出
11. **Extra** — 扩展会话更新（`_meta` 带外数据）

### 2.5 工具调用系统（tool_call.rs）

完整的工具调用生命周期：

```
ToolCall {
    id: ToolCallId,          // 唯一标识
    name: String,            // 工具名称
    kind: ToolKind,          // Read|Edit|Delete|Move|Search|Execute|Think|Fetch|SwitchMode|Other
    description: String,     // 人类可读描述
    status: ToolCallStatus,  // Pending|InProgress|Completed|Failed
    content: Vec<ToolCallContent>, // Content|Diff|Terminal
    // ...
}

ToolCallUpdate {
    id: ToolCallId,
    status: Option<..>,      // 更新状态
    content: Option<..>,     // 追加内容
    raw_input+raw_output,    // 原始 IO
    location: Option<..>,    // 文件位置
    timing: Option<..>,      // 耗时
    error: Option<..>,       // 错误信息
}
```

**工具种类**（`ToolKind`）：Read、Edit、Delete、Move、Search、Execute、Think、Fetch、SwitchMode、Other

**工具状态**（`ToolCallStatus`）：Pending → InProgress → Completed/Failed

### 2.6 内容块结构（content.rs）

```
ContentBlock → Text | Image | Audio | ResourceLink | Resource

TextContent  { text: String, annotations: Annotations }
ImageContent { uri: String, mime_type, data: base64 }
AudioContent { uri: String, mime_type, data: base64 }
EmbeddedResource { uri, mime_type, data: base64 }
```

### 2.7 扩展机制（三层）

1. **`_meta` 字 段**：所有类型均包含 `#[serde(flatten)] _meta: Meta`，可附加任意元数据
2. **下划线前缀方法**：`_method_name` 形式的方法名保留给自定义扩展
3. **能力声明**：初始化时通过 `capabilities._meta` 广告自定义能力

对应的类型：
```rust
Meta = HashMap<String, Value>    // 类型别名
ExtRequest     → method 以 `_` 开头
ExtResponse    → 对应 ExtRequest
ExtNotification → method 以 `_` 开头
```

### 2.8 不稳定特性门控

| 特性 flag | 提供的方法/类型 |
|-----------|----------------|
| `unstable_mcp_over_acp` | mcp/connect、mcp/message、mcp/disconnect |
| `unstable_elicitation` | elicitation/create、elicitation/complete |
| `unstable_nes` | nes/start、nes/suggest、nes/close、文档事件 |
| `unstable_cancel_request` | `$/cancel_request` |
| `unstable_session_fork` | session/fork |
| `unstable_session_usage` | Usage、UsageUpdate、Cost |
| `unstable_session_model` | session/set_model、SessionModelState |
| `unstable_auth_methods` | EnvVar/Terminal 认证 |
| `unstable_logout` | logout |
| `unstable_llm_providers` | providers/list、set、disable |
| `unstable_message_id` | 消息 UUID |
| `unstable_boolean_config` | Boolean config options |
| `unstable_protocol_v2` | 整个 v2 模块 |

### 2.9 能力声明

初始化时双方交换能力（Capabilities）：

```rust
ClientCapabilities {
    fs,           // 文件系统访问
    terminal,     // 终端管理
    elicitation,  // 结构化输入（unstable）
    mcp,          // MCP 支持
    prompt_capabilities: PromptCapabilities { // 图像、音频、嵌入支持等 }
}
AgentCapabilities {
    prompt_capabilities: PromptCapabilities,
    mcp_capabilities: McpCapabilities,
    session_capabilities: SessionCapabilities,  // fork/load/resume 等
}
```

### 2.10 MCP-over-ACP 传输（mcp.rs）

允许通过 ACP 通道承载 MCP 协议：

```rust
// 连接 MCP 服务器
ConnectMcpRequest  { servers: Vec<McpServer> }  // Stdio|Http|Sse|Acp
ConnectMcpResponse { connection_ids: Vec<McpConnectionId> }

// 消息中继
MessageMcpRequest   { connection_id, message_id, data: Value }
MessageMcpResponse  { data: Value }
MessageMcpNotification { connection_id, data: Value }

// 断开连接
DisconnectMcpRequest { connection_ids: Vec<McpConnectionId> }
```

### 2.11 终端管理

Agent 通过 ACP 请求创建和管理终端：

```
CreateTerminal → (terminal_id) → TerminalOutput(streaming)
                                → WaitForTerminalExit
                                → KillTerminal | ReleaseTerminal
```

### 2.12 文件系统操作

Agent 请求 Client 执行文件操作：

- `fs/write_text_file` — 写文本文件，支持 `new`/`overwrite` 操作
- `fs/read_text_file` — 读文本文件，支持 `offset`/`limit` 分页

### 2.13 权限申请

Agent 可请求用户授权：

```rust
RequestPermissionRequest {
    options: Vec<PermissionOption>,  // allow_once|allow_always|reject_once|reject_always
    resource_categories: Option<Vec<..>>, // file|command|network|shell
}
RequestPermissionResponse {
    outcome: RequestPermissionOutcome  // selected option
}
```

### 2.14 序列化工具（serde_util.rs）

```rust
MaybeUndefined<T>   // 三态：Undefined（字段不存在）/Null（JSON null）/Value(T)
IntoOption<T>       // 反序列化为 Option<T>，缺失/Null → None
IntoMaybeUndefined<T> // 反序列化为 MaybeUndefined<T>
SkipListener        // 反序列化钩子，跳过无法识别的变体（容错）
```

### 2.15 v2 模块

v2 模块存在结构但不稳定（需 `unstable_protocol_v2` flag）。类型定义与 v1 并行，通过 `v2/conversion.rs` 实现互转。

---

## 3. 通信流程

### 3.1 标准主流程

```
Client (IDE)                  Agent (AI)
   |                            |
   |-- initialize ------------->|  ← 版本协商 + 能力声明
   |<- initialize response -----|
   |                            |
   |-- authenticate (可选) ---->|
   |<- authenticate response ---|
   |                            |
   |-- session/new ------------>|
   |<- session/new response ----|  ← 返回 sessionId
   |                            |
   |-- session/prompt --------->|  ← 用户提示词
   |<- session/update (x N) ----|  ← 流式文本 + 工具调用
   |<- prompt response ---------|  ← stop_reason
   |                            |
   |-- session/cancel --------->|  ← 用户取消（通知）
```

### 3.2 初始化协商

版本协商策略：Client 发送支持的 `ProtocolVersion` 列表，Agent 选择最高可支持的版本。能力声明用于特性发现——非破坏性变更通过新增能力引入而非递增版本号。

### 3.3 会话生命周期

```
new → active → close
     ↓
     fork → active (新会话)
     ↓
load/resume → active
```

### 3.4 权限请求流程

```
Agent                          Client
 |-- request_permission ------>|
 |<- request_permission resp --|  ← selected: allow_once/always/reject_*
```

---

## 4. 传输层

### 4.1 stdio（稳定，已实现）

- Client 将 Agent 作为子进程启动
- 消息通过 stdin/stdout 交换，`\n` 分隔
- Agent 可在 stderr 输出日志
- 消息 **不得** 包含内嵌换行 → 整个 JSON 消息单行传输

### 4.2 Streamable HTTP + WebSocket（草案）

- 单一 `/acp` 端点承载两种传输模式
- 双流模型：连接级 GET 流（`Acp-Connection-Id`）+ 会话级 GET 流（`Acp-Session-Id`）
- 必须使用 HTTP/2
- 身份模型三层：连接 ID / 会话流 ID / 业务 sessionId

### 4.3 自定义传输

允许任意传输，只要满足 JSON-RPC 消息格式和 ACP 生命周期要求。

---

## 5. 测试覆盖

Rust crate 包含内联测试（`#[cfg(test)]`），覆盖：

| 测试文件 | 覆盖内容 |
|----------|---------|
| `rpc.rs` | RequestId 序列化/反序列化/Display，JSON-RPC 消息格式快照 |
| `serde_util.rs` | MaybeUndefined 三态、SkipListener/VecSkipError 弹性反序列化 |
| `version.rs` | ProtocolVersion 多种输入格式反序列化 |
| `content.rs` | TextContent/ImageContent/AudioContent 序列化/可选字段省略 |
| `error.rs` | ErrorCode 序列化/反序列化/循环一致性 |
| `agent.rs` | MCP 服务器序列化、认证方法序列化、配置选项序列化 |
| `client.rs` | SessionInfoUpdate 三态、NES 位置编码、MCP-over-ACP 方法名 |
| `elicitation.rs` | 完整 Serialization/Deserialization 覆盖 |
| `generate.rs` | Markdown 文档生成器测试 |

---

## 6. perihelion 的 ACP 服务端实现

> **路径**：`peri-tui/src/acp/`（9 个文件，~5800 行 Rust）
> **角色**：ACP Agent 端——perihelion 作为 ACP 服务端，通过 stdio 接受 IDE（如 Cursor）连接

### 6.1 架构全景

```
IDE (Cursor / ACP Client)
   │
   │  stdin/stdout (JSON-RPC 2.0 / ACP v1)
   │
   ▼
┌─────────────────────────────────────────────────────────┐
│ main_acp.rs          ← 入口：Agent::builder()          │
│                      ← connect_to(Stdio::new())        │
│                      ← 注册 12 个 request handler +    │
│                        1 个 dispatch handler            │
├─────────────────────────────────────────────────────────┤
│ request_handler.rs   ← initialize 协商（能力声明）      │
├─────────────────────────────────────────────────────────┤
│ dispatch.rs          ← 核心：10 个 session handler     │
│    handle_new_session    → SessionManager::new_session │
│    handle_prompt         → agent_assembler + 事件映射   │
│    handle_load_session   → 历史消息回放                │
│    handle_resume_session → 恢复中断会话                │
│    handle_close_session  → cancel_token.cancel()       │
│    handle_list_sessions  → ThreadStore::list_threads   │
│    handle_set_mode       → PermissionMode 切换         │
│    handle_set_model      → model_alias 切换            │
│    handle_set_config_option → mode/model/thinking 设置 │
│    handle_fork_session   → 继承 model+thinking 分叉    │
│    handle_dispatch       → cancel 通知 / 未匹配传递    │
├─────────────────────────────────────────────────────────┤
│ session.rs           ← SessionManager（DashMap 多会话）│
│    AcpSession { session_id, thread_id, cwd,            │
│                 cancel_token, model_alias,             │
│                 permission_mode, thinking, ... }       │
├─────────────────────────────────────────────────────────┤
│ agent_assembler.rs   ← 构建 ReActAgent + 完整中间件链  │
│    AgentAssembleConfig → (ReActAgent, Todo Rx)         │
│    中间件链（15 层）:                                   │
│    AgentsMd → AgentDefine → Skills → SkillPreload →    │
│    Filesystem → GitAttribution → Terminal → Todo →     │
│    Cron → HITL → SubAgent → + AskUserTool              │
├─────────────────────────────────────────────────────────┤
│ event_mapper.rs      ← 双映射层                         │
│    map_executor_to_updates()  ← ExecutorEvent → SessionUpdate │
│    map_event_to_updates()     ← AgentEvent → SessionUpdate   │
│    map_message_to_updates()   ← BaseMessage → SessionUpdate  │
├─────────────────────────────────────────────────────────┤
│ broker.rs            ← HITL 权限桥接                   │
│    AcpInteractionBroker → UserInteractionBroker trait  │
│    HandlePendingPermission → RequestPermission RPC     │
│    permission_forwarding_loop → 异步转发循环           │
└─────────────────────────────────────────────────────────┘
```

### 6.2 入口（main_acp.rs）

```rust
pub async fn run_acp_mode(cwd, model_override, agent_type) -> Result<()> {
    // 1. 初始化 telemetry
    // 2. 加载 peri_config (JSON settings)
    // 3. 解析 LLM provider（支持 --model 覆盖）
    // 4. 加载 agent_overrides（--agent-type）
    // 5. 创建 SQLite ThreadStore（持久化会话消息）
    // 6. 创建 SessionManager（多会话管理）
    // 7. 注册 12 个 handler + 1 个 dispatch handler
    // 8. connect_to(Stdio::new()) ← 阻塞运行
}
```

**Handler 注册**（12 个 `on_receive_request` + 1 个 `on_receive_dispatch`）：

| Handler | ACP 方法 |
|---------|----------|
| `handle_initialize` | `initialize` |
| `handle_new_session` | `session/new` |
| `handle_close_session` | `session/close` |
| `handle_list_sessions` | `session/list` |
| `handle_prompt` | `session/prompt` |
| `handle_load_session` | `session/load` |
| `handle_resume_session` | `session/resume` |
| `handle_set_mode` | `session/set_mode` |
| `handle_set_config_option` | `session/set_config_option` |
| `handle_set_model` | `session/set_model` |
| `handle_fork_session` | `session/fork` |
| `handle_dispatch` | 通知 + 未匹配请求兜底 |

### 6.3 能力声明（request_handler.rs）

```rust
AgentCapabilities {
    load_session: true,
    prompt_capabilities.image: true,
    session_capabilities.close: Some,
    session_capabilities.list: Some,
    session_capabilities.resume: Some,
}
// Agent info: "peri" @ CARGO_PKG_VERSION
```

### 6.4 会话结构（session.rs）

```rust
AcpSession {
    session_id: String,                     // = thread_id
    thread_id: ThreadId,                    // SQLite 持久化标识
    cwd: String,                            // 工作目录
    cancel_token: CancellationToken,        // 取消信号
    state_messages: Vec<BaseMessage>,       // （预留，未使用）
    created_at: DateTime<Utc>,              // 创建时间
    model_alias: String,                    // "opus"/"sonnet"/"haiku"
    permission_mode: Arc<SharedPermissionMode>,
    thinking: Option<ThinkingConfig>,
}
```

**SessionManager**：基于 `DashMap<String, AcpSession>` + `Arc<dyn ThreadStore>` 的多会话管理器。会话存储在 `OnceCell` 全局单例中。

### 6.5 prompt 处理核心流程（dispatch.rs:273-454）

这是最复杂的 handler，展示了 ACP → ReActAgent 的完整桥接：

```
1. 从 prompt ContentBlock 提取文本
   ↓
2. 获取 session 元数据（thread_id, cwd, cancel_token,
   model_alias, permission_mode, thinking）
   ↓
3. 从 model_alias 构建 LlmProvider
   ↓
4. tokio::spawn 异步任务（避免阻塞 ACP 事件循环）：
   a. 加载线程历史 → mgr.load_thread_messages()
   b. 构建系统提示词 → build_system_prompt()
   c. 创建 AgentCancellationToken（关联 session cancel_token）
   d. 创建事件处理器：
      ExecutorEvent → event_mapper::map_executor_to_updates()
      → SessionNotification → conn.send_notification()
   e. 创建 ACP 权限桥接 broker + 权限转发循环 spawn
   f. agent_assembler::assemble_agent() → (ReActAgent, Todo Rx)
   g. 转发 Todo 更新 → SessionUpdate::Plan spawn
   h. executor.execute(input, &mut state, cancel) ← 阻塞
   i. 返回 PromptResponse(stop_reason)
```

**关键设计决策**：
- prompt 处理在 `tokio::spawn` 中执行，防止阻塞 ACP 消息循环
- 会话级 `cancel_token` 通过独立 spawn 任务桥接到 `AgentCancellationToken`
- 事件流通过 `FnEventHandler` 直接映射为 `SessionNotification`，不经过 TUI 层
- 历史消息使用 `AgentState::with_persistence()` 自动持久化

### 6.6 事件映射层（event_mapper.rs）

三层映射函数，将内部事件转换为 ACP 的 `SessionUpdate`：

| 函数 | 输入 | 输出 | 用途 |
|------|------|------|------|
| `map_executor_to_updates()` | `ExecutorEvent` | `Vec<SessionUpdate>` | prompt 执行时实时推送 |
| `map_event_to_updates()` | `AgentEvent` (TUI 层) | `Vec<SessionUpdate>` | （预留，备用） |
| `map_message_to_updates()` | `BaseMessage` | `Vec<SessionUpdate>` | session/load 历史回放 |

**事件映射关系**：

| ExecutorEvent | SessionUpdate 变体 |
|---------------|-------------------|
| `TextChunk` | `AgentMessageChunk` |
| `AiReasoning` | `AgentThoughtChunk` |
| `ToolStart` | `ToolCall(status=InProgress)` |
| `ToolEnd` | `ToolCallUpdate(status=Completed/Failed)` |

**ToolKind 推断**：`Read → Read`、`Write/Edit → Edit`、`Bash → Execute`、`Grep/Glob → Search`、其他 → `Other`

**未映射事件**：`StateSnapshot`、`InternalEvent`、`ContextWarning`、`LlmCallStart/End`、`LlmRetrying`、`MessageAdded`、`CompactEvent` 等 → 返回空 `vec![]`

### 6.7 权限桥接（broker.rs）

将 HITL 中间件的 `UserInteractionBroker` trait 桥接到 ACP 的 `RequestPermission` RPC：

```
HITL middleware
  └→ broker.request(context)   ← UserInteractionBroker trait
      └→ permission_tx.send()   ← mpsc channel
          └→ permission_forwarding_loop
              └→ handle_pending_permission
                  └→ conn.send_request(RequestPermissionRequest)
                      .block_task().await  ← 等待客户端响应
                      → map_permission_response
                      → response_tx.send(decisions)
```

**权限选项映射**：
- `allow_once` / `allow_always` → `ApprovalDecision::Approve`
- 其他 / Cancelled → `ApprovalDecision::Reject`

**注意**：`AskUser` 问题不支持 ACP 模式（返回空答案），因为 ACP 没有等价机制。

### 6.8 Agent 组装（agent_assembler.rs）

`assemble_agent()` 构建完整的 ReActAgent，包含 15 层中间件链：

```
1.  AgentsMdMiddleware         ← CLAUDE.md 注入
2.  AgentDefineMiddleware      ← agent 定义覆盖
3.  SkillsMiddleware           ← Skills 摘要
4.  SkillPreloadMiddleware     ← /skill-name 全文
5.  FilesystemMiddleware       ← 6 个文件工具
6.  GitAttributionMiddleware   ← Co-Authored-By
7.  TerminalMiddleware         ← Bash
8.  TodoMiddleware             ← Todo 写入
9.  CronMiddleware             ← Cron 调度
10. HumanInTheLoopMiddleware   ← 权限审批
11. SubAgentMiddleware         ← 子 Agent
+   AskUserTool (register_tool)
```

HITL 中间件使用 `LlmAutoClassifier`（Auto 模式），权限模式从 session 级 `permission_mode` 获取。SubAgent 中间件通过 `llm_factory` 闭包复用当前 session 的 LLM 配置。

### 6.9 配置项（3 种）

ACP Client 可通过 `session/set_config_option` 配置：

| Config ID | 说明 | 类型 | 可选值 |
|-----------|------|------|--------|
| `mode` | 权限模式 | Select | auto/default/acceptEdits/dontAsk/bypass |
| `model` | AI 模型 | Select | opus/sonnet/haiku（从 provider 配置读取） |
| `thinking_effort` | 推理深度 | Select | low/medium/high/xhigh/max |

### 6.10 Load/Resume 差异

| 操作 | 历史回放 | session 注册 |
|------|----------|-------------|
| `session/load` | ✅ 回放 `BaseMessage → SessionNotification` | ✅ |
| `session/resume` | ❌ 不回放 | ✅ |

### 6.11 Fork 机制

`session/fork`：从父 session 继承 `model_alias` + `thinking` 配置 → 创建全新 session → 返回独立 `sessionId`。

### 6.12 测试覆盖

`dispatch_test.rs` 包含 2 个序列化测试：
- `test_new_session_response_serialization` — 验证 modes + configOptions 序列化
- `test_session_model_state_serialization` — 验证 models 序列化

---

## 7. 总结

### 协议 schema（knowledgebase）

| 维度 | 评估 |
|------|------|
| 代码量 | ~3000+ 行 Rust（核心协议类型） |
| 协议版本 | v1 稳定，v2 草案 |
| 消息类型 | 6 个路由枚举，20+ 方法 |
| 稳定性机制 | 12 个 feature flag 门控不稳定特性 |
| 扩展性 | 三层扩展机制（_meta、_method、能力声明） |
| 传输层 | stdio（稳定），HTTP+WS（草案） |
| 测试 | 内联测试覆盖核心序列化/反序列化路径 |
| 文档 | 协议源码 + 中文知识笔记 |

### perihelion ACP 服务端（peri-tui/src/acp/）

| 维度 | 评估 |
|------|------|
| 代码量 | ~5800 行 Rust（9 个文件） |
| 接入方式 | Stdio 传输，阻塞运行 |
| 支持方法 | 11 个（initialize + 10 个 session 方法） |
| 事件推送 | ExecutorEvent → SessionUpdate → SessionNotification（实时流式） |
| 权限桥接 | HITL UserInteractionBroker ↔ ACP RequestPermission RPC |
| 中间件链 | 15 层（完整 peri-agent 中间件栈） |
| 持久化 | SQLite ThreadStore（跨会话历史） |
| 多会话 | DashMap + CancellationToken（每会话独立取消） |
| 配置项 | 3 种（mode / model / thinking_effort） |
| 测试 | 2 个序列化测试（覆盖率偏低） |

### 2026-05-16 更新

**Agent 构建已统一**：`agent.rs:build_bare_agent()` 为 TUI 和 ACP 的唯一构建入口。`agent_assembler.rs` 已简化为薄封装（180 行 → 70 行），直接调用 `build_bare_agent()`。

**事件映射已对齐**：
- `LlmCallEnd` → `UsageUpdate`（token 消耗可见）
- `ContextWarning` → `UsageUpdate`（上下文溢出预警）
- `LlmRetrying` → `SessionInfoUpdate`（重试状态可见）
- `ToolCall` 补全 `raw_input`/`raw_output` 字段
- `ToolKind` 细化：`WebFetch`/`WebSearch` → `Fetch`
- `StopReason` 精确映射：`MaxIterationsExceeded` → `MaxTurnRequests`
- `context_window` 从 model 获取（含 `context_1m` 覆盖），不再硬编码
- `set_mode`/`set_model`/`set_config_option` 后主动下发 `CurrentModeUpdate`/`ConfigOptionUpdate` 通知
- ACP 路径自动获得完整中间件栈：`WebMiddleware`、`McpMiddleware`、`ToolSearchMiddleware`、`context_budget`、`compact_config`
