# 提取共享 LLM 适配器代码 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除 OpenAI 和 Anthropic LLM 适配器之间的代码重复，提取共享工具函数，删除冗余的 `impl ReactLLM` 实现。

**Architecture:** 三层去重——（1）删除两个 provider 中完全冗余的 `impl ReactLLM`（生产路径已通过 `BaseModelReactLLM`）；（2）将 `build_reqwest_client()` 提取到 `llm/mod.rs`；（3）在 OpenAI stream/invoke 中提取重复的 `TokenUsage` 构建和 `LlmResponse` 构建为共享函数。

**Tech Stack:** Rust, reqwest, serde_json, async-trait

---

## 当前重复分析

| 重复项 | 位置 | 行数 | 严重程度 |
|--------|------|------|----------|
| `impl ReactLLM for ChatOpenAI` | `llm/openai/invoke.rs:568-664` | ~97 | **关键**：与 `BaseModelReactLLM` 完全冗余，生产未使用 |
| `impl ReactLLM for ChatAnthropic` | `llm/anthropic/invoke.rs:621-709` | ~89 | **关键**：同上 |
| `build_reqwest_client()` | `llm/openai/mod.rs:10-16` + `llm/anthropic/mod.rs:8-14` | ~14×2 | 中 |
| OpenAI `TokenUsage` ��建 | `openai/invoke.rs:517-537` + `openai/stream.rs:232-248,270-285` | ~25×3 | 中 |
| OpenAI stream `LlmResponse` 构建（ToolUse vs text 两条分支） | `openai/stream.rs:218-294` | ~76（两分支几乎相同） | 低-中 |

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `peri-agent/src/llm/mod.rs` | 修改 | 添加 `build_reqwest_client()` pub(crate) 函数 |
| `peri-agent/src/llm/openai/mod.rs` | 修改 | 删除 `build_reqwest_client()`，使用上层共享版本 |
| `peri-agent/src/llm/anthropic/mod.rs` | 修改 | 同上 |
| `peri-agent/src/llm/openai/invoke.rs` | 修改 | 删除 `impl ReactLLM for ChatOpenAI`；提取 `extract_openai_usage` |
| `peri-agent/src/llm/openai/stream.rs` | 修改 | 使用共享 `extract_openai_usage`；合并 ToolUse/text 的 `LlmResponse` 构建 |
| `peri-agent/src/llm/anthropic/invoke.rs` | 修改 | 删除 `impl ReactLLM for ChatAnthropic` |

---

### Task 1: 删除冗余 `impl ReactLLM for ChatOpenAI`

**Files:**
- Modify: `peri-agent/src/llm/openai/invoke.rs:568-664`
- Test: `peri-agent/src/llm/openai_test.rs`（无需改动——测试不测 `generate_reasoning`）

- [ ] **Step 1: 删除 `impl ReactLLM for ChatOpenAI` 块**

删除 `peri-agent/src/llm/openai/invoke.rs` 第 568-664 行的整个 `impl ReactLLM for ChatOpenAI` 块，同时删除不再需要的 imports：

```rust
// 删除这行（如果 `react_adapter.rs` 和 `mock` 都不从此文件使用）
use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
```

只保留 `use crate::agent::react::{Reasoning, ToolCall};`（被 `parse_assistant_message` 中的 `ToolCallRequest` 使用，无需改动）。

实际需保留的 imports 检查：
- `ReactLLM` — 删除后无需保留
- `Reasoning` — 不被 invoke.rs 使用（只在被删除的 `generate_reasoning` 中），删除
- `ToolCall` — 同上，删除
- `BaseTool` — 同上，删除

最终 `invoke.rs` 顶部 imports 应为：

```rust
use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::BaseModel;
use super::ChatOpenAI;
use crate::error::{AgentError, AgentResult};
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
```

- [ ] **Step 2: 验证编译**

```bash
cargo build -p peri-agent
```

Expected: 编译成功（生产路径通过 `BaseModelReactLLM`，不直接使用 `ChatOpenAI` 的 `ReactLLM` impl）

- [ ] **Step 3: 运行测试确认无回归**

```bash
cargo test -p peri-agent --lib -- openai
```

Expected: 所有测试通过

---

### Task 2: 删除冗余 `impl ReactLLM for ChatAnthropic`

**Files:**
- Modify: `peri-agent/src/llm/anthropic/invoke.rs:621-709`

- [ ] **Step 1: 删除 `impl ReactLLM for super::ChatAnthropic` 块**

删除 `peri-agent/src/llm/anthropic/invoke.rs` 第 621-709 行的整个 `impl ReactLLM for super::ChatAnthropic` 块。

同步清理顶部 imports：

```rust
// 删除
use crate::agent::react::{ReactLLM, Reasoning, ToolCall};
use crate::tools::BaseTool;
```

最终 `anthropic/invoke.rs` 顶部 imports：

```rust
use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::BaseModel;
use super::cache::{self, SystemPromptBlock, SYSTEM_PROMPT_DYNAMIC_BOUNDARY};
use crate::error::{AgentError, AgentResult};
use crate::llm::types::{LlmRequest, LlmResponse, StopReason, StreamingContext};
use crate::messages::{BaseMessage, ContentBlock, ImageSource, MessageContent, ToolCallRequest};
```

- [ ] **Step 2: 验证编译**

```bash
cargo build -p peri-agent
```

- [ ] **Step 3: 运行测试**

```bash
cargo test -p peri-agent --lib -- anthropic
```

---

### Task 3: 提取共享 `build_reqwest_client()`

**Files:**
- Modify: `peri-agent/src/llm/mod.rs`
- Modify: `peri-agent/src/llm/openai/mod.rs`
- Modify: `peri-agent/src/llm/anthropic/mod.rs`

- [ ] **Step 1: 在 `llm/mod.rs` 添加共享函数**

在 `peri-agent/src/llm/mod.rs` 中，`pub use retry::{RetryConfig, RetryableLLM};` 之后添加：

```rust
/// Build a reqwest client with connection pool limits to prevent TLS session
/// accumulation. Default pool is unbounded — each idle connection holds
/// ~50-100 KB of TLS state that is never released.
pub(crate) fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
```

- [ ] **Step 2: 修改 `openai/mod.rs`**

删除 `build_reqwest_client()` 函数定义（第 10-16 行），并将 `client: build_reqwest_client()` 改为 `client: super::build_reqwest_client()`。

在 `ChatOpenAI::new()` 中：

```rust
client: super::build_reqwest_client(),
```

- [ ] **Step 3: 修改 `anthropic/mod.rs`**

同上：删除 `build_reqwest_client()` 函数定义（第 8-14 行），将 `client: build_reqwest_client()` 改为：

```rust
client: super::build_reqwest_client(),
```

- [ ] **Step 4: 验证编译**

```bash
cargo build -p peri-agent
```

---

### Task 4: 提取 OpenAI `TokenUsage` 构建为共享函数

**Files:**
- Modify: `peri-agent/src/llm/openai/invoke.rs`
- Modify: `peri-agent/src/llm/openai/stream.rs`

- [ ] **Step 1: 在 `invoke.rs` 添加 `extract_openai_usage` 函数**

在 `build_request_body` 函数之后添加：

```rust
/// 从 OpenAI API 响应中提取 TokenUsage
///
/// OpenAI 格式：`usage.prompt_tokens` / `usage.completion_tokens` /
/// `usage.prompt_tokens_details.cached_tokens`
pub(super) fn extract_openai_usage(
    usage_val: &serde_json::Value,
    request_id: Option<String>,
) -> Option<crate::llm::types::TokenUsage> {
    let input = usage_val["prompt_tokens"].as_u64().map(|v| v as u32);
    let output = usage_val["completion_tokens"].as_u64().map(|v| v as u32);
    let cache_read = usage_val["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .map(|v| v as u32);
    match (input, output) {
        (Some(i), Some(o)) => Some(crate::llm::types::TokenUsage {
            input_tokens: i,
            output_tokens: o,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: cache_read,
            request_id,
        }),
        _ => None,
    }
}
```

- [ ] **Step 2: 在 `invoke.rs` 的 `BaseModel::invoke` 中使用**

替换 `invoke.rs` 中 invoke 方法的 usage 构建块（约第 517-537 行）：

旧代码：
```rust
let usage = {
    let input = resp_json["usage"]["prompt_tokens"]
        .as_u64()
        .map(|v| v as u32);
    let output = resp_json["usage"]["completion_tokens"]
        .as_u64()
        .map(|v| v as u32);
    let cache_read = resp_json["usage"]["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .map(|v| v as u32);
    match (input, output) {
        (Some(i), Some(o)) => Some(crate::llm::types::TokenUsage {
            input_tokens: i,
            output_tokens: o,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: cache_read,
            request_id: request_id.clone(),
        }),
        _ => None,
    }
};
```

新代码：
```rust
let usage = extract_openai_usage(&resp_json["usage"], request_id.clone());
```

- [ ] **Step 3: 在 `stream.rs` 中使用**

在 `stream.rs` 顶部添加 import：

```rust
use super::invoke::extract_openai_usage;
```

替换 stream.rs 中两处重复的 usage 构建块：

**位置 1（ToolUse 分支，约第 232-248 行）：**

旧代码：
```rust
let usage = final_usage.as_ref().and_then(|u| {
    let input = u["prompt_tokens"].as_u64().map(|v| v as u32);
    let output = u["completion_tokens"].as_u64().map(|v| v as u32);
    let cache_read = u["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .map(|v| v as u32);
    match (input, output) {
        (Some(i), Some(o)) => Some(crate::llm::types::TokenUsage {
            input_tokens: i,
            output_tokens: o,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: cache_read,
            request_id: stream_request_id.clone(),
        }),
        _ => None,
    }
});
```

新代码：
```rust
let usage = final_usage.as_ref().and_then(|u| extract_openai_usage(u, stream_request_id.clone()));
```

**位置 2（text 分支，约第 270-285 行）：** 同样替换。

- [ ] **Step 4: 验证编译和测试**

```bash
cargo build -p peri-agent && cargo test -p peri-agent --lib -- openai
```

---

### Task 5: 合并 OpenAI stream `LlmResponse` 构建的双分支

**Files:**
- Modify: `peri-agent/src/llm/openai/stream.rs`

- [ ] **Step 1: 提取 `build_openai_llm_response` 函数**

在 `do_invoke_streaming` 函数之后（文件末尾），添加：

```rust
/// 从流式累积状态构建最终 LlmResponse
///
/// ToolUse 和 text 两种 stop_reason 的 LlmResponse 构建逻辑合并，
/// 差异仅在 content 和 message 类型上。
fn build_stream_response(
    reasoning_text: &str,
    content_text: &str,
    tool_call_requests: Vec<crate::messages::ToolCallRequest>,
    stop_reason: crate::llm::types::StopReason,
    usage: Option<crate::llm::types::TokenUsage>,
    request_id: Option<String>,
) -> crate::llm::types::LlmResponse {
    use crate::llm::types::StopReason;
    use crate::messages::{BaseMessage, ContentBlock, MessageContent};

    let mut blocks: Vec<ContentBlock> = Vec::new();
    if !reasoning_text.is_empty() {
        blocks.push(ContentBlock::reasoning(reasoning_text));
    }

    if stop_reason == StopReason::ToolUse {
        for tc in &tool_call_requests {
            blocks.push(ContentBlock::tool_use(
                &tc.id,
                &tc.name,
                tc.arguments.clone(),
            ));
        }
        if content_text.is_empty() && blocks.is_empty() {
            blocks.push(ContentBlock::text(""));
        }
        let content = if blocks.len() == 1 && blocks[0].as_text().is_some() {
            MessageContent::text(content_text)
        } else {
            MessageContent::Blocks(blocks)
        };
        let message = BaseMessage::ai_with_tool_calls(content, tool_call_requests);
        crate::llm::types::LlmResponse {
            message,
            stop_reason,
            usage,
            request_id,
        }
    } else {
        if !content_text.is_empty() {
            blocks.push(ContentBlock::text(content_text));
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::text(""));
        }
        let content = if blocks.len() == 1 && blocks[0].as_text().is_some() {
            MessageContent::text(content_text)
        } else {
            MessageContent::Blocks(blocks)
        };
        let message = BaseMessage::ai(content);
        crate::llm::types::LlmResponse {
            message,
            stop_reason,
            usage,
            request_id,
        }
    }
}
```

- [ ] **Step 2: 简化 `do_invoke_streaming` 末尾**

将 stream.rs 第 210-295 行（从 `// Build content blocks` 到函数结尾）替换为：

```rust
    let stop_reason = StopReason::from_openai(finish_reason.as_deref().unwrap_or("stop"));
    let usage = final_usage.as_ref().and_then(|u| extract_openai_usage(u, stream_request_id.clone()));

    Ok(build_stream_response(
        &reasoning_text,
        &content_text,
        tool_call_requests,
        stop_reason,
        usage,
        stream_request_id,
    ))
```

- [ ] **Step 3: 验证编译和测试**

```bash
cargo build -p peri-agent && cargo test -p peri-agent --lib -- openai
```

---

### Task 6: 全量验证和提交

**Files:**
- 无新增修改

- [ ] **Step 1: 全量构建**

```bash
cargo build
```

Expected: 所有 workspace crate 编译成功

- [ ] **Step 2: 全量测试**

```bash
cargo test -p peri-agent
```

Expected: 所有测试通过

- [ ] **Step 3: Clippy 检查**

```bash
cargo clippy -p peri-agent --lib -- -D warnings
```

Expected: 无 warning

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "refactor: extract shared LLM adapter code

- Remove redundant impl ReactLLM for ChatOpenAI/ChatAnthropic
  (production path uses BaseModelReactLLM exclusively)
- Extract build_reqwest_client() to llm/mod.rs
- Extract extract_openai_usage() for OpenAI TokenUsage construction
- Simplify OpenAI stream LlmResponse building

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Self-Review

### 1. Spec Coverage

所有 5 个重复项都有对应 Task：
- ✅ 冗余 `impl ReactLLM for ChatOpenAI` → Task 1
- ✅ 冗余 `impl ReactLLM for ChatAnthropic` → Task 2
- ✅ `build_reqwest_client()` 重复 → Task 3
- ✅ OpenAI `TokenUsage` 构建重复 ×3 → Task 4
- ✅ OpenAI stream `LlmResponse` 双分支 → Task 5

### 2. Placeholder Scan

无 TBD/TODO/占位符。

### 3. Type Consistency

- `extract_openai_usage` 的参数签名 `(Value, Option<String>)` 与 invoke.rs 和 stream.rs 中的实际使用一致
- `build_stream_response` 的参数与 stream.rs 中的局部变量类型一致
- `super::build_reqwest_client()` 的可见性为 `pub(crate)`，子模块通过 `super::` 访问

### 风险评估

| Task | 风险 | 缓解 |
|------|------|------|
| Task 1-2（删除 ReactLLM impl） | 低：生产代码仅通过 `BaseModelReactLLM` 路径 | Grep 确认无直接使用 |
| Task 3（build_reqwest_client） | 极低：纯移动 | 编译验证 |
| Task 4（extract_openai_usage） | 低：逻辑不变，仅提取 | 单元测试覆盖 |
| Task 5（build_stream_response） | 中：合并条件分支 | 需验证 ToolUse/text 两条路径的 message 构建差异 |

### 注意事项

- **不改动 `messages/adapters/` 层**：与 `llm/` 层的转换函数有不同语义（持久化 vs API 请求），统一风险高且收益低
- **不改 Anthropic stream/invoke 的 response 构建**：Anthropic 的 `handle_anthropic_response` 已做过提取，剩余重复（invoke vs stream 的 message 构建）因 Anthropic 特有的 `cache_creation/cache_read` 规范化逻辑交织，不宜强行合并
