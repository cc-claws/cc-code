# 实施计划: 20260514_F002 - LLM 流式输出

## 依赖图

```
Step 1 (StreamingContext + Reasoning.streamed + ReactLLM trait 变更 + BaseModel::invoke_streaming)
  |
  +---> Step 2 (llm/sse.rs - SSE 解析基础设施)
  |       |
  |       +---> Step 3 (ChatOpenAI::invoke_streaming 流式实现)
  |       |       |
  |       |       +---> Step 6 (OpenAI 流式测试)
  |       |
  |       +---> Step 4 (ChatAnthropic::invoke_streaming 流式实现)
  |       |       |
  |       |       +---> Step 7 (Anthropic 流式测试)
  |       |
  |       +---> Step 5 (BaseModelReactLLM 分流 + 非流式适配 + RetryableLLM + SubAgent 守卫)
  |
  +---> Step 8 (Executor 适配 - llm_step.rs + final_answer.rs + tool_dispatch.rs)
          |
          +---> Step 9 (集成测试 + cargo test 全量通过)
```

Steps 3 和 4 依赖 Step 2，可并行开发。Step 5 依赖 Step 1 后可开发（与 3-4 并行）。Step 8 依赖 Step 5。Step 9 是最终验证。

---

## Step 1: 类型定义变更 -- StreamingContext + Reasoning.streamed + ReactLLM trait + BaseModel::invoke_streaming

**文件:**

- `peri-agent/src/llm/types.rs` -- 新增 `StreamingContext`
- `peri-agent/src/llm/mod.rs` -- `BaseModel` trait 新增 `invoke_streaming()` 默认方法
- `peri-agent/src/agent/react.rs` -- `Reasoning` 新增 `streamed` 字段 + `ReactLLM` trait 签名变更 + blanket impl 变更

### 1.1 新增 `StreamingContext`（`llm/types.rs`）

在 `llm/types.rs` 末尾（`StopReason` 之后、tests 之前）新增：

```rust
use std::sync::Arc;
use crate::agent::events::AgentEventHandler;
use crate::messages::MessageId;

/// 流式输出上下文，由 Executor 注入到 LLM 适配器。
/// LLM 适配器在 SSE 解析过程中通过 event_handler 发射增量事件。
pub struct StreamingContext {
    pub event_handler: Arc<dyn AgentEventHandler>,
    /// 预生成的 AI 消息 ID，所有增量 TextChunk 关联到此 ID
    pub message_id: MessageId,
}
```

### 1.2 `Reasoning` 新增 `streamed` 字段（`agent/react.rs`）

```rust
pub struct Reasoning {
    // ... 现有字段不变 ...
    /// 标记是否已通过事件流式发射过文本（由流式 LLM 适配器设为 true）
    pub streamed: bool,
}
```

更新所有 `Reasoning` 构造方法，添加 `streamed: false`：
- `Reasoning::with_tools()` -- 添加 `streamed: false`
- `Reasoning::with_answer()` -- 添加 `streamed: false`

**影响范围:** 所有现有的 `Reasoning` 构造点都会自动获得 `streamed: false`（向后兼容）。

### 1.3 `ReactLLM` trait 签名变更（`agent/react.rs`）

```rust
#[async_trait::async_trait]
pub trait ReactLLM: Send + Sync {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,  // 新增参数
    ) -> crate::error::AgentResult<Reasoning>;
    // ... model_name(), context_window() 不变
}
```

### 1.4 Blanket impl 变更（`agent/react.rs`）

```rust
#[async_trait::async_trait]
impl ReactLLM for Box<dyn ReactLLM + Send + Sync> {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,
    ) -> crate::error::AgentResult<Reasoning> {
        (**self).generate_reasoning(messages, tools, streaming).await
    }
}
```

### 1.5 `BaseModel` trait 新增 `invoke_streaming()`（`llm/mod.rs`）

```rust
#[async_trait::async_trait]
pub trait BaseModel: Send + Sync {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse>;
    fn provider_name(&self) -> &str;
    fn model_id(&self) -> &str;
    fn context_window(&self) -> u32 { 200_000 }

    /// 流式调用。默认实现回退到非流式 invoke()。
    /// 仅 ChatOpenAI 和 ChatAnthropic override 此方法实现 SSE 流式。
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        _ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        self.invoke(request).await
    }
}
```

**设计要点**：默认实现回退到非流式 `invoke()`，确保所有现有 `BaseModel` 实现者（如 LiteLLM 代理、测试 mock）无需改动。仅 `ChatOpenAI` 和 `ChatAnthropic` override 为 SSE 流式实现。

### 验证

```bash
cargo build -p peri-agent 2>&1 | head -50
```

预期：编译错误（trait impl 不匹配），确认所有需要更新的 impl 点。

---

## Step 2: SSE 解析基础设施 -- `llm/sse.rs`

**文件:** `peri-agent/src/llm/sse.rs`（新增）
**依赖:** 在 `llm/mod.rs` 中添加 `pub mod sse;`

### 设计

采用**有状态 push 解析器**（非 Stream 包装），每个 `push()` 调用消费一部分字节并返回已完成的完整事件：

```rust
/// 有状态 SSE 解析器。配合 reqwest bytes_stream() 使用：
/// let mut parser = SseParser::new();
/// while let Some(chunk) = stream.next().await {
///     for (event_type, data) in parser.push(&chunk?) {
///         // 按协议分发
///     }
///     if parser.is_done() { break; }
/// }
pub struct SseParser {
    pending_line: String,    // 跨 chunk 不完整行缓冲区
    event_type: Option<String>,  // 当前累积的 event type
    data: String,            // 当前累积的 data 文本
    done: bool,              // [DONE] 或流终止标志
}

impl SseParser {
    pub fn new() -> Self { ... }

    /// Push 新到达的字节块，返回此次推入后解析出的所有完整事件。
    /// 返回空 Vec 表示当前 chunk 内无完整事件（仍在累积中）。
    pub fn push(&mut self, bytes: &[u8]) -> Vec<(Option<String>, String)> { ... }

    /// 流是否已终止
    pub fn is_done(&self) -> bool { self.done }
}
```

### 关键实现细节

1. **行尾兼容:** 行尾可为 `\r\n`（CRLF）或 `\n`（LF）。先按 `\n` split，然后 trim 行尾的 `\r`。**不直接**按 `\n\n` 分割整个缓冲区。
2. **跨 chunk 拼接:** `pending_line` 保存上一个 chunk 未完成的最后一行。每次 push 时先 prepend `pending_line` 到新数据前。
3. **事件边界:** 连续空行（`\n\n`，含 `\r\n\r\n` 等价）触发事件 commit：将当前累积的 `event_type` 和 `data` 输出为一个 `(event_type, data)` 对，然后重置。
4. **`data: [DONE]` 终止:** 检测 `data:` 前缀 + 内容为 `[DONE]` 后设 `done = true`，返回空事件（不产出一对）。
5. **空 data 行:** `data:` 后无内容的行跳过。
6. **Anthropic event 行:** `event: content_block_delta\n` 设置 `event_type`，不触发事件 commit。
7. **OpenAI data 行:** `data: {...}\n` 追加到 `data` 累积字符串，不触发事件 commit（等空行边界）。

### 测试

内联测试覆盖：
- 基本单行 data 解析
- `\r\n` 行尾处理
- 跨 chunk 行拼接（含不完整行 split 在两个 chunk 边界）
- event + data 行对（Anthropic 格式）
- `data: [DONE]` 终止
- 空 `data:` 行被跳过
- 空流（首次 push 空字节）返回空 Vec

### 验证

```bash
cargo test -p peri-agent --lib -- sse
```

---

## Step 3: ChatOpenAI::invoke_streaming 实现

**文件:** `peri-agent/src/llm/openai.rs`

### 背景

TUI 路径调用链为 `BaseModelReactLLM.generate_reasoning()` → `ChatOpenAI`（作为 `Box<dyn BaseModel>`）的 `invoke_streaming()`。流式入口在 `invoke_streaming()` 方法中。

### 3.1 请求构建复用

`invoke()` 和 `invoke_streaming()` 共享请求构建逻辑。将现有 `invoke()` 中的消息序列化、system 注入、工具 JSON 构建提取为私有方法（如果尚未提取）：

```rust
impl ChatOpenAI {
    /// 构建请求消息列表（从 LlmRequest 中提取 messages + system 注入）
    fn build_request_messages(&self, request: &LlmRequest) -> Vec<Value> { ... }
    /// 构建请求体 JSON（messages + tools + model + 其他参数 + stream 标志）
    fn build_request_body(
        &self, messages: &[Value], tools_json: &[Value],
        request: &LlmRequest, stream: bool,
    ) -> Value { ... }
}
```

`invoke()` 保持调用上述方法 + `stream: false`。`invoke_streaming()` 调用上述方法 + `stream: true`。

### 3.2 新增 `BaseModel::invoke_streaming()` 实现

```rust
#[async_trait::async_trait]
impl BaseModel for ChatOpenAI {
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        // 1. 构建请求（复用 build_request_body，stream: true）
        // 2. 添加 "stream_options": {"include_usage": true}
        // 3. 发送 POST，获取 response.bytes_stream()
        // 4. 用 SseParser 解析字节流
        // 5. 循环 push() + 分发事件（见下方事件处理）
        // 6. 流结束后构建 source_message，返回 LlmResponse
    }
}
```

### 3.3 SSE 事件处理

```rust
let mut parser = SseParser::new();
let mut reasoning_text = String::new();
let mut content_text = String::new();
let mut tool_accums: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();

while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    for (event_type, data) in parser.push(&chunk) {
        let delta = &data["choices"][0]["delta"];

        // 推理增量（双字段兼容：reasoning_content + reasoning for GLM）
        if let Some(r) = delta["reasoning_content"].as_str().or_else(|| delta["reasoning"].as_str()) {
            if !r.is_empty() {
                ctx.event_handler.on_event(AgentEvent::AiReasoning(r.to_string()));
                reasoning_text.push_str(r);
            }
        }

        // 文本增量
        if let Some(c) = delta["content"].as_str() {
            if !c.is_empty() {
                ctx.event_handler.on_event(AgentEvent::TextChunk {
                    message_id: ctx.message_id.clone(),
                    chunk: c.to_string(),
                });
                content_text.push_str(c);
            }
        }

        // 工具调用累积（多 index 交错处理）
        if let Some(tc_array) = delta["tool_calls"].as_array() {
            for tc in tc_array {
                let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                let acc = tool_accums.entry(idx).or_insert_with(|| ToolCallAccumulator {
                    id: None, name: None, arguments_fragments: Vec::new(),
                });
                if let Some(id) = tc["id"].as_str() { acc.id = Some(id.to_string()); }
                if let Some(name) = tc["function"]["name"].as_str() { acc.name = Some(name.to_string()); }
                if let Some(args) = tc["function"]["arguments"].as_str() { acc.arguments_fragments.push(args.to_string()); }
            }
        }

        // finish_reason + usage（最后一条 chunk）
        if let Some(fr) = data["choices"][0]["finish_reason"].as_str() { ... }
        if let Some(u) = data["usage"].as_object() { ... }
    }
    if parser.is_done() { break; }
}

// 流结束后构建 ToolCall 列表
let tool_calls: Vec<ToolCall> = tool_accums.values()
    .map(|acc| ToolCall::new(acc.id.clone().unwrap_or_default(), acc.name.clone().unwrap_or_default(), 
        serde_json::from_str(&acc.arguments_fragments.join("")).unwrap_or(Value::Null)))
    .collect();

// 构建 source_message（⚠️ 必须复用 block_to_openai_part() 推理处理逻辑）
// source_message.content_blocks 中包含 ContentBlock::Reasoning(reasoning_text)
// ContentBlock::Text(content_text) 等
```

### 3.4 工具调用累积结构

```rust
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments_fragments: Vec<String>,
}
```

**边界处理**：流结束后若 `id` 或 `name` 为空，`tracing::warn!` 降级处理（部分 provider 实现不一致）。

### 3.5 推理回传防护（⚠️ 关键）

- `source_message` 中 `ContentBlock::Reasoning` 的序列化必须由 `block_to_openai_part()`（`openai.rs:152-160`）控制——仅 `supports_thinking_content` 时输出 `{"type":"thinking"}`。
- `reasoning_content` 顶层字段回传由 `messages_to_json()`（`openai.rs:219-221`）自动处理——只需确保 reasoning block 在 content_blocks 中存在。
- **禁止**在流式处理中引入新的推理序列化路径。

### 3.6 `ReactLLM for ChatOpenAI` 不变

`ChatOpenAI` 的 `ReactLLM` impl 中 `generate_reasoning()` 保持现有实现（TUI 路径不走此处）。新增 `streaming` 参数后设为 `streamed: false` 即可。

### 验证

```bash
cargo build -p peri-agent
```

---

## Step 4: ChatAnthropic::invoke_streaming 实现

**文件:** `peri-agent/src/llm/anthropic.rs`

### 4.1 新增 `BaseModel::invoke_streaming()` 实现

```rust
#[async_trait::async_trait]
impl BaseModel for ChatAnthropic {
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        // 1. 构建请求（复用现有 build_request_body，stream: true）
        // 2. 发送 POST，获取 response.bytes_stream()
        // 3. 用 SseParser 解析字节流
        // 4. 循环 push() + 分发事件（见下方事件处理表）
        // 5. 流结束后返回 LlmResponse
    }
}
```

### 4.2 SSE 事件处理

| event_type | delta type | 动作 |
|---|---|---|
| `message_start` | - | 提取 `usage.input_tokens` |
| `content_block_start` | `thinking` | 记录当前 block 类型；提取 `content_block.signature`（⚠️ 签名在此处） |
| `content_block_start` | `text` | 记录当前 block 类型 |
| `content_block_start` | `tool_use` | 记录工具 id + name |
| `content_block_delta` | `thinking_delta` | 发射 `AgentEvent::AiReasoning(delta.thinking)`，累积 |
| `content_block_delta` | `text_delta` | 发射 `AgentEvent::TextChunk { message_id, chunk }`，累积 |
| `content_block_delta` | `input_json_delta` | 累积工具参数 JSON 片段 |
| `content_block_stop` | - | 仅标记 block 结束，**不提取签名** |
| `message_delta` | - | 提取 `stop_reason` + `usage.output_tokens` |
| `message_stop` | - | 流结束 |

### 4.3 Thinking 签名提取（⚠️ 位置已修正）

签名在 `content_block_start` 事件中提取，**非** `content_block_stop`：

```rust
// content_block_start 事件处理中：
if event_type == "content_block_start" {
    let cb = &data["content_block"];
    if cb["type"].as_str() == Some("thinking") {
        if let Some(sig) = cb["signature"].as_str() {
            thinking_signature = Some(sig.to_string());
        }
    }
}
```

若后续 `content_block_delta` 也携带 `signature`，以首个为准（或覆盖以最后一个为准，待 Anthropic 文档确认）。

### 4.4 `ReactLLM for ChatAnthropic` 不变

与 OpenAI 一致——`ChatAnthropic` 的 `ReactLLM` impl 保持现有非流式实现，新增 `streaming` 参数后设 `streamed: false`（TUI 路径不走此处）。

### 4.5 Prompt Cache 兼容性

流式请求体的构建完全复用非流式 `build_request_body()`——仅加 `"stream": true`。`split_system_blocks()`、`apply_cache_to_messages()`、`__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 处理流程不变，cache 标记正常生成。

### 验证

```bash
cargo build -p peri-agent
```

---

## Step 5: BaseModelReactLLM 分流 + 非流式适配 + RetryableLLM + SubAgent 守卫

### 5.1 `BaseModelReactLLM` 分流（`llm/react_adapter.rs`，⚠️ 核心变更）

新增 `streaming` 参数并根据其值选择 `invoke()` 或 `invoke_streaming()`：

```rust
#[async_trait::async_trait]
impl ReactLLM for BaseModelReactLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        let tool_defs = tools.iter().map(|t| t.definition()).collect();
        let mut request = LlmRequest::new(messages.to_vec()).with_tools(tool_defs);
        if let Some(system) = &self.system { request = request.with_system(system.clone()); }
        if let Some(ref sid) = self.session_id { request = request.with_session_id(sid.clone()); }

        let streamed = streaming.is_some();
        let response = if let Some(ctx) = streaming {
            self.model.invoke_streaming(request, ctx).await?
        } else {
            self.model.invoke(request).await?
        };

        // 现有 response → Reasoning 转换逻辑不变，streamed 字段根据路径设置
        let mut reasoning = /* ... 现有构建逻辑 ... */;
        reasoning.streamed = streamed;
        Ok(reasoning)
    }
}
```

### 5.2 `MockLLM`（`llm/adapter.rs`）

忽略 `streaming` 参数，`streamed: false`：

```rust
async fn generate_reasoning(&self, messages, tools, _streaming: Option<StreamingContext>) -> AgentResult<Reasoning> {
    // 现有逻辑不变
}
```

### 5.3 `RetryableLLM`（`llm/retry.rs`）

首次调用透传 `streaming`。**重试时传 `None`**，防止同一 message_id 的流式 TextChunk 双重发射：

```rust
async fn generate_reasoning(
    &self, messages, tools, streaming: Option<StreamingContext>,
) -> AgentResult<Reasoning> {
    for attempt in 0..self.config.max_retries {
        let retry_streaming = if attempt == 0 { streaming.clone() } else { None };
        match self.inner.generate_reasoning(messages, tools, retry_streaming).await {
            Ok(r) => return Ok(r),
            Err(e) if e.is_retryable() => {
                // 重试逻辑不变，但 emit LlmRetrying 时注意 streaming 已为 None
                ...
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

**`StreamingContext` Clone 实现**：需为 `StreamingContext` 实现 `Clone`（`event_handler: Arc<...>` 天然 Clone，`message_id: MessageId` 需 `#[derive(Clone)]`）。`MessageId` 当前已实现 Clone。

### 5.4 `Box<dyn ReactLLM>` blanket impl（`agent/react.rs`）

透传 `streaming`：

```rust
async fn generate_reasoning(&self, messages, tools, streaming: Option<StreamingContext>) -> AgentResult<Reasoning> {
    (**self).generate_reasoning(messages, tools, streaming).await
}
```

### 5.5 SubAgent 路径守卫（`peri-middlewares/` 不调用处）

SubAgent（Fork/Normal）共享父 Agent 的 `event_handler`（`subagent/tool.rs:345-346`），若传入 `StreamingContext` 会导致流式事件泄露。

**实现**：SubAgent 的 executor 中 `agent.llm.generate_reasoning(state.messages(), tool_refs, None)`——传 `None`，不传 `StreamingContext`。Background Agent 本就未共享 `event_handler`（`tool.rs:501-503`），无此问题。

在 `ReActAgent` 的 `run()` 或 SubAgent 组装处添加注释：
```rust
// SubAgent 不启用流式输出，防止流式事件通过共享 event_handler 泄露到父 TUI
// (CLAUDE.md: issue_2026-05-13-sync-subagent-events-leak-to-parent)
let streaming = None::<StreamingContext>;
```

### 5.6 其他实现者

| 实现者 | 文件 | 适配方式 |
|--------|------|----------|
| `ChatOpenAI.generate_reasoning()` | `openai.rs:640` | 忽略 `streaming`，`streamed: false`（TUI 不走此路径） |
| `ChatAnthropic.generate_reasoning()` | `anthropic.rs:835` | 同上 |
| `EchoLLM`（SubAgent 测试） | `subagent/mod.rs:373` | 忽略 `_streaming` |
| SubAgent 测试 mock ×8 | `subagent/tool_test.rs` | 忽略 `_streaming` |
| Executor 测试 mock ×17 | `mod_test.rs` | 忽略 `_streaming` |
| Prompt Hook 调用 | `hooks/executor.rs:192` | 传 `None` |

### 验证

```bash
cargo build -p peri-agent
cargo build -p peri-middlewares
```
预期：所有 mock 和适配器编译通过。

---

## Step 6: OpenAI 流式测试

**文件:** `peri-agent/src/llm/openai_test.rs`（新增）

| 测试名 | 场景 |
|--------|------|
| `test_openai_stream_text_only` | 纯文本流式：逐 chunk 发射 TextChunk，最终返回 streamed: true |
| `test_openai_stream_with_reasoning` | 含 reasoning_content 的流式 |
| `test_openai_stream_with_tool_calls` | 工具调用参数累积 |
| `test_openai_stream_usage_extraction` | 流式 mode 下 token usage 从最后 chunk 提取 |
| `test_openai_stream_error_mid_stream` | 流中途 HTTP 错误返回 Err |

### 验证

```bash
cargo test -p peri-agent --lib -- openai_test
```

---

## Step 7: Anthropic 流式测试

**文件:** `peri-agent/src/llm/anthropic.rs` tests 模块（新增）

| 测试名 | 场景 |
|--------|------|
| `test_anthropic_stream_text_only` | 纯文本流式 |
| `test_anthropic_stream_with_thinking` | Extended Thinking 流式 + signature 提取 |
| `test_anthropic_stream_with_tool_use` | 工具调用 input_json_delta 累积 |
| `test_anthropic_stream_usage_from_events` | 从 message_start + message_delta 提取 usage |

### 验证

```bash
cargo test -p peri-agent --lib -- anthropic
```

---

## Step 8: Executor 适配

### 8.1 `llm_step.rs` -- 注入 StreamingContext

在 `call_llm()` 中 `LlmCallStart` 之后、`generate_reasoning()` 调用之前，构建 `StreamingContext`：

```rust
let message_id = crate::messages::MessageId::new();
let streaming = agent.event_handler.as_ref().map(|h| {
    crate::llm::types::StreamingContext {
        event_handler: Arc::clone(h),
        message_id,
    }
});

let reasoning = tokio::select! {
    biased;
    _ = cancel.cancelled() => { return Err(AgentError::Interrupted); }
    result = agent.llm.generate_reasoning(state.messages(), tool_refs, streaming) => { result? }
};
```

**注意**：`message_id` 先于 `generate_reasoning()` 生成。非流式路径中，此 `message_id` 仍传递给 `final_answer.rs` 中的 `TextChunk`（非流式时），确保与流式路径使用相同语义的 UUID。

### 8.2 `final_answer.rs` -- 条件发射 TextChunk

```rust
if !reasoning.streamed {
    agent.emit(AgentEvent::TextChunk {
        message_id: ai_msg_id,
        chunk: answer.clone(),
    });
}
```

### 8.3 `tool_dispatch.rs` -- 条件发射工具前文本

```rust
if !reasoning.streamed && !reasoning.thought.trim().is_empty() {
    agent.emit(AgentEvent::TextChunk {
        message_id: ai_msg_id,
        chunk: reasoning.thought.clone(),
    });
}
```

### 8.4 TUI Error 路径 -- 流式缓冲区清理

`handle_agent_event(Error(...))`（`agent_ops.rs`）中新增 `pipeline.interrupt()` 调用，清理流式错误后残留的 `current_ai_text`/`current_ai_reasoning` 缓冲区。或保留流式内容并附加错误标记——由 TUI UX 团队决策。

### 验证

```bash
cargo build -p peri-tui
cargo test -p peri-agent --lib -- executor
```

---

## Step 9: 集成测试 + 全量验证

### 验证清单

```bash
# 全量测试
cargo test -p peri-agent

# Clippy
cargo clippy -p peri-agent

# 上层 crate 编译
cargo build -p peri-middlewares
cargo build -p peri-tui
```

### 集成测试（`agent/executor/mod_test.rs` 新增）

| 测试名 | 场景 |
|--------|------|
| `test_executor_streaming_text_chunk_not_duplicated` | 流式模式下 final_answer 不重复发射 TextChunk |
| `test_executor_non_streaming_unchanged` | 非流式 LLM 行为完全不变 |
| `test_mock_llm_streaming_param_ignored` | MockLLM 忽略 streaming 参数 |

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| SSE 行解析边界 -- bytes 不保证按行分割，行尾可能是 `\r\n` | `SseParser` 使用 `pending_line` 缓冲区 + 先按 `\n` split 再 trim `\r`，测试覆盖跨 chunk + `\r\n` |
| 流式重试导致同一 message_id 双重发射 | `RetryableLLM` 重试时传 `streaming: None`（走非流式），不使用原 StreamingContext |
| OpenAI 工具调用参数 JSON 分片 + 多 index 交错 | `BTreeMap<usize, ToolCallAccumulator>` 按 index 管理，流结束后统一 join + parse |
| Anthropic thinking signature 位置 | `content_block_start` 事件中提取（已修正，非 `content_block_stop`） |
| 推理内容序列化触发 DeepSeek/GLM 已知 TRAP | 流式构建 source_message 必须复用 `block_to_openai_part()` 推理处理逻辑，禁止新序列化路径 |
| SubAgent 流式事件泄露到父 TUI | SubAgent 路径传 `streaming: None`（Fork/Normal 不启用流式），Background Agent 本就隔离 |
| 流式错误后幽灵消息残留 | TUI Error 路径调用 `pipeline.interrupt()` 清理缓冲区，或保留+附加错误标记 |
| TUI 架构路径错位 | 流式入口在 `BaseModelReactLLM.generate_reasoning()` → `BaseModel::invoke_streaming()`，不做无效实现 |
| `peri-agent` 中 reqwest 已有 `stream` feature | `Cargo.toml` 已声明，无需新增依赖 |

## 无新 crate 依赖

所有改动使用现有依赖：`reqwest`(stream)、`futures`(StreamExt)、`tokio`、`serde_json`、`bytes`。
