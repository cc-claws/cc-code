# 统一 Token Usage 传递：引入 prompt_complete 事件

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 统一 token usage 数据传递路径，消除 Category ② 双路径冗余，让 TUI 和 IDE 从同一来源获取完整、一致的 token 用量数据。

**Architecture:** 在 `LlmCallEnd` 事件中补充 `stop_reason` 字段，将 mapper 中的 Category ② 映射改为单路径——`session/update` 携带完整 usage（通过 `_meta` 扩展），TUI 从 `session/update` 消费而非 `peri/agent_event`。`ContextWarning`/`LlmRetrying` 归入 Category ③（仅 TUI）。

**Tech Stack:** Rust, tokio async, serde JSON, agent-client-protocol SDK (`UsageUpdate._meta` 扩展)

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `peri-agent/src/agent/events.rs` | `LlmCallEnd` 新增 `stop_reason` 字段 |
| `peri-agent/src/llm/types.rs` | `StopReason` 枚举定义（已存在） |
| `peri-agent/src/agent/react.rs` | `Reasoning` 新增 `stop_reason` 字段，从 `LlmResponse` 传播 |
| `peri-agent/src/llm/react_adapter.rs` | `BaseModelReactLLM::generate_reasoning` 传播 `stop_reason` |
| `peri-agent/src/agent/executor/llm_step.rs` | `call_llm` 传递 `stop_reason` 到 `LlmCallEnd` |
| `peri-acp/src/event/mapper.rs` | 核心映射改造：Category ② → 单路径 + `_meta` 扩展 |
| `peri-acp/src/session/event_sink.rs` | EventSink 不变（已正确路由 `MappedEvent`） |
| `peri-tui/src/app/agent.rs` | `map_executor_event` 移除 `LlmCallEnd` Category ② 分支 |
| `peri-tui/src/app/agent_ops/acp_bridge.rs` | `handle_session_update_peri` 处理 enriched `usage_update` |
| `peri-tui/src/app/agent_ops/subagent.rs` | `handle_token_usage_update` 不变（接收类型不变） |
| `peri-tui/src/app/events.rs` | `AgentEvent::TokenUsageUpdate` 新增 `stop_reason` 字段 |

---

### Task 1: 传播 stop_reason — Reasoning 结构体

**Files:**
- Modify: `peri-agent/src/agent/react.rs:126-138`
- Modify: `peri-agent/src/llm/react_adapter.rs:85-195`
- Modify: `peri-agent/src/agent/executor/llm_step.rs:69-80`

- [ ] **Step 1: 给 Reasoning 添加 stop_reason 字段**

在 `peri-agent/src/agent/react.rs` 的 `Reasoning` 结构体中添加 `stop_reason` 字段：

```rust
// react.rs:126
pub struct Reasoning {
    pub thought: String,
    pub tool_calls: Vec<ToolCall>,
    pub final_answer: Option<String>,
    pub source_message: Option<BaseMessage>,
    pub usage: Option<crate::llm::types::TokenUsage>,
    pub model: String,
    pub streamed: bool,
    /// LLM 响应的停止原因（end_turn / tool_use / max_tokens）
    pub stop_reason: crate::llm::types::StopReason,
}
```

更新 `Reasoning::with_tools` 和 `Reasoning::with_answer` 构造方法，添加 `stop_reason` 参数：

```rust
// react.rs:140
impl Reasoning {
    pub fn with_tools(thought: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            thought: thought.into(),
            tool_calls,
            final_answer: None,
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
            stop_reason: crate::llm::types::StopReason::ToolUse,
        }
    }

    pub fn with_answer(answer: impl Into<String>) -> Self {
        Self {
            thought: String::new(),
            tool_calls: vec![],
            final_answer: Some(answer.into()),
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
            stop_reason: crate::llm::types::StopReason::EndTurn,
        }
    }
}
```

- [ ] **Step 2: 在 react_adapter 中传播 stop_reason**

在 `peri-agent/src/llm/react_adapter.rs` 的 `generate_reasoning` 方法中，将 `response.stop_reason` 传播到 `Reasoning`：

在 `Reasoning::with_tools` 和 `Reasoning::with_answer` 的所有构造点，改为传入 `response.stop_reason.clone()`。具体位置：

- `react_adapter.rs:~143`（`Reasoning::with_tools` 调用处）：
```rust
let mut r = Reasoning::with_tools(text, calls);
// 改为：
let mut r = Reasoning {
    stop_reason: response.stop_reason.clone(),
    ..Reasoning::with_tools(text, calls)
};
```

- `react_adapter.rs:~155`（`Reasoning::with_answer` 调用处）：
```rust
let mut r = Reasoning::with_answer(text);
// 改为：
let mut r = Reasoning {
    stop_reason: response.stop_reason.clone(),
    ..Reasoning::with_answer(text)
};
```

- `react_adapter.rs:~185`（MaxTokens 路径的 `Reasoning::with_tools`）：
```rust
// 同样模式，添加 stop_reason
let mut r = Reasoning {
    stop_reason: response.stop_reason.clone(),
    ..Reasoning::with_tools(text, calls)
};
```

- [ ] **Step 3: 在 llm_step 中传递 stop_reason 到 LlmCallEnd**

在 `peri-agent/src/agent/executor/llm_step.rs`，修改 `LlmCallEnd` 发射点：

```rust
// llm_step.rs:75
agent.emit(AgentEvent::LlmCallEnd {
    step,
    model: agent.llm.model_name(),
    output: llm_output,
    usage: reasoning.usage.clone(),
    stop_reason: Some(reasoning.stop_reason.clone()),
});
```

错误路径（`llm_step.rs:57`）保持 `stop_reason: None`（LLM 报错无 stop_reason）。

- [ ] **Step 4: 更新 LlmCallEnd 变体定义**

在 `peri-agent/src/agent/events.rs` 中给 `LlmCallEnd` 添加 `stop_reason` 字段：

```rust
/// LLM 调用结束（携带模型名、输出文本、token 使用量、停止原因）
LlmCallEnd {
    step: usize,
    model: String,
    output: String,
    usage: Option<crate::llm::types::TokenUsage>,
    /// LLM 响应停止原因（None 表示 LLM 调用失败/异常）
    stop_reason: Option<crate::llm::types::StopReason>,
},
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-agent`
Expected: 编译通过（可能需要修复测试中 `LlmCallEnd` 的构造点——搜索所有 `AgentEvent::LlmCallEnd` 调用处，添加 `stop_reason` 字段）

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(agent): propagate stop_reason through Reasoning → LlmCallEnd

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: Mapper 改造 — enriched UsageUpdate via _meta

**Files:**
- Modify: `peri-acp/src/event/mapper.rs`
- Create: `peri-acp/src/event/mapper_test.rs`

- [ ] **Step 1: 编写 mapper 测试 — enriched usage_update**

创建 `peri-acp/src/event/mapper_test.rs`：

```rust
use peri_agent::agent::events::AgentEvent as ExecutorEvent;
use peri_agent::llm::types::{TokenUsage, StopReason};

use super::*;

#[test]
fn test_llm_call_end_maps_to_enriched_usage_update() {
    let event = ExecutorEvent::LlmCallEnd {
        step: 1,
        model: "claude-sonnet-4-20250514".to_string(),
        output: "Hello".to_string(),
        usage: Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: Some(10),
            cache_read_input_tokens: Some(200),
            request_id: Some("req-123".to_string()),
        }),
        stop_reason: Some(StopReason::EndTurn),
    };

    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1, "应产出 1 个 MappedEvent");

    let m = &mapped[0];
    // 新行为：forward_to_tui = false（不再通过 peri/agent_event 转发）
    assert!(!m.forward_to_tui, "LlmCallEnd 不应再转发到 TUI（改为 session/update 单路径）");
    assert_eq!(m.updates.len(), 1, "应包含 1 个 SessionUpdate");

    match &m.updates[0] {
        SessionUpdate::UsageUpdate(usage) => {
            assert_eq!(usage.used, 150); // input_tokens + output_tokens
            assert_eq!(usage.size, 200_000);
            // 验证 _meta 中的详细字段
            let meta = usage.meta.as_ref().expect("_meta 应包含详细 usage");
            assert_eq!(meta.get("inputTokens").unwrap().as_u64(), Some(100));
            assert_eq!(meta.get("outputTokens").unwrap().as_u64(), Some(50));
            assert_eq!(meta.get("cacheCreationTokens").unwrap().as_u64(), Some(10));
            assert_eq!(meta.get("cacheReadTokens").unwrap().as_u64(), Some(200));
            assert_eq!(meta.get("model").unwrap().as_str(), Some("claude-sonnet-4-20250514"));
            assert_eq!(meta.get("stopReason").unwrap().as_str(), Some("end_turn"));
        }
        other => panic!("预期 UsageUpdate，实际: {:?}", other),
    }
}

#[test]
fn test_llm_call_end_no_usage_produces_nothing() {
    let event = ExecutorEvent::LlmCallEnd {
        step: 1,
        model: "test".to_string(),
        output: "ERROR".to_string(),
        usage: None,
        stop_reason: None,
    };
    let mapped = map_event(&event, 200_000);
    assert!(mapped.is_empty() || mapped.iter().all(|m| m.updates.is_empty() && !m.forward_to_tui),
        "LlmCallEnd usage=None 应被过滤");
}

#[test]
fn test_context_warning_is_tui_only() {
    let event = ExecutorEvent::ContextWarning {
        used_tokens: 150000,
        total_tokens: 200000,
        percentage: 75.0,
    };
    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1);
    assert!(mapped[0].forward_to_tui, "ContextWarning 应转发到 TUI");
    assert!(mapped[0].updates.is_empty(), "ContextWarning 不应产生 SessionUpdate（改为 Category ③）");
}

#[test]
fn test_llm_retrying_is_tui_only() {
    let event = ExecutorEvent::LlmRetrying {
        attempt: 2,
        max_attempts: 3,
        delay_ms: 1000,
        error: "timeout".to_string(),
    };
    let mapped = map_event(&event, 200_000);
    assert_eq!(mapped.len(), 1);
    assert!(mapped[0].forward_to_tui, "LlmRetrying 应转发到 TUI");
    assert!(mapped[0].updates.is_empty(), "LlmRetrying 不应产生 SessionUpdate（改为 Category ③）");
}
```

在 `peri-acp/src/event/mapper.rs` 底部添加测试模块声明：

```rust
#[cfg(test)]
#[path = "mapper_test.rs"]
mod tests;
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-acp -- event::mapper::tests`
Expected: FAIL — `LlmCallEnd` 映射逻辑尚未更新

- [ ] **Step 3: 改造 mapper 核心逻辑**

在 `peri-acp/src/event/mapper.rs` 中修改三处映射：

**3a. `LlmCallEnd` — 从 Category ② 改为 Category ①（enriched UsageUpdate）**

```rust
// 替换 mapper.rs:178-185
ExecutorEvent::LlmCallEnd {
    usage: Some(u),
    model,
    stop_reason,
    ..
} => {
    let mut meta = serde_json::Map::new();
    meta.insert("inputTokens".into(), serde_json::json!(u.input_tokens));
    meta.insert("outputTokens".into(), serde_json::json!(u.output_tokens));
    if let Some(v) = u.cache_creation_input_tokens {
        meta.insert("cacheCreationTokens".into(), serde_json::json!(v));
    }
    if let Some(v) = u.cache_read_input_tokens {
        meta.insert("cacheReadTokens".into(), serde_json::json!(v));
    }
    meta.insert("model".into(), serde_json::json!(model));
    if let Some(ref sr) = stop_reason {
        meta.insert("stopReason".into(), serde_json::json!(sr.to_string()));
    }

    vec![MappedEvent::standard(vec![SessionUpdate::UsageUpdate(
        UsageUpdate::new(
            u64::from(u.input_tokens) + u64::from(u.output_tokens),
            u64::from(context_window),
        )
        .meta(meta),
    )])]
}
```

注意：`forward_to_tui: false`（不再通过 `peri/agent_event` 转发）。

**3b. `ContextWarning` — 从 Category ② 改为 Category ③**

```rust
// 替换 mapper.rs:187-195
ExecutorEvent::ContextWarning { .. } => {
    vec![MappedEvent::tui_only()]
}
```

**3c. `LlmRetrying` — 从 Category ② 改为 Category ③**

```rust
// 替换 mapper.rs:197-209
ExecutorEvent::LlmRetrying { .. } => {
    vec![MappedEvent::tui_only()]
}
```

**3d. `LlmCallEnd { usage: None, .. }` — 保持 Category 过滤（不变）**

```rust
// 保持 mapper.rs:229 的过滤行为
ExecutorEvent::LlmCallEnd { usage: None, .. } => {
    vec![MappedEvent::none()]
}
```

但需要更新模式匹配，因为 `LlmCallEnd` 新增了 `stop_reason` 字段。将所有 `LlmCallEnd` 的匹配分支更新为包含 `stop_reason` 字段。

- [ ] **Step 4: 给 StopReason 添加 Display impl**

`peri-agent/src/llm/types.rs` 的 `StopReason` 需要实现 `ToString` 或 `Display` 以便在 `_meta` 中序列化：

```rust
impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::EndTurn => write!(f, "end_turn"),
            StopReason::ToolUse => write!(f, "tool_use"),
            StopReason::MaxTokens => write!(f, "max_tokens"),
            StopReason::Other(s) => write!(f, "{}", s),
        }
    }
}
```

检查是否已有 Display impl（derive_more 可能已提供）。如果没有，添加此 impl。

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p peri-acp -- event::mapper::tests`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(acp): enrich UsageUpdate with full token breakdown via _meta

- LlmCallEnd: Category ② → ①, enriched _meta (inputTokens/outputTokens/cache/model/stopReason)
- ContextWarning/LlmRetrying: Category ② → ③ (TUI-only, no lossy SessionUpdate)
- Add mapper tests for new behavior

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: TUI 消费路径改造

**Files:**
- Modify: `peri-tui/src/app/events.rs`
- Modify: `peri-tui/src/app/agent.rs`
- Modify: `peri-tui/src/app/agent_ops/acp_bridge.rs`
- Modify: `peri-tui/src/app/agent_ops/subagent.rs`

- [ ] **Step 1: 更新 AgentEvent::TokenUsageUpdate 添加 stop_reason**

在 `peri-tui/src/app/events.rs` 中：

```rust
/// Token 使用量更新（从 session/update enriched UsageUpdate 解析而来）
TokenUsageUpdate {
    usage: peri_agent::llm::types::TokenUsage,
    model: String,
    /// LLM 响应停止原因
    stop_reason: Option<peri_agent::llm::types::StopReason>,
},
```

- [ ] **Step 2: 移除 map_executor_event 中 LlmCallEnd 的 Category ② 分支**

在 `peri-tui/src/app/agent.rs` 中，将 `LlmCallEnd` 从 Category ② 分支移到过滤分支：

```rust
// agent.rs — 移除 LlmCallEnd 的 Category ② 映射（~line 90-94）
// 将 ExecutorEvent::LlmCallEnd { usage: Some(usage), model, .. } 分支删除

// 更新过滤分支（~line 107-117），添加 LlmCallEnd 所有变体：
ExecutorEvent::TextChunk { .. }
| ExecutorEvent::AiReasoning(_)
| ExecutorEvent::ToolStart { .. }
| ExecutorEvent::ToolEnd { .. }
| ExecutorEvent::TodoUpdate(_)
| ExecutorEvent::LlmCallEnd { .. }  // ← 新增：所有 LlmCallEnd 都过滤
| ExecutorEvent::StepDone { .. }
| ExecutorEvent::MessageAdded(_)
| ExecutorEvent::LlmCallStart { .. }
| ExecutorEvent::SessionEnded => return None,
```

- [ ] **Step 3: 在 acp_bridge.rs 中处理 enriched usage_update**

在 `peri-tui/src/app/agent_ops/acp_bridge.rs` 的 `handle_session_update_peri` 中，替换 `"usage_update"` 分支：

```rust
// 替换 acp_bridge.rs:204-207
"usage_update" => {
    // 从 enriched UsageUpdate _meta 中解析完整 token 用量
    let meta = update.get("_meta");
    let input_tokens = meta
        .and_then(|m| m.get("inputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = meta
        .and_then(|m| m.get("outputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_creation_input_tokens = meta
        .and_then(|m| m.get("cacheCreationTokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let cache_read_input_tokens = meta
        .and_then(|m| m.get("cacheReadTokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let model = meta
        .and_then(|m| m.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let stop_reason_str = meta
        .and_then(|m| m.get("stopReason"))
        .and_then(|v| v.as_str())
        .unwrap_or("end_turn");
    let stop_reason = Some(peri_agent::llm::types::StopReason::from_display(stop_reason_str));

    let usage = peri_agent::llm::types::TokenUsage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
        request_id: None,
    };

    self.handle_agent_event(super::super::AgentEvent::TokenUsageUpdate {
        usage,
        model,
        stop_reason,
    })
}
```

- [ ] **Step 4: 给 StopReason 添加 from_display 方法**

在 `peri-agent/src/llm/types.rs` 中添加反向解析方法：

```rust
impl StopReason {
    /// 从字符串表示解析 StopReason（用于 TUI 从 _meta 反序列化）
    pub fn from_display(s: &str) -> Self {
        match s {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            other => StopReason::Other(other.to_string()),
        }
    }
}
```

注意：如果已有 `from_openai` / `from_anthropic` 方法，检查是否有冲突。

- [ ] **Step 5: 更新 handle_token_usage_update 接收端**

检查 `peri-tui/src/app/agent_ops/subagent.rs` 的 `handle_token_usage_update` —— 函数签名不变（只接收 `usage: TokenUsage`），无需修改。

检查 `peri-tui/src/app/agent_ops/mod.rs` 中 `TokenUsageUpdate` 的 match 分支，更新解构以匹配新字段：

```rust
// mod.rs:147-150
AgentEvent::TokenUsageUpdate {
    usage,
    model: _model,
    stop_reason: _stop_reason,  // ← 新增，暂不使用
} => self.handle_token_usage_update(usage),
```

- [ ] **Step 6: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(tui): consume enriched usage_update from session/update

- Remove LlmCallEnd from peri/agent_event mapping (no more dual-path)
- Parse full token breakdown from UsageUpdate._meta
- Add stop_reason to AgentEvent::TokenUsageUpdate

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: Stdio 路径适配

**Files:**
- Modify: `peri-acp/src/session/event_sink.rs`

- [ ] **Step 1: 验证 StdioEventSink 行为**

`StdioEventSink::push_event` 已经通过 `map_event` → `SessionUpdate` 发送。改造后：
- `LlmCallEnd` → `UsageUpdate`（enriched `_meta`）→ IDE 获得完整数据 ✓
- `ContextWarning` → `tui_only()` → IDE 不再收到无意义的通知 ✓
- `LlmRetrying` → `tui_only()` → IDE 不再收到 ✓

无需修改 `event_sink.rs`，但需要确认 IDE 客户端能正确消费 enriched `_meta`。

- [ ] **Step 2: 全量编译 + 测试**

Run: `cargo build && cargo test`
Expected: 全部通过

- [ ] **Step 3: Commit（如有改动）**

```bash
git add -A && git commit -m "chore: verify stdio path compatibility with enriched UsageUpdate

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 5: 更新现有测试

**Files:**
- Modify: `peri-tui/src/app/agent_test.rs`
- Modify: `peri-agent/src/agent/executor/mod_test.rs`
- Modify: 其他引用 `LlmCallEnd` / `Reasoning` 的测试文件

- [ ] **Step 1: 修复 agent_test.rs**

搜索所有测试中构造 `ExecutorEvent::LlmCallEnd` 或 `Reasoning` 的地方，添加 `stop_reason` 字段：

```bash
grep -rn "LlmCallEnd" --include="*_test.rs" peri-agent/ peri-acp/ peri-tui/
grep -rn "Reasoning::" --include="*_test.rs" peri-agent/ peri-acp/ peri-tui/
```

对每个匹配点：
- `LlmCallEnd` 添加 `stop_reason: Some(StopReason::EndTurn)` 或 `stop_reason: None`
- `Reasoning::with_tools()` / `Reasoning::with_answer()` 使用 struct 更新语法添加 `stop_reason`

- [ ] **Step 2: 运行全量测试**

Run: `cargo test`
Expected: 全部通过

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "test: update tests for LlmCallEnd stop_reason and Reasoning changes

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 6: 清理与文档更新

**Files:**
- Modify: `CLAUDE.md`（如有必要更新 Category 描述）
- Modify: issue 文件状态为 Fixed

- [ ] **Step 1: 更新 CLAUDE.md 中的 Category 描述**

在 CLAUDE.md 的事件映射部分，更新 Category 定义：

```
Category ① (Full SessionUpdate): TextChunk, AiReasoning, ToolStart, ToolEnd, TodoUpdate, LlmCallEnd(usage)
  → `updates` only, `forward_to_tui: false`
Category ③ (TUI-only): StateSnapshot, Subagent*, Compact*, ContextWarning, LlmRetrying, etc.
  → `forward_to_tui: true` only
Filtered: StepDone, MessageAdded, LlmCallStart, SessionEnded, LlmCallEnd(usage:None)
  → empty
```

注意：Category ② 已被消除。`LlmCallEnd` 升级为 Category ①（通过 enriched `_meta`），`ContextWarning`/`LlmRetrying` 降级为 Category ③。

- [ ] **Step 2: 更新 issue 状态**

将 `spec/issues/2026-05-29-unify-token-usage-prompt-complete.md` 的状态改为 Fixed。

- [ ] **Step 3: 最终验证**

Run: `cargo build && cargo test && lefthook run pre-commit`
Expected: 全部通过

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: update CLAUDE.md category descriptions after dual-path elimination

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```
