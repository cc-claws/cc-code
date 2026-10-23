# cc-agent

Rust Agent 框架，实现 ReAct 循环与可组合中间件系统。与 TypeScript 端的 `@langgraph-js/standard-agent` 在概念上对齐。

## 概述

`cc-agent` 是 cc-code 的核心 crate，提供：

- **ReAct 循环**：推理 → 工具调用 → 结果反馈的完整流程
- **中间件系统**：可组合的生命周期钩子
- **工具系统**：统一的工具注册和调用接口
- **LLM 适配层**：支持 Anthropic、OpenAI 兼容 API
- **持久化**：SQLite 会话存储
- **日志**：tracing 订阅器初始化（Langfuse 集成在 `cc-acp`，不在本 crate）
- **上下文压缩**：auto compact 和 micro compact

## 快速开始

```rust
use cc_agent::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = cc_agent::telemetry::init_tracing("my-agent");

    let agent = ReActAgent::new(MockLLM::always_answer("任务完成"))
        .max_iterations(10)
        .add_middleware(Box::new(LoggingMiddleware::new().verbose()));

    let mut state = AgentState::new("/workspace");
    let output = agent.execute(AgentInput::text("请帮我完成这个任务"), &mut state, None).await?;

    println!("回答：{}", output.text);
    println!("步骤：{}", output.steps);
    Ok(())
}
```

## 核心概念

### ReActAgent

ReAct 循环的执行器，管理 LLM 推理 → 工具调用 → 结果反馈的完整流程。

```rust
let agent = ReActAgent::new(llm)
    .max_iterations(20)             // 最大循环步数，默认 10
    .register_tool(Box::new(my_tool))
    .add_middleware(Box::new(LoggingMiddleware::new()))
    .with_event_handler(Arc::new(handler));
```

### 中间件（Middleware）

通过实现 `Middleware<S>` trait 在 Agent 生命周期各节点插入逻辑。

```rust
use async_trait::async_trait;
use cc_agent::prelude::*;

struct MyMiddleware;

#[async_trait]
impl Middleware<AgentState> for MyMiddleware {
    fn name(&self) -> &str { "my-middleware" }

    async fn before_agent(&self, state: &mut AgentState) -> AgentResult<()> {
        // Agent 开始前执行
        Ok(())
    }

    async fn before_tool(&self, _: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        // 工具调用前执行，可修改调用参数
        Ok(call.clone())
    }

    async fn after_tool(&self, _: &mut AgentState, _: &ToolCall, _: &ToolResult) -> AgentResult<()> {
        // 工具调用后执行
        Ok(())
    }

    async fn after_agent(&self, _: &mut AgentState, output: &AgentOutput) -> AgentResult<AgentOutput> {
        // Agent 完成后执行，可修改最终输出
        Ok(output.clone())
    }
}
```

生命周期钩子执行顺序：`before_agent` → (每步) `before_tool` → `after_tool` → `after_agent`，出错时触发 `on_error`。

### 自定义工具（Tool）

```rust
use async_trait::async_trait;
use cc_agent::tools::BaseTool;

struct EchoTool;

#[async_trait]
impl BaseTool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "原样返回输入内容" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        })
    }

    async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(input["message"].as_str().unwrap_or("").to_string())
    }
}
```

### 事件回调（EventHandler）

在不修改中间件的情况下监听关键事件：

```rust
use std::sync::Arc;
use cc_agent::prelude::*;

let handler = FnEventHandler(|event| match event {
    AgentEvent::ToolStart { name, .. } => println!("开始调用工具: {name}"),
    AgentEvent::ToolEnd { name, is_error, .. } => println!("工具 {name} 完成，错误={is_error}"),
    AgentEvent::TextChunk { chunk, .. } => println!("回答: {chunk}"),
    AgentEvent::LlmCallEnd { step, .. } => println!("步骤 {step} 完成"),
});

let agent = ReActAgent::new(llm)
    .with_event_handler(Arc::new(handler));
```

## Telemetry（可观测性）

### 基本用法

在 `main` 入口调用一次，其余自动处理：

```rust
let _guard = cc_agent::telemetry::init_tracing("my-agent");
// _guard 必须存活到程序退出，drop 时自动 flush
```

### 开关控制

本 crate 的 telemetry 只做 tracing 日志初始化，不含 OTLP 导出（无 `otel` Cargo feature，`cc-agent/Cargo.toml` 未声明任何 feature）：

| 环境变量              | 说明                                                            |
| --------------------- | --------------------------------------------------------------- |
| `RUST_LOG`            | 日志级别，默认 `info`                                           |
| `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出（默认 pretty）                              |
| `RUST_LOG_FILE`       | 日志文件路径（最高优先级）；未设置时写入 `~/.cc-code/logs/{service_name}.log` |

```bash
# 调整日志级别
RUST_LOG=debug cargo run
RUST_LOG=cc_agent=trace cargo run
```

`ReActAgent::execute()`、每次工具调用均已通过 tracing span 自动埋点，无需额外代码。Langfuse 追踪由 `cc-acp` 的 `langfuse/` 模块与独立的 `langfuse-client` crate 承担。

## 核心模块

| 模块 | 职责 |
|------|------|
| `agent/` | ReAct 循环、AgentState、AgentOutput |
| `middleware/` | Middleware trait 定义 |
| `tools/` | BaseTool trait、工具注册 |
| `llm/` | LLM 适配层（BaseModel、ReactLLM） |
| `messages/` | BaseMessage / ContentBlock 消息类型 |
| `agent/compact/` | 上下文压缩（full_compact、micro_compact） |
| `thread/` | ThreadStore 会话持久化（SQLite + 文件系统） |
| `telemetry/` | tracing 日志初始化 |

工具懒加载（tool_search）不在本 crate，位于 `cc-middlewares/src/tool_search/`。

## ReAct 循环详解

```
AgentInput
  → collect_tools (收集可用工具)
  → before_agent (中间件钩子)
  → loop(max_iterations) {
      before_model (中间件钩子)
      → LLM 推理
      → after_model (中间件钩子)
      → [工具调用] {
          before_tool (中间件钩子)
          → 并发执行工具
          → after_tool (中间件钩子)
          → emit ToolResult
        }
      → [回答] {
          emit TextChunk + StateSnapshot
        }
    }
  → after_agent (中间件钩子)
```

## 消息类型

```rust
// 消息类型（所有变体均含 id: MessageId）
pub enum BaseMessage {
    Human { id: MessageId, content: MessageContent },
    Ai { id: MessageId, content: MessageContent, tool_calls: Vec<ToolCallRequest> },
    System { id: MessageId, content: MessageContent },
    Tool { id: MessageId, tool_call_id: String, content: MessageContent, is_error: bool },
}

// 内容块类型
pub enum ContentBlock {
    Text { text: Arc<str> },
    Image { source: ImageSource },
    Document { source: DocumentSource, title: Option<String> },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { id: Option<String>, tool_use_id: String, content: Vec<ContentBlock>, is_error: bool },
    Reasoning { text: String, signature: Option<String>, duration_ms: Option<u64> },
    Unknown(Value),
}
```

## LLM 适配层

```rust
// BaseModel trait
pub trait BaseModel: Send + Sync {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse>;
    fn provider_name(&self) -> &str;
    fn model_id(&self) -> &str;
    fn context_window(&self) -> u32 { 200_000 }
    async fn invoke_streaming(&self, request: LlmRequest, ctx: StreamingContext) -> AgentResult<LlmResponse>;
}

// 已实现的 Provider
- Anthropic (Claude)
- OpenAI 兼容 (GPT-4, DeepSeek, GLM, MiMo, Kimi)
```

### Thinking/推理模式

Thinking 配置在 `cc-acp` 的 `ThinkingConfig` 中管理，通过 `PeriConfig` 注入：

```rust
// cc-acp::provider::config::ThinkingConfig
pub struct ThinkingConfig {
    pub enabled: bool,
    pub budget_tokens: u32,   // 推理 token 预算
    pub effort: String,       // "low" / "medium" / "high"
    pub max_tokens: u32,      // 最大输出 token 数
}
```

## 工具懒加载

三层工具系统：

| 层级 | 说明 | 示例 |
|------|------|------|
| **Core** | 始终对 LLM 可见 | Read, Write, Edit, Glob, Grep, Bash |
| **Meta** | 始终可见，用于发现 | SearchExtraTools, ExecuteExtraTool |
| **Deferred** | 按需加载 | Cron, MCP, Lsp |

```rust
// 核心工具白名单（定义位于 cc-middlewares/src/tool_search/core_tools.rs，
// 类型为 LazyLock<HashSet<&'static str>>，共 11 个）
const CORE_TOOLS: &[&str] = &[
    "Read", "Write", "Edit", "Glob", "Grep",
    "Bash", "WebFetch", "WebSearch", "Agent",
    "AskUserQuestion", "TodoWrite",
];
```

## 上下文压缩

两种压缩模式：

| 模式 | 触发阈值 | 说明 |
|------|----------|------|
| Micro Compact | 70% | 轻量级，压缩历史消息 |
| Full Compact | 85% | 完整压缩，生成摘要 |

```rust
// 环境变量控制
DISABLE_COMPACT=true        // 禁用所有压缩
DISABLE_AUTO_COMPACT=true   // 仅禁用自动压缩
COMPACT_THRESHOLD=0.9       // 覆盖阈值
```

## 持久化

通过 `ThreadStore` trait 实现会话持久化，支持 SQLite 和文件系统两种后端：

```rust
// ThreadStore trait（thread/store.rs）
pub trait ThreadStore: Send + Sync {
    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>>;
    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta>;
    async fn save_messages(&self, id: &ThreadId, messages: &[BaseMessage]) -> Result<()>;
    // ...
}
```

实现：`SqliteThreadStore`（`thread/sqlite_store.rs`）、`FilesystemThreadStore`（`thread/filesystem.rs`）

## 相关文档

- [CLAUDE.md](../CLAUDE.md) — 项目开发指南
- [中间件文档](../cc-middlewares/README.md)
- [ACP 集成](../cc-acp/README.md)
- [TUI 集成](../cc-tui/README.md)
