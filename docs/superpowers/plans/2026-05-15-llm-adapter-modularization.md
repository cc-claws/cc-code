# LLM 适配器模块化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- ]`) syntax for tracking.

**Goal:** 将 `anthropic.rs`（1287 行）和 `openai.rs`（1065 行）拆分为子模块目录，每个文件职责单一，提取 invoke/stream 共享的请求体构建逻辑。

**Architecture:** 将两个适配器从单文件转为子模块目录结构。`anthropic/` 拆为 `mod.rs` + `cache.rs` + `invoke.rs` + `stream.rs`，`openai/` 拆为 `mod.rs` + `invoke.rs` + `stream.rs`。利用 Rust 多 `impl` block 特性将 `BaseModel::invoke()` 和 `BaseModel::invoke_streaming()` 分放不同文件。提取两个函数共用的请求体构建逻辑为 `build_request_body()` 自由函数。

**Tech Stack:** Rust, tokio, async-trait, serde_json, reqwest

**关于 spec 中的 re-export 文件：** spec 建议保留 `llm/anthropic.rs` 作为 re-export，但 Rust 不允许同时存在 `anthropic.rs` 和 `anthropic/mod.rs`（冲突）。将 `anthropic.rs` 重命名为 `anthropic/mod.rs` 后，模块路径 `crate::llm::anthropic::ChatAnthropic` 保持不变，`mod.rs` 中的 `pub use anthropic::ChatAnthropic` 也不受影响，因此无需额外的 re-export 文件。

**前置注意事项：** 仓库中已存在空的 `peri-agent/src/llm/anthropic/` 和 `peri-agent/src/llm/openai/` 目录（可能是之前准备阶段创建的）。Rust 编译器发现同名目录会优先选择 `foo/mod.rs` 而非 `foo.rs`，因此在 `git mv` 之前必须删除这些空目录，否则构建立即失败。Task 1 和 Task 5 的 Step 0 处理此问题。

---

## 当前代码分布

### anthropic.rs（1287 行）

| 行范围 | 内容 | 目标文件 |
|--------|------|----------|
| 1-12 | imports | 分散到各子模块 |
| 14-16 | `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` 常量 | `cache.rs` |
| 18-22 | `SystemPromptBlock` struct | `cache.rs` |
| 24-37 | `ChatAnthropic` struct | `mod.rs` |
| 39-86 | 构造器方法 | `mod.rs` |
| 87-148 | `block_to_anthropic()` + `content_to_anthropic()` | `invoke.rs` |
| 160-194 | `split_system_blocks()` | `cache.rs` |
| 196-323 | `messages_to_anthropic()` | `invoke.rs` |
| 325-430 | `apply_cache_to_messages()` | `cache.rs` |
| 432-469 | `ensure_thinking_blocks()` | `cache.rs` |
| 471-512 | `parse_content_blocks()` | `invoke.rs` |
| 515-833 | `impl BaseModel`（invoke + 简单方法） | `invoke.rs` |
| 835-1212 | `impl BaseModel`（invoke_streaming） | `stream.rs` |
| 1215-1283 | `impl ReactLLM` | `invoke.rs` |
| 1285-1287 | `#[cfg(test)]` | `mod.rs`（更新路径） |

### openai.rs（1065 行）

| 行范围 | 内容 | 目标文件 |
|--------|------|----------|
| 1-12 | imports | 分散到各子模块 |
| 14-28 | `ChatOpenAI` struct | `mod.rs` |
| 30-108 | 构造器 + helper 方法 | `mod.rs` |
| 110-188 | `content_to_openai()` + `block_to_openai_part()` + `extract_reasoning_text()` | `invoke.rs` |
| 190-273 | `messages_to_json()` | `invoke.rs` |
| 275-398 | `parse_assistant_message()` | `invoke.rs` |
| 400-405 | `ToolCallAccumulator` struct | `stream.rs` |
| 407-645 | `impl BaseModel`（invoke + 简单方法） | `invoke.rs` |
| 647-993 | `impl BaseModel`（invoke_streaming） | `stream.rs` |
| 995-1061 | `impl ReactLLM` | `invoke.rs` |
| 1063-1065 | `#[cfg(test)]` | `mod.rs`（更新路径） |

---

## Task 1: anthropic.rs → anthropic/mod.rs（目录转换）

将单文件模块转为目录模块。Rust 视 `foo/mod.rs` 与 `foo.rs` 等价，模块路径 `crate::llm::anthropic` 不变。

**Files:**
- Delete: `peri-agent/src/llm/anthropic.rs`
- Create: `peri-agent/src/llm/anthropic/mod.rs`（内容与原文件相同，仅更新测试路径）

- [ ] **Step 0: 删除已存在的空目录（防止 Rust 编译器冲突）**

```bash
rmdir peri-agent/src/llm/anthropic/
```

如果目录非空，说明之前有残留文件，需先清理。此步骤必须在 `mkdir` 之前执行，否则目录已存在导致 Rust 优先选择 `mod.rs` 而找不到文件。

- [ ] **Step 1: 创建目录并移动文件**

```bash
mkdir -p peri-agent/src/llm/anthropic
git mv peri-agent/src/llm/anthropic.rs peri-agent/src/llm/anthropic/mod.rs
```

- [ ] **Step 2: 更新测试文件路径**

在 `peri-agent/src/llm/anthropic/mod.rs` 末尾（原第 1285-1287 行），将测试路径改为上级目录：

```rust
#[cfg(test)]
#[path = "../anthropic_test.rs"]
mod tests;
```

- [ ] **Step 3: 验证编译和测试**

```bash
cargo build -p peri-agent
cargo test -p peri-agent --lib
```

Expected: 编译通过，所有测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/anthropic/
git commit -m "refactor(llm): convert anthropic.rs to anthropic/mod.rs directory module"
```

---

## Task 2: 提取 anthropic/cache.rs

将缓存相关逻辑从 `mod.rs` 提取到独立子模块。

**Files:**
- Create: `peri-agent/src/llm/anthropic/cache.rs`
- Modify: `peri-agent/src/llm/anthropic/mod.rs`（删除已提取代码，添加 `mod cache`）

- [ ] **Step 1: 创建 `cache.rs`**

创建 `peri-agent/src/llm/anthropic/cache.rs`，包含从 `mod.rs` 提取的缓存逻辑。所有项标记为 `pub(super)` 以供同模块内的 `invoke.rs` 和 `stream.rs` 使用：

```rust
use serde_json::{json, Value};

/// system prompt 边界标记：之前的内容可被 Anthropic prompt cache 命中，
/// 之后的内容变化不会破坏前缀缓存。
pub(super) const SYSTEM_PROMPT_DYNAMIC_BOUNDARY: &str = "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__";

/// system prompt 的独立缓存块
pub(super) struct SystemPromptBlock {
    pub(super) text: String,
    pub(super) cache_control: bool,
}
```

然后将以下函数从 `mod.rs` 原封不动搬过来，改为 `pub(super)`：
- `split_system_blocks(text: &str) -> Vec<SystemPromptBlock>`（原第 164-194 行）
- `apply_cache_to_messages(messages: &mut [Value])`（原第 338-430 行）
- `ensure_thinking_blocks(messages: &mut [Value])`（原第 437-469 行）

注意：这些函数体内的 `Self::` 调用需要删除（它们现在是自由函数而非方法）。`apply_cache_to_messages` 内部无 `Self::` 调用，无需修改。`ensure_thinking_blocks` 内部也无 `Self::` 调用。

imports 需要从原文件复制，只保留本文件用到的：
```rust
use serde_json::{json, Value};
```

- [ ] **Step 2: 更新 `mod.rs`**

在 `mod.rs` 顶部（imports 之后、常量定义之前）添加：
```rust
mod cache;
```

删除以下代码（已移到 `cache.rs`）：
- 第 14-16 行：`SYSTEM_PROMPT_DYNAMIC_BOUNDARY` 常量
- 第 18-22 行：`SystemPromptBlock` struct
- 第 160-194 行：`split_system_blocks()` 方法
- 第 325-430 行：`apply_cache_to_messages()` 方法
- 第 432-469 行：`ensure_thinking_blocks()` 方法

在 `mod.rs` 中 `invoke()` 和 `invoke_streaming()` 内部，将以下调用改为通过 `cache::` 前缀：
- `Self::split_system_blocks(...)` → `cache::split_system_blocks(...)`
- `Self::apply_cache_to_messages(...)` → `cache::apply_cache_to_messages(...)`
- `Self::ensure_thinking_blocks(...)` → `cache::ensure_thinking_blocks(...)`
- `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` → `cache::SYSTEM_PROMPT_DYNAMIC_BOUNDARY`
- `SystemPromptBlock { ... }` → `cache::SystemPromptBlock { ... }`

在 `mod.rs` 的 imports 中添加：
```rust
use super::BaseModel;
```
（原来是通过 `use super::BaseModel;` 引入的，保持不变）

- [ ] **Step 3: 验证编译**

```bash
cargo build -p peri-agent
```

Expected: 编译通过。如有 visibility 错误，检查 cache.rs 中的 `pub(super)` 标记。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/anthropic/cache.rs peri-agent/src/llm/anthropic/mod.rs
git commit -m "refactor(llm): extract anthropic cache logic to cache.rs submodule"
```

---

## Task 3: 提取 anthropic/invoke.rs（消息转换 + invoke + parse + ReactLLM）

将消息转换、响应解析、invoke()、ReactLLM 提取到独立子模块。同时提取 `build_request_body()` 共享函数供 stream.rs 复用。

**Files:**
- Create: `peri-agent/src/llm/anthropic/invoke.rs`
- Modify: `peri-agent/src/llm/anthropic/mod.rs`

- [ ] **Step 1: 创建 `invoke.rs`**

创建 `peri-agent/src/llm/anthropic/invoke.rs`。此文件包含：

**imports：**
```rust
use async_trait::async_trait;
use serde_json::{json, Value};

use super::ChatAnthropic;
use super::cache::{SystemPromptBlock, SYSTEM_PROMPT_DYNAMIC_BOUNDARY, split_system_blocks, apply_cache_to_messages, ensure_thinking_blocks};
use super::BaseModel;
use crate::agent::events::AgentEvent;
use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::{AgentError, AgentResult};
use crate::llm::sse::SseParser;
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use crate::tools::BaseTool;
```

**从 mod.rs 搬入的函数（改为 `pub(super)` 或 `pub(crate)`）：**

1. `block_to_anthropic(block: &ContentBlock) -> Option<Value>`（原第 89-148 行）— 保持 `fn`（私有，仅 invoke.rs 内部使用）

2. `content_to_anthropic(content: &MessageContent) -> Value`（原第 148 行附近）— 保持 `fn`（私有）

3. `messages_to_anthropic(messages: &[BaseMessage]) -> (Vec<Value>, Vec<SystemPromptBlock>)`（原第 204-323 行）— 改为 `pub(super) fn`（stream.rs 通过 `build_request_body` 间接使用）

4. `parse_content_blocks(raw_blocks: &[Value]) -> (Vec<ContentBlock>, Vec<ToolCallRequest>)`（原第 473-512 行）— 改为 `pub(super) fn`（stream.rs 直接使用）

**Self:: 前缀删除清单**（impl 方法 → 自由函数）：

| 函数 | 需删除的 `Self::` 调用 | 替换为 |
|------|----------------------|--------|
| `content_to_anthropic` | `Self::block_to_anthropic(...)` (1 处) | `block_to_anthropic(...)` |
| `block_to_anthropic` | `Self::block_to_anthropic(...)` (2 处，递归调用) | `block_to_anthropic(...)` |
| `messages_to_anthropic` | `Self::content_to_anthropic(...)` (4 处) | `content_to_anthropic(...)` |
| | `Self::split_system_blocks(...)` (1 处) | `cache::split_system_blocks(...)` |
| | `Self::block_to_anthropic(...)` (3 处) | `block_to_anthropic(...)` |
| `invoke()` 方法内 | `Self::parse_content_blocks(...)` (1 处) | `parse_content_blocks(...)` |
| `invoke()` 方法内 | `Self::messages_to_anthropic(...)` | 已由 `build_request_body` 内部处理 |
| `ReactLLM::generate_reasoning` | `Self::messages_to_anthropic(...)` | `messages_to_anthropic(...)` |
| | `Self::parse_content_blocks(...)` | `parse_content_blocks(...)` |

实施时逐项替换，不可遗漏。`parse_content_blocks` 原是 impl 外自由函数，无 `Self::`，无需修改。

**补充提醒**：`invoke()` 内部原来对 `self.base_url`、`self.api_key`、`self.client` 等的引用保持为 `self.` 形式（这些是字段访问，不是 `Self::` 调用，不需要修改）。

**提取的新函数 `build_request_body()`：**

从 `invoke()` 和 `invoke_streaming()` 中提取共享的请求体构建逻辑（约 100 行，当前两处近乎完全重复）。函数签名：

```rust
/// 构建 Anthropic API 请求体（invoke 和 invoke_streaming 共用）
///
/// 返回 (body, messages, system_blocks)，其中 messages 和 system_blocks
/// 是中间产物，仅 invoke() 的日志需要 messages.len()。
pub(super) fn build_request_body(
    adapter: &ChatAnthropic,
    request: &LlmRequest,
    streaming: bool,
) -> (Value, Vec<Value>, Vec<SystemPromptBlock>) {
    let msg_count = request.messages.len();

    let chat_url = match &adapter.base_url {
        Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
        None => "https://api.anthropic.com/v1/messages".to_string(),
    };

    let tools_json: Vec<Value> = request
        .tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })
        })
        .collect();

    let (mut messages, system_from_msgs) = messages_to_anthropic(&request.messages);

    let mut system_blocks = system_from_msgs;
    if let Some(ref base) = request.system {
        if !base.is_empty() {
            system_blocks.push(SystemPromptBlock {
                text: base.clone(),
                cache_control: false,
            });
        }
    }
    let max_tokens = request.max_tokens.unwrap_or(4096);

    if adapter.enable_cache {
        apply_cache_to_messages(&mut messages);
    }

    if adapter.extended_thinking {
        ensure_thinking_blocks(&mut messages);
    }

    let mut body = json!({
        "model": adapter.model,
        "max_tokens": max_tokens,
        "messages": messages
    });

    if streaming {
        body["stream"] = json!(true);
    }

    if adapter.enable_cache {
        if !system_blocks.is_empty() {
            let last_idx = system_blocks.len() - 1;
            let blocks_json: Vec<Value> = system_blocks
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let mut block = json!({"type": "text", "text": &b.text});
                    if b.cache_control || i == last_idx {
                        block["cache_control"] = json!({"type": "ephemeral"});
                    }
                    block
                })
                .collect();
            body["system"] = Value::Array(blocks_json);
        }
    } else if !system_blocks.is_empty() {
        let text = system_blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
            .replace(SYSTEM_PROMPT_DYNAMIC_BOUNDARY, "");
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            body["system"] = json!(trimmed);
        }
    }

    if !tools_json.is_empty() {
        body["tools"] = Value::Array(tools_json);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    if adapter.extended_thinking {
        body["thinking"] = json!({
            "type": "enabled",
            "budget_tokens": adapter.thinking_budget
        });
        body["output_config"] = json!({ "effort": adapter.thinking_effort });
    }

    (body, messages, system_blocks)
}
```

注意：`build_request_body` 内部引用的 `messages_to_anthropic`、`ensure_thinking_blocks`、`apply_cache_to_messages` 是同文件内的函数（`messages_to_anthropic`）或来自 `super::cache` 的函数，无需 `Self::` 前缀。

**impl BaseModel for ChatAnthropic：**

从 mod.rs 搬入以下方法（原第 515-833 行），但 `invoke()` 改为调用 `build_request_body()`：

```rust
#[async_trait]
impl BaseModel for ChatAnthropic {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let msg_count = request.messages.len();
        let start = std::time::Instant::now();

        let (body, _messages, _system_blocks) = build_request_body(self, &request, false);

        let chat_url = match &self.base_url {
            Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
            None => "https://api.anthropic.com/v1/messages".to_string(),
        };

        let mut req = self
            .client
            .post(chat_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if self.enable_cache {
            req = req.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        if let Some(ref sid) = request.session_id {
            req = req.header("x-session-id", sid.as_str());
        }

        tracing::debug!(
            provider = "anthropic",
            model = %self.model,
            messages_count = body["messages"].as_array().map(|a| a.len()).unwrap_or(0),
            "LLM 请求发送"
        );

        // ... HTTP 发送 + 响应解析（原第 650-821 行代码原封不动搬入）
        // 注意：将 Self::parse_content_blocks 改为 parse_content_blocks
        let resp = req.json(&body).send().await.map_err(|e| {
            tracing::error!(
                provider = "anthropic", model = %self.model,
                elapsed_ms = start.elapsed().as_millis() as u64, error = %e,
                "LLM 网络请求失败"
            );
            AgentError::LlmError(e.to_string())
        })?;

        // ... 后续响应处理代码不变（原第 661-821 行）
    }

    fn provider_name(&self) -> &str { "anthropic" }
    fn model_id(&self) -> &str { &self.model }
    fn context_window(&self) -> u32 { 200_000 }
}
```

**impl ReactLLM for ChatAnthropic：**

从 mod.rs 搬入（原第 1215-1283 行），将 `Self::` 调用改为直接函数调用：

```rust
#[async_trait]
impl ReactLLM for ChatAnthropic {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        // ... 原第 1217-1278 行代码原封不动
    }

    fn model_name(&self) -> String {
        self.model.clone()
    }
}
```

- [ ] **Step 2: 更新 `mod.rs`**

在 `mod.rs` 顶部添加 `mod invoke;`。

删除已移到 `invoke.rs` 的代码：
- 第 87-148 行：`block_to_anthropic()` 和 `content_to_anthropic()`
- 第 196-323 行：`messages_to_anthropic()`
- 第 471-512 行：`parse_content_blocks()`
- 第 515-833 行：`impl BaseModel` 整个 block
- 第 1215-1283 行：`impl ReactLLM` 整个 block

删除后 `mod.rs` 只保留：
- imports（精简为只含构造器需要的）
- `mod cache;`
- `mod invoke;`
- `ChatAnthropic` struct 定义
- 构造器 impl block（new, with_base_url, with_extended_thinking, without_cache, from_env）
- `#[cfg(test)]` block

精简后的 `mod.rs` imports：
```rust
use super::BaseModel;
```
（其他 imports 如 `async_trait`、`futures`、`serde_json`、`crate::agent::*` 等只有在 invoke/stream 中使用，构造器不需要）

- [ ] **Step 3: 验证编译**

```bash
cargo build -p peri-agent
```

Expected: 编译通过。如报 `duplicate impl` 错误，检查 mod.rs 中是否残留 impl block。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/anthropic/invoke.rs peri-agent/src/llm/anthropic/mod.rs
git commit -m "refactor(llm): extract anthropic invoke, message conversion, and ReactLLM to invoke.rs"
```

---

## Task 4: 提取 anthropic/stream.rs

将 `invoke_streaming()` 提取到独立子模块，复用 `invoke.rs` 中的 `build_request_body()` 和 `parse_content_blocks()`。

**Files:**
- Create: `peri-agent/src/llm/anthropic/stream.rs`
- Modify: `peri-agent/src/llm/anthropic/mod.rs`

- [ ] **Step 1: 创建 `stream.rs`**

创建 `peri-agent/src/llm/anthropic/stream.rs`：

```rust
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};

use super::ChatAnthropic;
use super::cache::{SystemPromptBlock, apply_cache_to_messages, ensure_thinking_blocks};
use super::invoke::{build_request_body, parse_content_blocks};
use super::BaseModel;
use crate::agent::events::AgentEvent;
use crate::error::{AgentError, AgentResult};
use crate::llm::sse::SseParser;
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, MessageContent};
```

从 mod.rs 搬入 `BaseModel` 的 `invoke_streaming()` 方法（原第 835-1212 行），但重构为使用 `build_request_body()`：

```rust
#[async_trait]
impl BaseModel for ChatAnthropic {
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        let msg_count = request.messages.len();
        let start = std::time::Instant::now();

        let (body, _messages, _system_blocks) = build_request_body(self, &request, true);

        let chat_url = match &self.base_url {
            Some(base) => format!("{}/v1/messages", base.trim_end_matches('/')),
            None => "https://api.anthropic.com/v1/messages".to_string(),
        };

        let mut req = self
            .client
            .post(chat_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if self.enable_cache {
            req = req.header("anthropic-beta", "prompt-caching-2024-07-31");
        }

        if let Some(ref sid) = request.session_id {
            req = req.header("x-session-id", sid.as_str());
        }

        let resp = req.json(&body).send().await.map_err(|e| {
            tracing::error!(
                provider = "anthropic", model = %self.model,
                elapsed_ms = start.elapsed().as_millis() as u64, error = %e,
                "LLM 流式网络请求失败"
            );
            AgentError::LlmError(e.to_string())
        })?;

        let status = resp.status();
        if !status.is_success() {
            let resp_text = resp.text().await.unwrap_or_default();
            let error_msg = serde_json::from_str::<Value>(&resp_text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "未知错误".to_string());
            tracing::error!(
                provider = "anthropic", model = %self.model, status = %status,
                error_message = %error_msg,
                elapsed_ms = start.elapsed().as_millis() as u64, msg_count,
                "LLM 流式 API 错误"
            );
            return Err(AgentError::LlmHttpError {
                status: status.as_u16(),
                message: format!("API 错误 {status}: {error_msg}"),
            });
        }

        // SSE 流式处理（原第 976-1151 行代码原封不动）
        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::new();

        // ... accumulators 和 SSE 循环代码与原文件完全相同 ...

        // 最终响应构建（原第 1153-1211 行）
        let stop_reason = StopReason::from_anthropic(&stop_reason_str);
        let (blocks, tool_calls) = parse_content_blocks(&accumulated_blocks);
        // ... 消息构建和 usage 构建代码与原文件相同 ...
    }
}
```

**关键变更点：**
1. 删除了原 `invoke_streaming()` 中重复的请求体构建代码（约 100 行），替换为 `build_request_body(self, &request, true)` 一行调用
2. 将 `Self::parse_content_blocks(...)` 改为 `parse_content_blocks(...)`
3. `Self::messages_to_anthropic(...)` 已被 `build_request_body()` 内部调用，无需单独调用
4. `Self::ensure_thinking_blocks(...)` 和 `Self::apply_cache_to_messages(...)` 同理

- [ ] **Step 2: 更新 `mod.rs`**

在 `mod.rs` 顶部添加 `mod stream;`。

删除已移到 `stream.rs` 的代码：
- `mod.rs` 中 `impl BaseModel` block 里的 `invoke_streaming()` 方法（如果 Task 3 后 mod.rs 中还残留了 invoke_streaming）

精简 `mod.rs` 的 imports — 移除不再直接使用的导入（`futures::StreamExt`、`serde_json` 等，除非构造器仍需要）。构造器不使用 serde_json，所以可以移除。最终 mod.rs imports：

```rust
use super::BaseModel;
```

- [ ] **Step 3: 验证编译和测试**

```bash
cargo build -p peri-agent
cargo test -p peri-agent --lib
```

Expected: 编译通过，所有 anthropic 测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/anthropic/stream.rs peri-agent/src/llm/anthropic/mod.rs
git commit -m "refactor(llm): extract anthropic invoke_streaming to stream.rs, reuse build_request_body"
```

---

## Task 5: openai.rs → openai/mod.rs（目录转换）

与 Task 1 相同的流程，处理 openai 适配器。

**Files:**
- Delete: `peri-agent/src/llm/openai.rs`
- Create: `peri-agent/src/llm/openai/mod.rs`

- [ ] **Step 0: 删除已存在的空目录（防止 Rust 编译器冲突）**

```bash
rmdir peri-agent/src/llm/openai/
```

与 Task 1 同理，必须先删除空目录。

- [ ] **Step 1: 创建目录并移动文件**

```bash
mkdir -p peri-agent/src/llm/openai
git mv peri-agent/src/llm/openai.rs peri-agent/src/llm/openai/mod.rs
```

- [ ] **Step 2: 更新测试文件路径**

在 `peri-agent/src/llm/openai/mod.rs` 末尾：

```rust
#[cfg(test)]
#[path = "../openai_test.rs"]
mod tests;
```

- [ ] **Step 3: 验证编译和测试**

```bash
cargo build -p peri-agent
cargo test -p peri-agent --lib
```

Expected: 编译通过，所有测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/openai/
git commit -m "refactor(llm): convert openai.rs to openai/mod.rs directory module"
```

---

## Task 6: 提取 openai/invoke.rs

将消息转换、响应解析、invoke()、消息不变量校验、ReactLLM 提取到独立子模块。同时提取 `build_request_body()` 和 `validate_message_invariants()` 共享函数。

**Files:**
- Create: `peri-agent/src/llm/openai/invoke.rs`
- Modify: `peri-agent/src/llm/openai/mod.rs`

- [ ] **Step 1: 创建 `invoke.rs`**

创建 `peri-agent/src/llm/openai/invoke.rs`：

**imports：**
```rust
use async_trait::async_trait;
use serde_json::{json, Value};

use super::ChatOpenAI;
use super::BaseModel;
use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::error::{AgentError, AgentResult};
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
use crate::tools::BaseTool;
```

**从 mod.rs 搬入的函数：**

1. `content_to_openai(content: &MessageContent, supports_thinking_content: bool) -> Value`（原第 117-136 行）— 改为 `pub(super) fn`。**无跨 crate 引用**（已 grep 确认），但测试文件使用 `ChatOpenAI::content_to_openai(...)` 关联函数语法调用，需在 `mod.rs` 添加 thin wrapper（见 Step 2）

2. `block_to_openai_part(block: &ContentBlock, supports_thinking_content: bool) -> Option<Value>`（原第 138-171 行）— 保持 `fn`（私有）

3. `extract_reasoning_text(content: &MessageContent) -> Option<String>`（原第 176-188 行）— 保持 `fn`（私有）

4. `messages_to_json(adapter: &ChatOpenAI, messages: &[BaseMessage]) -> Vec<Value>`（原第 190-273 行）— 改为 `pub(super) fn`（去掉 `&self`，改为接受 `&ChatOpenAI` 参数获取 `supports_thinking_content`）。原 `&self` 方法被移除，内部 `Self::content_to_openai` → `content_to_openai`，`Self::extract_reasoning_text` → `extract_reasoning_text`。测试文件使用 `llm.messages_to_json(&msgs)` 调用，需在 `mod.rs` 添加 thin wrapper（见 Step 2）

5. `parse_assistant_message(assistant_msg: &Value, stop_reason: &StopReason) -> BaseMessage`（原第 287-397 行）— 改为 `pub(super) fn`

**Self:: 前缀删除清单**（impl 方法 → 自由函数）：

| 函数 | 需删除的 `Self::` 调用 | 替换为 |
|------|----------------------|--------|
| `messages_to_json` | `Self::content_to_openai(...)` (3 处) | `content_to_openai(...)` |
| | `Self::extract_reasoning_text(...)` (1 处) | `extract_reasoning_text(...)` |
| `invoke()` 方法内 | `Self::messages_to_json(...)` | 已由 `build_request_body` 内部处理 |
| `invoke()` 方法内 | `Self::parse_assistant_message(...)` (1 处) | `parse_assistant_message(...)` |
| `ReactLLM::generate_reasoning` | `Self::messages_to_json(...)` | `messages_to_json(self, ...)` |

`parse_assistant_message` 原是 impl 外自由函数，无 `Self::`，无需修改。

**提取的新函数：**

```rust
/// 校验消息序列不变量：每段连续 tool 消息块之前必须有 assistant with tool_calls
pub(super) fn validate_message_invariants(messages: &[Value]) {
    let mut i = 0;
    while i < messages.len() {
        if messages[i]["role"] == "tool" {
            let block_start = i;
            let prev_non_tool = if block_start > 0 {
                let mut j = block_start;
                while j > 0 && messages[j - 1]["role"] == "tool" {
                    j -= 1;
                }
                if j > 0 {
                    Some(&messages[j - 1])
                } else {
                    None
                }
            } else {
                None
            };
            let valid = prev_non_tool
                .is_some_and(|p| p["role"] == "assistant" && p["tool_calls"].is_array());
            if !valid {
                tracing::error!(
                    block_start,
                    total = messages.len(),
                    prev_non_tool_role = ?prev_non_tool.map(|m| m["role"].as_str()),
                    "消息序列不变量违反：连续 tool 块前缺少 assistant with tool_calls"
                );
            }
            while i < messages.len() && messages[i]["role"] == "tool" {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

/// 构建 OpenAI API 请求体（invoke 和 invoke_streaming 共用）
///
/// 返回 (body, messages)，其中 messages 是已转换的 JSON 消息列表。
pub(super) fn build_request_body(
    adapter: &ChatOpenAI,
    request: &LlmRequest,
    streaming: bool,
) -> (Value, Vec<Value>) {
    let chat_url = format!("{}/chat/completions", adapter.base_url.trim_end_matches('/'));

    let tools_json: Vec<Value> = request
        .tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect();

    let mut messages = messages_to_json(adapter, &request.messages);

    validate_message_invariants(&messages);

    if let Some(base_system) = &request.system {
        if let Some(first) = messages.first_mut() {
            if first["role"] == "system" {
                let existing = first["content"].as_str().unwrap_or("");
                first["content"] = json!(format!("{}\n\n{}", existing, base_system));
            } else {
                messages.insert(0, json!({ "role": "system", "content": base_system }));
            }
        } else {
            messages.insert(0, json!({ "role": "system", "content": base_system }));
        }
    }

    let mut body = json!({
        "model": adapter.model,
        "messages": messages,
        "stream": streaming
    });

    if streaming {
        body["stream_options"] = json!({"include_usage": true});
    }

    if !tools_json.is_empty() {
        body["tools"] = Value::Array(tools_json);
        body["tool_choice"] = json!("auto");
    }

    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    if let Some(ref effort) = adapter.reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    } else if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    if adapter.thinking_enabled {
        body["thinking"] = json!({ "type": "enabled" });
    }

    if let Some(ref sid) = request.session_id {
        body["metadata"] = json!({ "session_id": sid });
    }

    (body, messages)
}
```

注意：`build_request_body` 内部直接调用同文件的 `messages_to_json(adapter, &request.messages)`。`chat_url` 已在上文构建，此处省略。

**impl BaseModel：**

```rust
#[async_trait]
impl BaseModel for ChatOpenAI {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        let msg_count = request.messages.len();
        let start = std::time::Instant::now();

        let (body, _messages) = build_request_body(self, &request, false);

        let chat_url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&chat_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(
                    provider = "openai", model = %self.model,
                    elapsed_ms = start.elapsed().as_millis() as u64, error = %e,
                    "LLM 网络请求失败"
                );
                AgentError::LlmError(e.to_string())
            })?;

        // ... 响应处理代码与原文件相同（原第 536-633 行）...
        // 将 Self::parse_assistant_message 改为 parse_assistant_message
    }

    fn provider_name(&self) -> &str { "openai" }
    fn model_id(&self) -> &str { &self.model }
    fn context_window(&self) -> u32 { self.context_window_inner() }
}
```

**impl ReactLLM：**

```rust
#[async_trait]
impl ReactLLM for ChatOpenAI {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
        _streaming: Option<StreamingContext>,
    ) -> AgentResult<Reasoning> {
        // ... 原第 997-1056 行代码原封不动搬入
    }

    fn model_name(&self) -> String {
        self.model.clone()
    }
}
```

- [ ] **Step 2: 更新 `mod.rs`**

在 `mod.rs` 顶部添加 `mod invoke;`。

删除已移到 `invoke.rs` 的代码：
- 第 110-188 行：`content_to_openai()`、`block_to_openai_part()`、`extract_reasoning_text()`
- 第 190-273 行：`messages_to_json()`
- 第 275-398 行：`parse_assistant_message()`
- 第 407-645 行：`impl BaseModel` 整个 block
- 第 995-1061 行：`impl ReactLLM` 整个 block

**添加 thin wrapper**（在 `ChatOpenAI` 的构造器 impl block 内添加，兼容测试文件中 `ChatOpenAI::content_to_openai(...)` 和 `llm.messages_to_json(...)` 的调用方式）：

```rust
impl ChatOpenAI {
    pub(crate) fn content_to_openai(
        content: &MessageContent,
        supports_thinking_content: bool,
    ) -> Value {
        invoke::content_to_openai(content, supports_thinking_content)
    }

    pub(crate) fn messages_to_json(&self, messages: &[BaseMessage]) -> Vec<Value> {
        invoke::messages_to_json(self, messages)
    }
}
```

精简 `mod.rs` imports 为构造器所需的最小集合。

- [ ] **Step 3: 验证编译**

```bash
cargo build -p peri-agent
```

Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/openai/invoke.rs peri-agent/src/llm/openai/mod.rs
git commit -m "refactor(llm): extract openai invoke, message conversion, and ReactLLM to invoke.rs"
```

---

## Task 7: 提取 openai/stream.rs

将 `invoke_streaming()` 和 `ToolCallAccumulator` 提取到独立子模块。

**Files:**
- Create: `peri-agent/src/llm/openai/stream.rs`
- Modify: `peri-agent/src/llm/openai/mod.rs`

- [ ] **Step 1: 创建 `stream.rs`**

创建 `peri-agent/src/llm/openai/stream.rs`：

```rust
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::ChatOpenAI;
use super::invoke::{build_request_body, parse_assistant_message};
use super::BaseModel;
use crate::agent::events::AgentEvent;
use crate::error::{AgentError, AgentResult};
use crate::llm::sse::SseParser;
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, MessageContent, ToolCallRequest};

/// 流式工具调用参数累积器
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments_fragments: Vec<String>,
}

#[async_trait]
impl BaseModel for ChatOpenAI {
    async fn invoke_streaming(
        &self,
        request: LlmRequest,
        ctx: StreamingContext,
    ) -> AgentResult<LlmResponse> {
        let msg_count = request.messages.len();
        let start = std::time::Instant::now();

        let (body, _messages) = build_request_body(self, &request, true);

        let chat_url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&chat_url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(
                    provider = "openai", model = %self.model,
                    elapsed_ms = start.elapsed().as_millis() as u64, error = %e,
                    "LLM 流式网络请求失败"
                );
                AgentError::LlmError(e.to_string())
            })?;

        let status = resp.status();
        if !status.is_success() {
            // ... 错误处理与原文件相同（原第 770-787 行）...
        }

        // SSE 流式处理（原第 789-882 行代码原封不动）
        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::new();
        // ... accumulators 和 SSE 循环 ...

        // 最终响应构建（原第 884-991 行）
        // ... 与原文件相同 ...
    }
}
```

**关键变更点：**
1. 删除了原 `invoke_streaming()` 中重复的请求体构建代码（约 80 行），替换为 `build_request_body(self, &request, true)`
2. 删除了重复的 `validate_message_invariants` 调用（已在 `build_request_body` 内部）
3. `ToolCallAccumulator` struct 移到 stream.rs（仅流式处理使用）

- [ ] **Step 2: 更新 `mod.rs`**

在 `mod.rs` 顶部添加 `mod stream;`。

删除 `mod.rs` 中 `impl BaseModel` block 里的 `invoke_streaming()` 方法（如果 Task 6 后还残留）。

- [ ] **Step 3: 验证编译和测试**

```bash
cargo build -p peri-agent
cargo test -p peri-agent --lib
```

Expected: 编译通过，所有 openai 测试通过。

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/llm/openai/stream.rs peri-agent/src/llm/openai/mod.rs
git commit -m "refactor(llm): extract openai invoke_streaming to stream.rs, reuse build_request_body"
```

---

## Task 8: 最终验证

**Files:** 无修改

- [ ] **Step 1: 全量构建**

```bash
cargo build
```

Expected: 所有 crate 编译通过，无 warning。

- [ ] **Step 2: 全量测试**

```bash
cargo test
```

Expected: 所有测试通过。

- [ ] **Step 3: clippy 检查**

```bash
cargo clippy -p peri-agent -- -D warnings
```

Expected: 无 warning。

- [ ] **Step 4: 检查最终文件结构**

```bash
find peri-agent/src/llm/ -type f | sort
```

Expected:
```
peri-agent/src/llm/adapter.rs
peri-agent/src/llm/adapter_test.rs
peri-agent/src/llm/anthropic/cache.rs
peri-agent/src/llm/anthropic/invoke.rs
peri-agent/src/llm/anthropic/mod.rs
peri-agent/src/llm/anthropic/stream.rs
peri-agent/src/llm/anthropic_test.rs
peri-agent/src/llm/mod.rs
peri-agent/src/llm/openai/invoke.rs
peri-agent/src/llm/openai/mod.rs
peri-agent/src/llm/openai/stream.rs
peri-agent/src/llm/openai_test.rs
peri-agent/src/llm/react_adapter.rs
peri-agent/src/llm/react_adapter_test.rs
peri-agent/src/llm/retry.rs
peri-agent/src/llm/retry_test.rs
peri-agent/src/llm/sse.rs
peri-agent/src/llm/sse_test.rs
peri-agent/src/llm/types.rs
peri-agent/src/llm/types_test.rs
```

- [ ] **Step 5: 检查各文件行数**

```bash
wc -l peri-agent/src/llm/anthropic/*.rs peri-agent/src/llm/openai/*.rs
```

Expected:
```
  anthropic/cache.rs      ~130 行
  anthropic/invoke.rs     ~500 行
  anthropic/mod.rs        ~90 行
  anthropic/stream.rs     ~250 行
  openai/invoke.rs        ~400 行
  openai/mod.rs           ~110 行
  openai/stream.rs        ~300 行
```

所有文件应低于 550 行。

---

## Self-Review Checklist

**1. Spec 覆盖率：**
- [x] `anthropic/` 子模块结构（mod.rs + cache.rs + invoke.rs + stream.rs）→ Task 1-4
- [x] `openai/` 子模块结构（mod.rs + invoke.rs + stream.rs）→ Task 5-7
- [x] 保留原模块路径（通过 mod.rs 等价）→ Task 1, 5
- [x] 构造器留在 mod.rs → Task 2, 6
- [x] 缓存策略独立 cache.rs → Task 2
- [x] invoke() 留在 invoke.rs → Task 3, 6
- [x] 流式处理独立 stream.rs → Task 4, 7
- [x] 消息转换与 invoke 在同一文件 → Task 3, 6
- [x] 测试文件路径更新 → Task 1, 5

**2. Placeholder 扫描：**
- [x] 无 TBD/TODO
- [x] 所有函数签名已给出
- [x] 所有 imports 已列出
- [x] 行范围引用精确

**3. 类型一致性：**
- [x] `SystemPromptBlock` 在 cache.rs 定义，invoke.rs/stream.rs 通过 `super::cache::` 引用
- [x] `build_request_body()` 返回类型一致（anthropic: `(Value, Vec<Value>, Vec<SystemPromptBlock>)`, openai: `(Value, Vec<Value>)`)
- [x] `parse_content_blocks` / `parse_assistant_message` 签名不变
- [x] `messages_to_json` 改为接受 `&ChatOpenAI` 参数的自由函数，mod.rs 有 thin wrapper 保持向后兼容

**4. 潜在风险：**
- `content_to_openai` 标记为 `pub(super)` — 已 grep 确认无跨 crate 引用，mod.rs 添加 thin wrapper 兼容测试文件中的 `ChatOpenAI::content_to_openai(...)` 调用
- `messages_to_json` 原为 `pub(crate)` — 已确认无外部引用，降为 `pub(super)`，mod.rs 添加 thin wrapper 兼容测试文件中的 `llm.messages_to_json(...)` 调用
- `build_request_body` 内部调用 `messages_to_anthropic`/`messages_to_json` — 这些函数在同文件内定义，无跨模块问题
