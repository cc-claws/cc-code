# peri-acp

ACP (Agent Client Protocol) 服务层，桥接 TUI/IDE 前端与 Agent 执行引擎。

## 概述

`peri-acp` 实现了 [Agent Client Protocol](https://agentclientprotocol.com/)，提供：

- **Session 管理**：创建、恢复、持久化会话
- **Agent 构建**：组装 ReActAgent + 中间件链 + 工具集
- **Transport 抽象**：MpscTransport（TUI）和 StdioTransport（IDE）
- **事件映射**：ExecutorEvent → AcpNotification 转换
- **HITL/AskUser Broker**：交互式审批和问答
- **Langfuse 集成**：LLM 调用遥测追踪

## 架构

```
┌─────────────┐     ┌─────────────┐
│   TUI App   │     │  IDE (Zed)  │
└──────┬──────┘     └──────┬──────┘
       │ MpscTransport     │ StdioTransport
       └─────────┬─────────┘
                 │
        ┌────────▼────────┐
        │   ACP Server    │
        │  (peri-acp)     │
        └────────┬────────┘
                 │
    ┌────────────┼────────────┐
    │            │            │
┌───▼───┐  ┌────▼────┐  ┌───▼───┐
│Session│  │ Agent   │  │Broker │
│Manager│  │ Builder │  │ HITL  │
└───────┘  └────┬────┘  └───────┘
                │
       ┌────────▼────────┐
       │  peri-agent     │
       │  (ReAct Loop)   │
       └─────────────────┘
```

## 核心模块

| 模块 | 职责 |
|------|------|
| `session/` | 会话生命周期管理、executor 构建 |
| `agent/` | Agent 配置、AgentPool 缓存 |
| `transport/` | MpscTransport / StdioTransport 实现 |
| `broker/` | HITL 审批、AskUser 问答、Multiplex 路由 |
| `event/` | 事件映射和分发 |
| `dispatch/` | ACP 请求路由和命令分发 |
| `hooks/` | Hook 事件拦截 |
| `langfuse/` | Langfuse 追踪集成 |
| `lsp/` | LSP 客户端桥接 |
| `prompt/` | 系统提示词构建 |
| `provider/` | LLM Provider 管理 |

## 数据流

### TUI 路径

```
TUI 输入
  → AcpTuiClient.new_session() / .prompt()
  → MpscClientTransport.send_request()
  → MpscServerTransport.recv() (tokio::spawn)
  → ExecutorEvent → TransportEventSink.push_event()
  → AcpTuiClient.pump_notifications()
  → AcpNotification::AgentEvent
  → TUI UI 更新
```

### Stdio 路径（IDE）

```
SDK (JSON-RPC)
  → StdioTransport.recv()
  → executor::execute_prompt()
  → StdioEventSink.push_event()
  → stdout JSON-RPC
```

## 关键概念

### Frozen Data Flow

会话内不可变数据在 `session/new` 时一次性捕获：

- `frozen_system_prompt`：系统提示词
- `frozen_date`：会话创建日期
- `frozen_claude_md`：CLAUDE.md 内容
- `frozen_skill_summary`：Skills 摘要

每轮重新计算的数据：

- `is_git_repo`：实时检查
- 中间件链、AgentState：全新构造
- Provider Snapshot：从 `Arc<RwLock<>>` 克隆

### ACP Slash Commands

符合 agentclientprotocol.com 规范的命令系统：

| 命令 | 类型 | 说明 |
|------|------|------|
| `/compact` | Immediate | 上下文压缩 |
| `/clear` | Immediate | 清空对话 |
| `/rewind` | Immediate | 回滚对话到指定点 |
| `/commit` | Passthrough | Git 提交 |
| `/review` | Passthrough | PR 代码审查 |
| `/init` | Passthrough | 初始化项目配置 |

### AgentPool

会话级 LLM 实例缓存，避免每轮重建大对象（主要是 `reqwest::Client` 的连接池和 TLS 会话缓存）：

```rust
use peri_acp::session::agent_pool::AgentPool;

let pool = Arc::new(Mutex::new(AgentPool::new()));

// 检查缓存是否有效
if pool.lock().has_valid_cache(&provider) {
    // 复用缓存的 LLM 实例
    let cached = pool.lock().get_cached_llm().cloned();
} else {
    // 构建新实例并缓存
    let instances = CachedLlmInstances { ... };
    pool.lock().store_llm(instances);
}

// SubAgent LLM 缓存（双检锁优化）
let llm = AgentPool::get_or_create_subagent_llm(
    &pool,
    &fingerprint,
    || create_new_model(),
);
```

## 使用示例

### Transport 使用

```rust
use peri_acp::transport::mpsc::mpsc_transport_pair;
use peri_acp::transport::AcpTransport;

// 创建 transport（返回 client/server 通道对）
let (client, server) = mpsc_transport_pair();

// 发送请求
let response = client.send_request("session/new", params).await?;

// 发送通知（fire-and-forget）
client.send_notification("session/cancel", params).await?;

// 接收消息
if let Some(msg) = client.recv().await {
    // 处理 IncomingMessage
}
```

### 执行 Prompt

```rust
use peri_acp::session::executor::{execute_prompt, FrozenSessionData};

// 构建冻结数据
let frozen = FrozenSessionData {
    system_prompt: "...".to_string(),
    claude_md: Some("...".to_string()),
    claude_local_md: None,
    skill_summary: None,
    date: "2026-07-01".to_string(),
    is_git_repo: true,
    language: Some("zh-CN".to_string()),
};

// 执行 prompt（完整参数见 executor.rs）
let result = execute_prompt(
    &provider,
    peri_config,
    &cwd,
    content,
    Some(frozen),
    history,
    vec![],
    false,
    permission_mode,
    event_sink,
    cancel_token,
    broker,
    None, // shell_executor
    vec![],
    vec![],
    vec![],
    None,
    session_id,
    None,
    None,
    vec![],
    tool_search_index,
    shared_tools,
    vec![],
    None,
    pool,
    None,
    None,
    None,
    vec![],
).await;
```

## 依赖关系

```
peri-acp
  ├── peri-agent        # 核心 Agent 框架
  ├── peri-middlewares   # 中间件实现
  ├── peri-lsp          # LSP 客户端
  ├── langfuse-client   # 遥测客户端
  └── agent-client-protocol  # ACP 协议定义
```

## 相关文档

- [Agent 架构](../spec/global/domains/agent.md)
- [ACP 协议规范](https://agentclientprotocol.com/)
- [TUI 集成](../peri-tui/README.md)
