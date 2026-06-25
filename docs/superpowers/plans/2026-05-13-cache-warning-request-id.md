# 缓存命中率警告显示 API Request ID 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Prompt cache 命中率低于 80% 的警告消息中附带 API request ID，方便在 Anthropic Console / OpenAI Dashboard 中定位请求。

**Architecture:** 在 `LlmResponse` 和 `TokenUsage` 中新增 `request_id` 字段，由各 provider 的 `invoke` 方法从 API 响应中提取。`TokenTracker` 在 `accumulate` 时记录 `last_request_id`，TUI 层在生成警告消息时读取并拼接。数据流保持与现有 `usage` 一致的路径，不新增事件或接口。

**Tech Stack:** Rust, reqwest (response headers), serde_json

---

### Task 1: `TokenUsage` 和 `LlmResponse` 新增 `request_id` 字段

**Files:**
- Modify: `peri-agent/src/llm/types.rs:43-60`

- [ ] **Step 1: 在 `TokenUsage` 中新增 `request_id` 字段**

在 `TokenUsage` struct 中 `cache_read_input_tokens` 之后添加：

```rust
/// API 提供商返回的请求 ID（Anthropic: x-request-id header, OpenAI: response body id 字段）
pub request_id: Option<String>,
```

- [ ] **Step 2: 在 `LlmResponse` 中新增 `request_id` 字段**

在 `LlmResponse` struct 中 `usage` 之后添加：

```rust
/// API 提供商返回的请求 ID
pub request_id: Option<String>,
```

- [ ] **Step 3: 更新 types.rs 中的测试（如有构造 TokenUsage 的地方）**

`types.rs` 的测试中没有直接构造 `TokenUsage`，无需修改。

- [ ] **Step 4: 编译验证**

Run: `cargo check -p peri-agent 2>&1 | head -40`
Expected: 编译错误指向 `anthropic.rs` 和 `openai.rs` 中构造 `TokenUsage` 和 `LlmResponse` 的位置（缺少 `request_id` 字段）

---

### Task 2: `ChatAnthropic::invoke` 提取 `x-request-id` header

**Files:**
- Modify: `peri-agent/src/llm/anthropic.rs:557-559`（header 提取点）
- Modify: `peri-agent/src/llm/anthropic.rs:670-705`（TokenUsage + LlmResponse 构造点）

- [ ] **Step 1: 在 `resp.text().await` 之前提取 `x-request-id` header**

在 `anthropic.rs:558`（`let status = resp.status();`）之后、`let resp_text = resp.text().await` 之前添加：

```rust
let request_id = resp
    .headers()
    .get("x-request-id")
    .and_then(|v| v.to_str().ok())
    .map(|s| s.to_string());
```

- [ ] **Step 2: 在 `TokenUsage` 构造中填入 `request_id`**

在 `anthropic.rs:689` 的 `TokenUsage` 构造中添加 `request_id: request_id.clone()`：

```rust
match (resp_json["usage"]["input_tokens"].as_u64(), output) {
    (Some(_), Some(o)) => Some(crate::llm::types::TokenUsage {
        input_tokens: raw_input + cache_creation + cache_read,
        output_tokens: o,
        cache_creation_input_tokens: Some(cache_creation),
        cache_read_input_tokens: Some(cache_read),
        request_id: request_id.clone(),
    }),
    _ => None,
}
```

- [ ] **Step 3: 在 `LlmResponse` 构造中填入 `request_id`**

在 `anthropic.rs:701` 的 `LlmResponse` 构造中添加 `request_id`：

```rust
Ok(LlmResponse {
    message,
    stop_reason,
    usage,
    request_id,
})
```

- [ ] **Step 4: 编译验证**

Run: `cargo check -p peri-agent 2>&1 | head -40`
Expected: 编译错误仅指向 `openai.rs`（Task 3 修复）

---

### Task 3: `ChatOpenAI::invoke` 提取 request `id` 字段

**Files:**
- Modify: `peri-agent/src/llm/openai.rs:590-614`（TokenUsage + LlmResponse 构造点）

- [ ] **Step 1: 在 `TokenUsage` 构造中填入 `request_id`**

在 `openai.rs:601` 的 `TokenUsage` 构造之前提取 request_id：

```rust
let request_id = resp_json["id"].as_str().map(|s| s.to_string());
```

然后在构造中添加字段：

```rust
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
```

- [ ] **Step 2: 在 `LlmResponse` 构造中填入 `request_id`**

在 `openai.rs:610` 的 `LlmResponse` 构造中添加 `request_id`：

```rust
Ok(LlmResponse {
    message,
    stop_reason,
    usage,
    request_id,
})
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p peri-agent 2>&1 | head -40`
Expected: 编译错误指向 `react_adapter.rs` 和 `anthropic.rs`/`openai.rs` 中 `generate_reasoning` 方法（`Reasoning` 不携带 request_id，但 `usage` 已携带，无需修改 Reasoning）

---

### Task 4: `TokenTracker` 记录 `last_request_id`

**Files:**
- Modify: `peri-agent/src/agent/token.rs:5-18`（TokenTracker struct）
- Modify: `peri-agent/src/agent/token.rs:21-36`（accumulate 方法）
- Test: `peri-agent/src/agent/token.rs`（测试模块）

- [ ] **Step 1: 在 `TokenTracker` 中新增 `last_request_id` 字段**

在 `llm_call_count` 之后添加：

```rust
/// 最近一次 LLM 响应的 API request ID
pub last_request_id: Option<String>,
```

- [ ] **Step 2: 在 `accumulate` 中更新 `last_request_id`**

在 `accumulate` 方法末尾（`self.llm_call_count += 1;` 之后）添加：

```rust
self.last_request_id = usage.request_id.clone();
```

- [ ] **Step 3: 更新 `reset` 方法**

`reset` 使用 `*self = Self::default()`，`Default` 会自动将 `last_request_id` 设为 `None`，无需额外修改。

- [ ] **Step 4: 添加测试**

在 `token.rs` 测试模块中追加：

```rust
#[test]
fn test_accumulate_records_request_id() {
    let mut tracker = TokenTracker::default();
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        request_id: Some("req_01ABC".to_string()),
    };
    tracker.accumulate(&usage);
    assert_eq!(tracker.last_request_id.as_deref(), Some("req_01ABC"));
}

#[test]
fn test_accumulate_overwrites_request_id() {
    let mut tracker = TokenTracker::default();
    let usage1 = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        request_id: Some("req_01ABC".to_string()),
    };
    tracker.accumulate(&usage1);
    let usage2 = TokenUsage {
        input_tokens: 200,
        output_tokens: 80,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        request_id: Some("req_02DEF".to_string()),
    };
    tracker.accumulate(&usage2);
    assert_eq!(tracker.last_request_id.as_deref(), Some("req_02DEF"));
}

#[test]
fn test_accumulate_none_request_id() {
    let mut tracker = TokenTracker::default();
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        request_id: None,
    };
    tracker.accumulate(&usage);
    assert!(tracker.last_request_id.is_none());
}

#[test]
fn test_reset_clears_request_id() {
    let mut tracker = TokenTracker::default();
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        request_id: Some("req_01ABC".to_string()),
    };
    tracker.accumulate(&usage);
    tracker.reset();
    assert!(tracker.last_request_id.is_none());
}
```

- [ ] **Step 5: 修复已有测试中的 `TokenUsage` 构造**

`make_usage` 辅助函数需要新增 `request_id: None` 参数：

```rust
fn make_usage(
    input: u32,
    output: u32,
    cache_creation: Option<u32>,
    cache_read: Option<u32>,
) -> TokenUsage {
    TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: cache_creation,
        cache_read_input_tokens: cache_read,
        request_id: None,
    }
}
```

- [ ] **Step 6: 运行测试验证**

Run: `cargo test -p peri-agent --lib -- token`
Expected: 所有测试通过

---

### Task 5: TUI 警告消息附带 request ID

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:152-171`

- [ ] **Step 1: 修改警告消息拼接逻辑**

将 `agent_ops.rs:167-168`：

```rust
let percentage = (rate * 100.0) as u32;
let msg = format!("⚠ Prompt cache 命中率 {}% < 80%", percentage);
```

改为：

```rust
let percentage = (rate * 100.0) as u32;
let req_id = tracker.last_request_id.as_deref().unwrap_or("-");
let msg = format!("⚠ Prompt cache 命中率 {}% < 80% (req: {})", percentage, req_id);
```

注意：`tracker` 变量已在第 158-160 行定义，可直接使用其 `last_request_id` 字段。

- [ ] **Step 2: 编译验证**

Run: `cargo check -p peri-tui 2>&1 | head -40`
Expected: 编译通过

- [ ] **Step 3: 全量编译验证**

Run: `cargo build 2>&1 | tail -5`
Expected: 编译通过，无错误

---

### Task 6: 全量测试 + 提交

- [ ] **Step 1: 运行全量测试**

Run: `cargo test 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 2: 提交**

```bash
git add peri-agent/src/llm/types.rs peri-agent/src/llm/anthropic.rs peri-agent/src/llm/openai.rs peri-agent/src/agent/token.rs peri-tui/src/app/agent_ops.rs
git commit -m "feat: 缓存命中率警告显示 API request ID"
```
