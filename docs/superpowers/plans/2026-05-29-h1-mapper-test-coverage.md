# H1: mapper_test.rs ExecutorEvent 覆盖补全 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `mapper.rs` 中未覆盖的 14 个 ExecutorEvent 变体添加单元测试，覆盖率从 33% 提升到 100%。

**Architecture:** 在 `peri-acp/src/event/mapper_test.rs` 中追加测试函数，遵循现有测试的 `make_agent_event()` helper 模式。按 Category 分组测试：Category ① (SessionUpdate)、Category ③ (TUI-only)、Filtered (无输出)。

**Tech Stack:** Rust, tokio async test, serde_json

---

## 文件结构

| 操作 | 文件路径 | 职责 |
|------|----------|------|
| 修改 | `peri-acp/src/event/mapper_test.rs` | 添加 14 个新测试函数 |
| 参考 | `peri-acp/src/event/mapper.rs` | 了解 map_event 逻辑 |
| 参考 | `peri-agent/src/agent/events.rs` | ExecutorEvent 定义 |

---

### Task 1: 为 Category ① SessionUpdate 变体添加测试（AiReasoning, TextChunk, ToolStart, TodoUpdate）

**Files:**
- Modify: `peri-acp/src/event/mapper_test.rs`

- [ ] **Step 1: 添加 AiReasoning 测试**

在 `mapper_test.rs` 末尾追加：

```rust
#[test]
fn test_ai_reasoning_maps_to_session_update() {
    let event = AgentEvent::AiReasoning("思考过程...".to_string());
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert_eq!(result.updates.len(), 1);
    let update = &result.updates[0];
    assert_eq!(update["updateType"].as_str(), Some("peri/aiReasoning"));
    assert_eq!(update["content"].as_str(), Some("思考过程..."));
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test::test_ai_reasoning_maps_to_session_update`
Expected: PASS

- [ ] **Step 3: 添加 TextChunk 测试**

```rust
#[test]
fn test_text_chunk_maps_to_session_update_with_source() {
    let event = AgentEvent::TextChunk {
        message_id: "msg-1".to_string(),
        chunk: "Hello".to_string(),
        source_agent_id: Some("agent-1".to_string()),
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert_eq!(result.updates.len(), 1);
    let update = &result.updates[0];
    assert_eq!(update["updateType"].as_str(), Some("peri/textChunk"));
    assert_eq!(update["content"].as_str(), Some("Hello"));
    assert_eq!(update["sourceAgentId"].as_str(), Some("agent-1"));
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test::test_text_chunk_maps_to_session_update_with_source`
Expected: PASS

- [ ] **Step 5: 添加 ToolStart 测试**

```rust
#[test]
fn test_tool_start_maps_to_session_update_with_tool_info() {
    let event = AgentEvent::ToolStart {
        message_id: "msg-2".to_string(),
        tool_call_id: "tc-1".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
        source_agent_id: None,
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert_eq!(result.updates.len(), 1);
    let update = &result.updates[0];
    assert_eq!(update["updateType"].as_str(), Some("peri/toolStart"));
    assert_eq!(update["toolName"].as_str(), Some("Bash"));
    assert_eq!(update["toolCallId"].as_str(), Some("tc-1"));
}
```

- [ ] **Step 6: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test::test_tool_start_maps_to_session_update_with_tool_info`
Expected: PASS

- [ ] **Step 7: 添加 TodoUpdate 测试**

```rust
#[test]
fn test_todo_update_maps_to_session_update() {
    let event = AgentEvent::TodoUpdate(vec![]);
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert_eq!(result.updates.len(), 1);
    let update = &result.updates[0];
    assert_eq!(update["updateType"].as_str(), Some("peri/todoUpdate"));
}
```

- [ ] **Step 8: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test::test_todo_update_maps_to_session_update`
Expected: PASS

- [ ] **Step 9: 提交**

```bash
git add peri-acp/src/event/mapper_test.rs
git commit -m "test: add Category 1 SessionUpdate mapper tests (AiReasoning/TextChunk/ToolStart/TodoUpdate)"
```

---

### Task 2: 为 Category ③ TUI-only 变体添加测试（StateSnapshot, SubagentStarted, SubagentStopped, CompactStarted, CompactCompleted, CompactError, BackgroundTaskCompleted, LspDiagnostics, AgentExecutionFailed）

**Files:**
- Modify: `peri-acp/src/event/mapper_test.rs`

- [ ] **Step 1: 添加 StateSnapshot 测试**

```rust
#[test]
fn test_state_snapshot_is_tui_only() {
    let event = AgentEvent::StateSnapshot(vec![]);
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 2: 添加 SubagentStarted 测试**

```rust
#[test]
fn test_subagent_started_is_tui_only() {
    let event = AgentEvent::SubagentStarted {
        agent_name: "sub-1".to_string(),
        instance_id: "inst-1".to_string(),
        is_background: false,
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 3: 添加 SubagentStopped 测试**

```rust
#[test]
fn test_subagent_stopped_is_tui_only() {
    let event = AgentEvent::SubagentStopped {
        agent_name: "sub-1".to_string(),
        result: "done".to_string(),
        is_error: false,
        instance_id: "inst-1".to_string(),
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 4: 添加 CompactStarted 测试**

```rust
#[test]
fn test_compact_started_is_tui_only() {
    let event = AgentEvent::CompactStarted;
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 5: 添加 CompactCompleted 测试**

```rust
#[test]
fn test_compact_completed_is_tui_only() {
    let event = AgentEvent::CompactCompleted {
        summary: Some("摘要".to_string()),
        files: vec![],
        skills: vec![],
        micro_cleared: false,
        messages: vec![],
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 6: 添加 CompactError 测试**

```rust
#[test]
fn test_compact_error_is_tui_only() {
    let event = AgentEvent::CompactError {
        message: "压缩失败".to_string(),
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 7: 添加 BackgroundTaskCompleted 测试**

```rust
#[test]
fn test_background_task_completed_is_tui_only() {
    let event = AgentEvent::BackgroundTaskCompleted(BackgroundTaskResult {
        agent_name: "bg-1".to_string(),
        instance_id: "inst-bg".to_string(),
        result: "ok".to_string(),
        is_error: false,
    });
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 8: 添加 LspDiagnostics 测试**

```rust
#[test]
fn test_lsp_diagnostics_is_tui_only() {
    let event = AgentEvent::LspDiagnostics {
        errors: 1,
        warnings: 2,
        files_with_errors: vec!["main.rs".to_string()],
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 9: 添加 AgentExecutionFailed 测试**

```rust
#[test]
fn test_agent_execution_failed_is_tui_only() {
    let event = AgentEvent::AgentExecutionFailed {
        message: "执行失败".to_string(),
    };
    let result = map_event(event, "sess-1");
    assert!(result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 10: 运行所有 Category ③ 测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test`
Expected: ALL PASS

- [ ] **Step 11: 提交**

```bash
git add peri-acp/src/event/mapper_test.rs
git commit -m "test: add Category 3 TUI-only mapper tests (9 variants)"
```

---

### Task 3: 为 Filtered 变体添加空输出断言测试（StepDone, MessageAdded, LlmCallStart, SessionEnded）

**Files:**
- Modify: `peri-acp/src/event/mapper_test.rs`

- [ ] **Step 1: 添加 StepDone 测试**

```rust
#[test]
fn test_step_done_produces_no_output() {
    let event = AgentEvent::StepDone { step: 1 };
    let result = map_event(event, "sess-1");
    assert!(!result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 2: 添加 MessageAdded 测试**

```rust
#[test]
fn test_message_added_produces_no_output() {
    let event = AgentEvent::MessageAdded(BaseMessage::human("test"));
    let result = map_event(event, "sess-1");
    assert!(!result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 3: 添加 LlmCallStart 测试**

```rust
#[test]
fn test_llm_call_start_produces_no_output() {
    let event = AgentEvent::LlmCallStart {
        step: 1,
        messages: vec![],
        tools: vec![],
    };
    let result = map_event(event, "sess-1");
    assert!(!result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 4: 添加 SessionEnded 测试**

```rust
#[test]
fn test_session_ended_produces_no_output() {
    let event = AgentEvent::SessionEnded;
    let result = map_event(event, "sess-1");
    assert!(!result.forward_to_tui);
    assert!(result.updates.is_empty());
}
```

- [ ] **Step 5: 运行所有 Filtered 测试验证通过**

Run: `cargo test -p peri-acp --lib -- event::mapper_test`
Expected: ALL PASS

- [ ] **Step 6: 提交**

```bash
git add peri-acp/src/event/mapper_test.rs
git commit -m "test: add Filtered variant mapper tests (StepDone/MessageAdded/LlmCallStart/SessionEnded)"
```

---

### Task 4: 运行全量测试确认无回归

- [ ] **Step 1: 运行 peri-acp 全量测试**

Run: `cargo test -p peri-acp --lib`
Expected: ALL PASS

- [ ] **Step 2: 运行 peri-agent 全量测试**

Run: `cargo test -p peri-agent --lib`
Expected: ALL PASS
