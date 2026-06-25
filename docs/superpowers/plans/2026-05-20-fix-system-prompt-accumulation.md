# System Prompt 累积修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 system prompt 每轮被重复注入导致上下文膨胀的 bug——`execute()` 中 `before_agent` middleware 和 `with_system_prompt()` prepend 的 system 消息未被清理，随 `agent_state.into_messages()` 写入 history，逐轮累积。

**Architecture:** 在 `execute()` 的 ReAct 循环结束后，通过记录 prepend 消息的 ID 集合，用 `retain` 移除这些临时 system 消息。如果 compact 发生过（替换了全部消息），这些 ID 不存在于新消息中，不会误删 compact summary。修复点在 `peri-agent/src/agent/executor/mod.rs`，不涉及 TUI 或 ACP 层。

**Tech Stack:** Rust, peri-agent crate

---

## File Structure

| 操作 | 文件 | 职责 |
|------|------|------|
| Modify | `peri-agent/src/agent/executor/mod.rs:234-240` | 在 `execute()` 中记录 prepend 消息 ID 并在循环后清理 |
| Modify | `peri-agent/src/agent/executor/mod_test.rs` | 添加回归测试 |

---

### Task 1: 修复 `execute()` 中 prepend system 消息的累积

**Files:**
- Modify: `peri-agent/src/agent/executor/mod.rs:234-240`

- [ ] **Step 1: 在 `execute()` 中记录 prepend 前的消息数量**

在 `peri-agent/src/agent/executor/mod.rs` 中，找到 `run_before_agent` 调用之前，添加消息数量记录：

```rust
// 文件: peri-agent/src/agent/executor/mod.rs
// 位置: line 234 之前 (self.chain.run_before_agent(state).await?; 之前)

// 记录 prepend 前的消息数量，用于追踪临时 system 消息
let len_before_prepend = state.messages().len();

self.chain.run_before_agent(state).await?;

// 固定 system prompt：在所有中间件 before_agent 之后 prepend
if let Some(ref prompt) = self.system_prompt {
    state.prepend_message(BaseMessage::system(prompt.clone()));
}

// 记录被 prepend 的消息 ID（位于 messages 列表头部）
let prepended_count = state.messages().len() - len_before_prepend;
let prepended_ids: Vec<crate::messages::MessageId> = state
    .messages()
    .iter()
    .take(prepended_count)
    .map(|m| m.id())
    .collect();
```

- [ ] **Step 2: 在 ReAct 循环结束后清理 prepend 的 system 消息**

在同一个文件的 `execute()` 方法中，找到 ReAct 循环结束后的位置（`all_tool_calls` 收集完成后、返回 `Ok(AgentOutput)` 之前），添加清理逻辑：

```rust
// 清理临时 prepend 的 system 消息
// compact 可能已替换所有消息（此时 prepended_ids 中的 ID 不存在，retain 无操作）
// 未发生 compact 时，移除 prepend 的 system 消息防止累积到 history
if !prepended_ids.is_empty() {
    state.messages_mut().retain(|m| !prepended_ids.contains(&m.id()));
}
```

**具体插入位置**：在 `execute()` 方法中，ReAct 循环（`for step in 0..self.max_iterations`）结束后、构建 `AgentOutput` 返回之前。需要先阅读循环结束处的代码找到精确位置。循环可能在 `needs_tool_call()` 的 else 分支 break，也可能在 `all_tool_calls.extend()` 后继续。搜索 `Ok(AgentOutput` 或最终的 `Ok(` 来找到返回点。

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p peri-agent 2>&1 | tail -20`
Expected: 编译成功，无 error

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/executor/mod.rs
git commit -m "fix(agent): 清理 execute() 中 prepend 的临时 system 消息，防止跨轮次累积"
```

---

### Task 2: 添加回归测试——system 消息不累积

**Files:**
- Modify: `peri-agent/src/agent/executor/mod_test.rs`

- [ ] **Step 1: 编写测试验证 prepend system 消息在 execute() 结束后被清理**

在 `peri-agent/src/agent/executor/mod_test.rs` 末尾添加测试：

```rust
/// 回归测试：execute() 中 prepend 的 system 消息（middleware 注入 + with_system_prompt）
/// 必须在 execute() 返回前清理，不应累积到返回的 state.messages 中
#[tokio::test]
async fn test_execute_prepended_system_messages_are_cleaned_up() {
    let mut executor = ReActAgent::new(make_test_llm("done"))
        .with_system_prompt("You are a test assistant.".to_string())
        .max_iterations(1);

    let mut state = TestState::new();
    state.add_message(BaseMessage::human("hello"));

    let _ = executor
        .execute(
            peri_agent::agent::react::AgentInput::text("test"),
            &mut state,
            None,
        )
        .await;

    // execute() 结束后，state.messages 不应包含 with_system_prompt 注入的 system 消息
    let system_count = state.messages().iter().filter(|m| m.is_system()).count();
    assert_eq!(
        system_count, 0,
        "prepend 的 system 消息应在 execute() 返回前被清理，实际有 {} 条 system 消息",
        system_count
    );
}
```

注意：此测试需要检查 `mod_test.rs` 中已有的测试基础设施（`make_test_llm`、`TestState` 等），确保使用一致的 mock 结构。如果 `make_test_llm("done")` 不能让 agent 在 1 轮内结束，需要调整 mock 的行为。

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test -p peri-agent --lib -- test_execute_prepended_system_messages_are_cleaned_up 2>&1 | tail -20`
Expected: 测试通过

- [ ] **Step 3: 编写测试验证多次 execute() 调用 system 消息不累积**

```rust
/// 回归测试：多次调用 execute() 时，system 消息不会跨调用累积
#[tokio::test]
async fn test_execute_no_system_message_accumulation_across_calls() {
    let mut executor = ReActAgent::new(make_test_llm("done"))
        .with_system_prompt("You are a test assistant.".to_string())
        .max_iterations(1);

    let mut state = TestState::new();

    // 第一次 execute
    let _ = executor
        .execute(
            peri_agent::agent::react::AgentInput::text("first"),
            &mut state,
            None,
        )
        .await;
    let msgs_after_first = state.messages().len();

    // 第二次 execute
    let _ = executor
        .execute(
            peri_agent::agent::react::AgentInput::text("second"),
            &mut state,
            None,
        )
        .await;
    let msgs_after_second = state.messages().len();

    // 第二次 execute 新增的消息数应与第一次大致相同（+2：user + assistant）
    // 不应有额外的 system 消息累积
    let system_count = state.messages().iter().filter(|m| m.is_system()).count();
    assert_eq!(
        system_count, 0,
        "多次 execute() 后不应有 system 消息累积，实际有 {} 条",
        system_count
    );
}
```

- [ ] **Step 4: 运行所有 executor 测试验证无回归**

Run: `cargo test -p peri-agent --lib -- executor 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 5: Commit**

```bash
git add peri-agent/src/agent/executor/mod_test.rs
git commit -m "test(agent): 添加 system 消息累积回归测试"
```

---

### Task 3: 端到端验证

**Files:** 无代码修改

- [ ] **Step 1: 运行 peri-acp 单元测试**

Run: `cargo test -p peri-acp 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 2: 运行 peri-middlewares 单元测试（ToolSearchMiddleware 相关）**

Run: `cargo test -p peri-middlewares --lib -- tool_search 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 3: 运行全量测试**

Run: `cargo test 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 4: 手动 smoke test——启动 TUI 进行一轮对话**

Run: `cargo run -p peri-tui`

操作步骤：
1. 输入一条消息让 agent 执行几轮工具调用
2. 输入第二条消息继续对话
3. 打开 LLM 日志（`data/` 目录下最新的请求）
4. 检查 `request.json` 的 `body.system[1].text` 中 "## Deferred Tools" 是否只出现一次
5. 检查 `request.json` 的 `body.messages` 开头是否没有 system 消息

验证点：system prompt 不再逐轮膨胀，"## Deferred Tools" 只出现 1 次。
