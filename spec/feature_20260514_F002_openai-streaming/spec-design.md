# Feature: 20260514_F002 - LLM 流式输出

## 需求背景

当前 OpenAI 和 Anthropic 两个 LLM 适配器均使用非流式请求（`stream: false`），等待完整响应后才返回。用户在 TUI 中看到的是"长时间无响应 → 突然出现完整回答"的体验，尤其长回复时体感延迟明显。

TUI 层已完整支持增量渲染（`TextChunk`/`AiReasoning` 事件 + `message_pipeline.rs` 增量处理路径），但 LLM 层从未产生过增量事件——`TextChunk` 仅在 `final_answer.rs` 中一次性发射完整文本。

需要改造 LLM 层，使 OpenAI 和 Anthropic 适配器都通过 SSE 流式输出，文本和推理（reasoning）内容均逐 token 流式展示。

## 目标

- OpenAI 兼容接口（含 DeepSeek/GLM）支持 SSE 流式文本和推理输出
- Anthropic Messages API 支持 SSE 流式文本和 thinking 输出
- TUI 层零改动（已有增量渲染能力）
- 非流式实现（MockLLM、BaseModelReactLLM）向后兼容

## 方案设计

### 架构概览

采用**回调注入**方案：在 `ReactLLM::generate_reasoning()` 中注入可选的 `StreamingContext`，LLM 适配器在 SSE 解析过程中通过 `event_handler` 直接发射 `AgentEvent::TextChunk`/`AiReasoning` 事件。

**关键架构事实**：TUI 的 LLM 调用链是 `RetryableLLM<BaseModelReactLLM<Box<dyn BaseModel>>>`（见 `peri-tui/src/app/agent.rs:173-174`、`src/acp/agent_assembler.rs:15`）。`ChatOpenAI` 和 `ChatAnthropic` 在此路径中作为 `Box<dyn BaseModel>`（`BaseModel` trait 实现者）被包装在 `BaseModelReactLLM` 内——它们的 `ReactLLM` impl **在 TUI 路径中不会被调用**。

因此流式入口在 `BaseModelReactLLM.generate_reasoning()` 中，而非 `ChatOpenAI.generate_reasoning()`。新增 `BaseModel::invoke_streaming()` 可选方法，仅 `ChatOpenAI` 和 `ChatAnthropic` override 实现 SSE 流式。

```
Executor (llm_step.rs)
  ├── 预生成 MessageId
  ├── 构建 StreamingContext { event_handler, message_id }
  └── 调用 RetryableLLM.generate_reasoning(messages, tools, Some(streaming_ctx))
        └── RetryableLLM 透传 streaming → BaseModelReactLLM.generate_reasoning()
              ├── streaming == Some(ctx)
              │     └── self.model.invoke_streaming(request, ctx)
              │           ├── ChatOpenAI.invoke_streaming: SSE 解析 → 逐 chunk emit → 返回 LlmResponse
              │           └── ChatAnthropic.invoke_streaming: SSE 解析 → 逐 chunk emit → 返回 LlmResponse
              ├── streaming == None
              │     └── self.model.invoke(request)  ← 非流式，行为不变
              └── 返回 Reasoning { streamed: bool, ... }

Executor (final_answer.rs)
  └── reasoning.streamed == true → 跳过一次性 TextChunk 发射
```

### 类型定义

**新增 `StreamingContext`**（`llm/types.rs`）：

```rust
/// 流式输出上下文，由 Executor 注入到 LLM 适配器。
/// LLM 适配器在 SSE 解析过程中通过 event_handler 发射增量事件。
pub struct StreamingContext {
    pub event_handler: Arc<dyn AgentEventHandler>,
    /// 预生成的 AI 消息 ID，所有增量 TextChunk 关联到此 ID
    pub message_id: MessageId,
}
```

### BaseModel trait 变更（`llm/mod.rs`）

新增可选的流式调用方法，默认实现回退到非流式：

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

需要在 `BaseModelReactLLM.generate_reasoning()` 中根据 `streaming` 选择路径：

```rust
impl ReactLLM for BaseModelReactLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        let request = /* 构建请求 */;
        let response = if let Some(ctx) = streaming {
            self.model.invoke_streaming(request, ctx).await?
        } else {
            self.model.invoke(request).await?
        };
        // response → Reasoning，streamed 标志根据路径设置
    }
}
```

### ReactLLM trait 变更

```rust
#[async_trait::async_trait]
pub trait ReactLLM: Send + Sync {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        streaming: Option<StreamingContext>,  // 新增
    ) -> AgentResult<Reasoning>;
    // ...
}
```

**所有 `generate_reasoning` 实现者需同步更新**（完整清单，含行号）：

| 实现者 | 文件:行 | 适配方式 |
|--------|---------|----------|
| `BaseModelReactLLM` | `react_adapter.rs:40` | 使用 streaming — 选择 `invoke` vs `invoke_streaming` |
| `RetryableLLM<L>` | `retry.rs:89` | 透传 `streaming` 到 `self.inner.generate_reasoning()` |
| `Box<dyn ReactLLM>` blanket | `react.rs:186` | 透传 `streaming` |
| `MockLLM` | `adapter.rs:47` | 忽略 `streaming`，`streamed: false` |
| `ChatOpenAI` | `openai.rs:640` | 忽略（TUI 不走此路径），`streamed: false` |
| `ChatAnthropic` | `anthropic.rs:835` | 忽略（TUI 不走此路径），`streamed: false` |
| `EchoLLM`（SubAgent 测试） | `subagent/mod.rs:373` | 忽略 `streaming` |
| SubAgent 测试 mock ×8 | `subagent/tool_test.rs` | 忽略 `streaming` |
| Executor 测试 mock ×17 | `mod_test.rs` | 忽略 `streaming` |
| Prompt Hook 调用 | `hooks/executor.rs:192` | 传 `None` |

**`RetryableLLM` 关键考量**：重试时 `streaming` 参数需**克隆 StreamId**（可 Clone 的 UUID）+ `Arc::clone(&event_handler)`。第一次流式调用已在 SSE 过程中发射了大量 TextChunk——重试时如果仍传入同一个 `StreamingContext`，`event_handler` 会为同一个 `message_id` 重复发射 chunk。**TUI 的 pipeline 对同一 message_id 的重复 push_chunk 会累积文本而非覆盖**，因此重试会导致双重显示。

**建议**：在 `RetryableLLM` 的重试路径中，`streaming` 设为 `None`（重试走非流式），或将 `StreamingContext` 设计为可重置状态（新增 `retry_count: u32` 字段，pipeline 端根据计数过滤）。

### Reasoning 结构体变更

```rust
pub struct Reasoning {
    // ... 现有字段不变 ...
    /// 标记是否已通过事件流式发射过文本（由流式 LLM 适配器设为 true）
    pub streamed: bool,
}
```

### OpenAI SSE 流式解析

**请求变更**：
- `"stream": true`（当前为 `false`）
- 新增 `"stream_options": {"include_usage": true}` 以获取 token usage

**SSE 数据格式**：
```
data: {"choices":[{"delta":{"role":"assistant"},...}]}     ← 首条，角色声明
data: {"choices":[{"delta":{"reasoning_content":"..."}},...]}  ← 推理增量
data: {"choices":[{"delta":{"content":"..."}},...]}         ← 文本增量
data: {"choices":[{"delta":{"tool_calls":[...]}},...]}     ← 工具调用增量
data: {"choices":[{"delta":{},"finish_reason":"stop"}], "usage":{...}}  ← 结束 + usage
data: [DONE]
```

**事件映射**：

| delta 字段 | 发射事件 |
|---|---|
| `delta.reasoning_content`（非空） | `AgentEvent::AiReasoning(chunk)` |
| `delta.content`（非空） | `AgentEvent::TextChunk { message_id, chunk }` |
| `delta.tool_calls[i]` | 累积到 `ToolCallAccumulator`（见下方），流结束后构建完整 `ToolCall` |

**工具调用累积**（`openai.rs` 新增）：

OpenAI SSE 中 `delta.tool_calls` 数组可能包含多个 index，且同一 index 的不同字段（id/name/arguments）可能跨 chunk 分片到达，不同 index 会交错出现。需要维护按 index 索引的 accumulator：

```rust
struct ToolCallAccumulator {
    id: Option<String>,        // 首次出现时设置，后续 chunk 不重复
    name: Option<String>,      // 首次出现时设置，后续 chunk 不重复
    arguments_fragments: Vec<String>,  // 累积所有 arguments 片段
}

// SSE 解析循环中
let mut tool_accums: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
for delta_tc in &delta.tool_calls {
    let idx = delta_tc.index;
    let acc = tool_accums.entry(idx).or_default();
    if let Some(ref id) = delta_tc.id { acc.id = Some(id.clone()); }
    if let Some(ref name) = delta_tc.function.name { acc.name = Some(name.clone()); }
    if let Some(ref args) = delta_tc.function.arguments { acc.arguments_fragments.push(args.clone()); }
}
```

流结束（`finish_reason == "tool_calls"`）后，按 index 排序，依次 join arguments_fragments 并解析为 JSON → 构建 `ToolCall`。若 `id` 或 `name` 为空，`tracing::warn!` 降级处理（部分 provider 的实现不一致）。

**推理回传防护**（引用 CLAUDE.md 已知 TRAP）：

流式构建 `source_message` 时，推理内容（reasoning）的处理必须复用现有的 `supports_thinking_content` 判断逻辑（`openai.rs:153-160`）：

- **TRAP 1** (`DeepSeek unknown variant 'thinking'`)：Reasoning block 不应序列化为 `{"type":"thinking"}` 发给不支持的 provider。仅 `deepseek-v4-pro` 开启 `supports_thinking_content`。流式构建的 `source_message` 中 `ContentBlock::Reasoning` 的序列化由 `block_to_openai_part()`（`openai.rs:152-160`）统一处理——流式路径必须使用相同的序列化逻辑。
- **TRAP 2** (`reasoning_content must be passed back`)：过滤 `Reasoning` 时必须同时作为顶层 `reasoning_content` 字段回传。现有 `messages_to_json()`（`openai.rs:219-221`）已有统一的回传处理——流式构建的 `source_message` 只需确保 reasoning block 正确存在于 content_blocks 中即可，顶层回传由现有逻辑自动完成。

**关键要求**：流式结束后构建 `source_message` 必须复用或等价于 `parse_assistant_message()` 的非推理构建逻辑，不可引入新的序列化路径。

**请求构建重构**：将 `invoke()` 中的消息序列化、system 注入、工具 JSON 构建逻辑提取为 `build_request_messages()` 和 `build_request_body()` 共享方法。`generate_reasoning()` 复用这些方法，将 `stream` 设为 `true` 后发起 SSE 请求。

**reasoning_content 双字段**：流式 delta 中同时检查 `reasoning_content` 和 `reasoning`（GLM 兼容）。

### Anthropic SSE 流式解析

**请求变更**：body 加 `"stream": true`。

**SSE 数据格式**：
```
event: message_start
data: {"type":"message_start","message":{...,"usage":{"input_tokens":...}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"..."}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"..."}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":...}}

event: message_stop
data: {"type":"message_stop"}
```

**事件映射**：

| SSE event + delta type | 发射事件 |
|---|---|
| `content_block_delta` + `thinking_delta` | `AgentEvent::AiReasoning(thinking)` |
| `content_block_delta` + `text_delta` | `AgentEvent::TextChunk { message_id, chunk }` |
| `content_block_start` + `tool_use` | 记录工具名和 id（累积） |
| `content_block_delta` + `input_json_delta` | 累积工具参数 JSON 片段 |
| `message_start` | 提取 input_tokens |
| `message_delta` | 提取 stop_reason + output_tokens |

**Extended Thinking**：thinking block 通过 `thinking_delta` 流式输出。签名从 thinking block 的 **`content_block_start`** 事件的 `content_block.signature` 字段中提取（**注意**：`content_block_stop` 不含 `signature` 字段，Anthropic 将签名携带在首个 `content_block_start` 中）。若后续 thinking delta 也携带 `signature`，以首个为准。

**Prompt Cache**：流式请求与 Prompt Cache 兼容，无需特殊处理。`__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 逻辑保持不变。

### Executor 适配

**`llm_step.rs`**：
```rust
// 预生成 message_id
let message_id = MessageId::new();
let streaming = agent.event_handler.as_ref().map(|h| StreamingContext {
    event_handler: Arc::clone(h),
    message_id,
});

let reasoning = agent.llm.generate_reasoning(
    state.messages(), tool_refs, streaming
).await?;
```

**`final_answer.rs`**：
```rust
// 仅在非流式时发射 TextChunk（流式已在 LLM 适配器中逐 chunk 发射）
if !reasoning.streamed {
    agent.emit(AgentEvent::TextChunk {
        message_id: ai_msg_id,
        chunk: answer.clone(),
    });
}
```

### 非流式实现适配

**`BaseModelReactLLM`**：根据 `streaming` 参数选择 `invoke_streaming()` 或 `invoke()`。流式返回时设 `Reasoning { streamed: true }`，非流式 `streamed: false`。

**`MockLLM`**：忽略 `streaming` 参数，从脚本返回预设 `Reasoning`，`streamed: false`。

**`Box<dyn ReactLLM>` blanket impl**：透传 `streaming` 参数到内部实现。

**`RetryableLLM`**：首次调用透传 `streaming`；**重试调用时传 `None`**（防止流式 chunk 双重发射，见上方"RetryableLLM 关键考量"）。

**SubAgent 路径（Fork/Normal）**：不传递 `StreamingContext`。Normal/Fork 子 Agent 共享父 Agent 的 `event_handler`（`subagent/tool.rs:345-346`），若传入 `StreamingContext` 会导致子 Agent 流式事件泄露到父 TUI（CLAUDE.md 已记载 `issue_2026-05-13-sync-subagent-events-leak-to-parent`）。Background Agent 不受影响——已明确不共享 `event_handler`（`tool.rs:501-503`）。

**`ChatOpenAI.generate_reasoning()` / `ChatAnthropic.generate_reasoning()`**：忽略 `streaming`，保持现有非流式实现（TUI 路径已不走此处）。

### SSE 解析基础设施

两个适配器共享同一个 SSE 行解析逻辑。提取为 `llm/sse.rs`：

```rust
/// 有状态 SSE 解析器：从 reqwest bytes_stream() 的 chunk 中提取 (event_type, data) 对。
///
/// 处理边界：
/// - 行尾可为 `\r\n`（CRLF）或 `\n`（LF），`\r` 作为行尾修饰符被 trim
/// - 事件分隔符为连续空行（`\n\n`），`\r\n\r\n` 等价
/// - `data: [DONE]` 作为流终止信号
/// - 空 `data:` 行（仅 `data:` 无内容）跳过
/// - 不完整行跨 chunk 拼接：内部 `pending_line: String` 缓冲区累积
pub struct SseParser {
    pending_line: String,
    done: bool,
}

impl SseParser {
    pub fn new() -> Self { ... }

    /// Push 新到达的字节块，返回此次推入后解析出的所有完整事件。
    /// 返回空 Vec 表示当前 chunk 内无完整事件（仍在累积中）。
    /// `done` 标志设为 true 后（收到 `[DONE]`），后续 push 均返回空。
    pub fn push(&mut self, bytes: &[u8]) -> Vec<(Option<String>, String)> { ... }

    /// 流是否已终止（收到 `[DONE]` 或流关闭）
    pub fn is_done(&self) -> bool { self.done }
}
```

OpenAI 使用 `data:` 行（无 event type → `None`），Anthropic 使用 `event:` + `data:` 行对（`Some(event_type)`）。`SseParser` 统一处理两种格式，调用方按协议自行分发。

**reqwest 集成示例**：
```rust
let mut stream = response.bytes_stream();
let mut parser = SseParser::new();
while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    for (event_type, data) in parser.push(&chunk) {
        // 按协议分发事件
    }
    if parser.is_done() { break; }
}
```

## 实现要点

### 关键技术决策

1. **message_id 预生成**：Executor 在调用 `generate_reasoning` 前通过 `MessageId::new()` 生成，确保所有增量 chunk 关联到同一 AI 消息。当前 `MessageId` 是 UUID v7（时间有序），预生成不影响语义。

2. **TextChunk 去重**：通过 `Reasoning::streamed` 标志控制。流式适配器设为 `true`，`final_answer.rs` 据此跳过一次性发射。非流式实现保持 `false`，行为不变。

3. **工具调用累积**：流式过程中工具调用参数是分片到达的（`arguments` 是 JSON 字符串片段）。适配器内部累积完整的 arguments 字符串，流结束后一次性解析为 `ToolCall`。不需要增量工具调用事件——工具调用在 UI 中始终作为完整块展示。

4. **流式错误处理**：SSE 流中途断开时，已发射的增量事件（部分文本/推理）保留在 TUI 的 `current_ai_text`/`current_ai_reasoning` 流式缓冲区中，但**不持久化到消息历史**（未 emit StateSnapshot）。`generate_reasoning()` 返回 `Err(AgentError::LlmError(...))`，executor 的错误处理流程（emit LlmCallEnd、run_on_error）照常执行。

   **注意**：TUI 的 `AgentEvent::Error` 处理路径（`agent_ops.rs`）当前未调用 `pipeline.interrupt()`，因此流式缓冲区在错误后会残留显示。这是**有意设计**——用户看到部分回复可作为上下文参考。当用户重新提交消息或 agent 最终 emit Done 时，pipeline 会在 `handle_event(Done)` → `done()` 中调用 `finalize_current_ai()` 清理缓冲区。需在 `handle_agent_event(Error(...))` 中添加 `pipeline.interrupt()` 或明确保留流式内容并附加错误标记。

5. **token usage 提取**：
   - OpenAI：最后一条 chunk 的 `usage` 字段（需 `stream_options.include_usage: true`）
   - Anthropic：`message_start.usage.input_tokens` + `message_delta.usage.output_tokens`

6. **请求构建复用**：`ChatOpenAI` 和 `ChatAnthropic` 的 `invoke()` 方法中现有的请求构建逻辑（消息序列化、system 注入、工具 JSON 构建）可直接复用于 `invoke_streaming()`——仅需将请求体中的 `stream` 设为 `true`，响应处理路径改为 SSE 解析。

### 难点

- **SSE 行解析边界**：reqwest 返回的 `bytes::Bytes` 流不保证按行分割，行尾可能是 `\r\n`（CRLF）或 `\n`（LF）。需要 `SseParser` 内部维护 `pending_line` 缓冲区，先按 `\n` split，trim `\r`，空行作为事件边界。详见上方 `llm/sse.rs` 设计。
- **OpenAI 工具调用参数累积**：`delta.tool_calls[i].function.arguments` 是 JSON 字符串片段，多 index 可能交错到达。需 `BTreeMap<usize, ToolCallAccumulator>` 按 index 管理。见上方"工具调用累积"节。
- **Anthropic thinking 签名**：Extended Thinking 模式下，`signature` 在 `content_block_start` 事件的 `content_block.signature` 字段中提供（**非** `content_block_stop`）。见上方"Extended Thinking"节。
- **推理回传一致性**：流式构建 `source_message` 时必须复用 `block_to_openai_part()` 推理处理逻辑，不可引入新的序列化路径。需确保 `supports_thinking_content` 标志在流式路径中生效。见上方"推理回传防护"节。

## 约束一致性

本方案与 `spec/global/constraints.md` 和 `spec/global/architecture.md` 的架构约束一致：

- **API 风格**：保持 OpenAI `POST /v1/chat/completions` SSE streaming + Anthropic `POST /v1/messages` SSE streaming（约束已声明）
- **异步优先**：SSE 解析基于 `reqwest` 的 async byte stream + `StreamExt`，完全 async
- **事件驱动 TUI 通信**：通过 `AgentEventHandler::on_event()` 发射事件，不引入新的通信机制
- **消息不可变历史**：流式事件只影响 UI 渲染，不修改已持久化的消息历史
- **Middleware Chain**：不受影响，流式是 LLM 适配层内部实现细节。流式 TextChunk/AiReasoning 不经过 `before_tool`/`after_tool` 中间件钩子。流式文本在工具调用前已发射完毕，HITL 拦截 ToolStart 时无冲突
- **Workspace 依赖**：`BaseModel` trait 新增 `invoke_streaming()` 默认方法，不破坏 `peri-agent` → `peri-tui` 的依赖方向。仅 `ChatOpenAI`/`ChatAnthropic`（`peri-agent` 内部）override 该方法
- **错误处理**：LLM 层返回 `anyhow::Result`，流式错误走相同路径

**需注意的架构约定**：
- `StreamingContext` **不传入 SubAgent（Fork/Normal）路径**——与 CLAUDE.md 中 `issue_2026-05-13-sync-subagent-events-leak-to-parent` 的防范一致
- `RetryableLLM` 重试时 `streaming` 设为 `None`——防止同一 message_id 的 chunk 双重发射

## 验收标准

- [ ] OpenAI 兼容接口（GPT-4o、DeepSeek、GLM 等）文本内容逐 token 流式显示在 TUI
  - 验证：发送"请写一首关于春天的诗歌"，确认每行逐字出现而非一次性显示
- [ ] OpenAI 推理内容（reasoning_content / reasoning）逐 token 流式显示
  - 验证：使用 DeepSeek-R1 模型，确认 `reasoning_content` 在 TUI 中逐 token 渲染
- [ ] Anthropic API 文本内容逐 token 流式显示
  - 验证：使用 Claude Sonnet 模型，发送任意问题确认流式逐 token 显示
- [ ] Anthropic Extended Thinking 的 thinking 内容逐 token 流式显示
  - 验证：使用 Extended Thinking 模型（需 `thinking.budget_tokens >= 1024`），确认 thinking 内容流式显示
- [ ] 流式模式下工具调用正常工作（参数累积、完整 ToolCall 返回）
  - 验证：发送"读取 README.md 并总结"，确认 Read 工具调用参数完整，ToolEnd 后能继续流式输出总结
- [ ] 流式模式下 token usage 正确提取和追踪
  - 验证：确认 TUI 状态栏中 token 计数在流式请求后正确更新（无需等完整响应）
- [ ] 流式模式下 Prompt Cache 正常工作（Anthropic）
  - 验证：发送两轮相同的 prompt，检查第二轮 message_start SSE event 中 `usage` 字段是否包含 `cache_read_input_tokens > 0`（或通过 Anthropic API 后台的 cache 命中率指标确认）
- [ ] 非流式实现（MockLLM、BaseModelReactLLM）行为不变，测试通过
- [ ] SSE 流中途断开时正确返回错误，不影响后续重试
  - 验证：模拟网络中断（或使用超时），确认 TUI 显示部分流式文本 + 错误标记，下次请求正常
- [ ] SubAgent（Fork/Normal）内流式事件不泄露到父 TUI
  - 验证：启动一个调用子 Agent 的对话（如"用 subagent 分析代码结构"），确认子 Agent 输出不会混入父 TUI 的流式文本
- [ ] 流式过程中 Ctrl+C 中断后 SSE 流正确关闭，不残留连接
  - 验证：流式输出过程中触发取消中断，确认后台无未关闭的 HTTP 连接
- [ ] `cargo test` 全量通过
- [ ] `cargo clippy` 无新 warning
