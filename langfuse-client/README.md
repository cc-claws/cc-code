# langfuse-client

[Langfuse](https://langfuse.com) 遥测客户端，用于 LLM 调用追踪和可观测性。

## 概述

`langfuse-client` 提供：

- **OTLP 协议**：通过 OpenTelemetry OTLP 端点发送追踪数据
- **批量上报**：异步批量发送，减少网络开销
- **自动重试**：网络失败时指数退避重试
- **背压控制**：队列满时的丢弃/阻塞策略

## 核心模块

| 模块 | 职责 |
|------|------|
| `client.rs` | Langfuse OTLP 客户端 |
| `batcher.rs` | 异步批量上报器 |
| `config.rs` | 配置和策略定义 |
| `types/` | 数据类型定义（IngestionEvent、TraceBody 等） |
| `error.rs` | 错误类型 |

## 使用示例

### 基础用法

```rust
use langfuse_client::{LangfuseClient, ClientConfig};

// 从环境变量创建配置
let config = ClientConfig::from_env()?;

// 创建客户端
let client = LangfuseClient::new(
    &config.public_key,
    &config.secret_key,
    &config.base_url,
    3, // max_retries
);

// 发送事件
let events = vec![/* ... */];
client.ingest(events).await?;
```

### 批量上报

```rust
use std::time::Duration;
use langfuse_client::{Batcher, BatcherConfig, BackpressurePolicy};

let config = BatcherConfig {
    max_events: 100,
    flush_interval: Duration::from_secs(5),
    backpressure: BackpressurePolicy::DropNew,
    max_retries: 3,
};

let batcher = Batcher::new(client, config);

// 添加事件（异步入队）
batcher.add(event).await?;

// 手动刷新
batcher.flush().await?;

// Batcher drop 时自动 flush 剩余事件
```

### 数据类型

```rust
use langfuse_client::{IngestionEvent, TraceBody, GenerationBody};

// 创建 Trace 事件
let trace_event = IngestionEvent::TraceCreate {
    id: "trace-1".to_string(),
    timestamp: chrono::Utc::now().to_rfc3339(),
    body: TraceBody {
        id: Some("trace-1".to_string()),
        name: Some("user-query".to_string()),
        user_id: Some("user-123".to_string()),
        metadata: Some(serde_json::json!({ "env": "production" })),
        tags: Some(vec!["production".to_string()]),
        ..Default::default()
    },
    metadata: None,
};

// 创建 Generation 事件
let gen_event = IngestionEvent::GenerationCreate {
    id: "gen-1".to_string(),
    timestamp: chrono::Utc::now().to_rfc3339(),
    body: GenerationBody {
        id: Some("gen-1".to_string()),
        trace_id: Some("trace-1".to_string()),
        name: Some("openai-chat".to_string()),
        model: Some("gpt-4".to_string()),
        input: Some(serde_json::json!({ "messages": [...] })),
        output: Some(serde_json::json!({ "content": "..." })),
        ..Default::default()
    },
    metadata: None,
};
```

## 背压策略

```rust
enum BackpressurePolicy {
    /// 队列满时丢弃新事件（默认）
    DropNew,
    /// 队列满时阻塞等待
    Block,
}
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `LANGFUSE_PUBLIC_KEY` | 公钥 | 必填 |
| `LANGFUSE_SECRET_KEY` | 秘钥 | 必填 |
| `LANGFUSE_BASE_URL` | 服务地址 | `https://cloud.langfuse.com` |

## 与 Agent 集成

`langfuse-client` 被 `peri-acp` 使用，自动追踪 Agent 的 LLM 调用：

```
Agent LLM 调用
  → peri-acp/langfuse 模块
  → langfuse-client Batcher
  → Langfuse OTLP API
  → 可观测性面板
```

## 依赖关系

```
langfuse-client
  ├── reqwest         # HTTP 客户端
  ├── serde_json      # JSON 序列化
  ├── tokio           # 异步运行时
  ├── chrono          # 时间处理
  └── base64          # 编码
```

## 测试

```bash
cargo test -p langfuse-client
```
