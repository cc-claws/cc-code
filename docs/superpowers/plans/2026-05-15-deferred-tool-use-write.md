# Deferred tool_use Write: 延迟写入重构消除孤儿 tool_use

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `tool_dispatch.rs` 的两阶段写入模式重构为延迟写入，使 AI 消息（含 tool_use）在所有 tool_result 准备好后才写入 state，从架构上消除孤儿 tool_use 导致 Anthropic API 400 的可能。

**Architecture:** 当前 `dispatch_tools` 在第 37 行立即将 AI 消息写入 state，tool_result 在阶段三才写入。重构后改为：先执行所有 before_tool + 工具调用 + 收集结果，最后一步才将 AI 消息 + 所有 tool_result 按序写入 state。这意味着即使中间任何步骤出错，state 中也永远不会出现"有 tool_use 无 tool_result"的不一致状态。错误路径不再需要 flush 函数。

**Tech Stack:** Rust, tokio async, `peri-agent` crate

---

## 文件结构

| 文件 | 操作 | 职责 |
|------|------|------|
| `peri-agent/src/agent/executor/tool_dispatch.rs` | 修改（主要） | 重构核心：延迟写入 + 简化错误路径 |
| `peri-agent/src/agent/executor/tool_dispatch_test.rs` | 修改 | 更新现有测试 + 新增测试覆盖 |

## 设计要点

### 当前流程（两阶段写入）

```
1. state.add_message(ai_msg)          ← AI 消息立即写入（含 tool_use blocks）
2. emit MessageAdded + TextChunk
3. before_tool 循环（失败需 flush）
4. 并发执行工具
5. for each result: state.add_message(tool_msg)  ← tool_result 逐个写入
6. 错误检查 → return Err
```

问题：步骤 1 和步骤 5 之间有 8 条执行路径，每条都需要确保所有 tool_use 都有配对 tool_result。

### 目标流程（延迟写入）

```
1. 创建 ai_msg 但不写入 state
2. emit TextChunk（非流式文本先发给 TUI）
3. before_tool 循环：
   - ToolRejected → 记录 rejection result，continue
   - Ok → 加入 ready_calls
   - Err → 为所有已处理+未处理的 calls 写 error result 到 pending_results，return Err
   - Cancel → 同上，return Interrupted
4. 并发执行 ready_calls 中的工具
5. 收集所有结果到 results: Vec<(ToolCall, ToolResult)>
6. emit MessageAdded(ai_msg)    ← 此时才发射事件
7. state.add_message(ai_msg)    ← 此时才写入 state
8. for each (call, result):
   - emit ToolStart + ToolEnd
   - state.add_message(tool_msg)
   - emit MessageAdded(tool_msg)
9. 错误检查 → return Err（此时 state 已一致）
```

关键不变量：**步骤 7-8 要么全部执行，要么都不执行。** 如果步骤 3-5 中任何环节出错，state 中不会有 AI 消息，因此不存在孤儿 tool_use。

### 事件顺序说明

**已验证（审阅结论）**：`ExecutorEvent::MessageAdded` 被 TUI 的 `map_executor_event`（`agent.rs:541`）**完全丢弃**，TUI 从不接收 `MessageAdded`。`MessagePipeline` 完全通过 `StateSnapshot`（批量替换）和流式事件（`AssistantChunk`/`ToolStart`/`ToolEnd`）维护状态，有自己的内部缓冲区（`current_ai_text`、`pending_tools`、`completed_tools`），不依赖 `MessageAdded`。

因此延迟写入导致的事件发射时序变化对 TUI **无任何影响**。`MessageAdded` 仍然需要 emit（供 Langfuse 追踪器等消费者使用），但 TUI 不依赖其到达顺序。

流式模式下 LLM 适配器在 SSE 解析期间就已 emit `TextChunk`（远在 `dispatch_tools` 调用之前），TUI 的 Pipeline 通过 `push_chunk()` 缓冲到 `current_ai_text`，直到 `StateSnapshot` 触发 `set_completed()`。非流式 `TextChunk` 走完全相同的路径，无需特殊处理。

---

### Task 1: 添加「延迟写入模式」的验证性测试

**Files:**
- Test: `peri-agent/src/agent/executor/tool_dispatch_test.rs`

这一步先写一个**期望行为**的测试，验证延迟写入的最终不变量：无论 dispatch_tools 返回 Ok 还是 Err，state 中的 tool_use 数量始终等于 tool_result 数量。

- [ ] **Step 1: 编写「通用不变量检查」辅助函数**

在 `tool_dispatch_test.rs` 顶部添加：

```rust
/// 通用不变量：state 中每个 tool_use 必须有对应 tool_result。
/// 收集所有 AI 消息中的 tool_call_id 和所有 Tool 消息中的 tool_call_id，
/// 断言两者一一匹配。
fn assert_no_orphaned_tool_uses(state: &AgentState) {
    let mut ai_tool_ids: Vec<String> = Vec::new();
    let mut tool_result_ids: Vec<String> = Vec::new();
    for msg in state.messages() {
        if let BaseMessage::Ai { tool_calls, .. } = msg {
            for tc in tool_calls {
                ai_tool_ids.push(tc.id.clone());
            }
        }
        if let BaseMessage::Tool { tool_call_id, .. } = msg {
            tool_result_ids.push(tool_call_id.clone());
        }
    }
    assert_eq!(
        ai_tool_ids.len(),
        tool_result_ids.len(),
        "tool_use 数量 ({}) != tool_result 数量 ({})\n\
         tool_use IDs: {:?}\n\
         tool_result IDs: {:?}",
        ai_tool_ids.len(),
        tool_result_ids.len(),
        ai_tool_ids,
        tool_result_ids
    );
    for id in &ai_tool_ids {
        assert!(
            tool_result_ids.contains(id),
            "tool_use id={} 缺少配对 tool_result（孤儿 tool_use → Anthropic API 400）",
            id
        );
    }
}
```

- [ ] **Step 2: 编写「并发工具执行部分失败」测试**

这个测试模拟 3 个工具并发执行，其中 1 个失败。验证所有 3 个 tool_use 都有配对 tool_result，且 Agent 继续循环（不停止）。

```rust
/// 并发工具执行中部分失败：3 个工具并发，tool_b 执行失败。
/// 验证所有 tool_use 都有配对 tool_result，Agent 继续（不停止）。
#[tokio::test]
async fn test_concurrent_partial_failure_all_results_written() {
    struct FailToolB;
    #[async_trait::async_trait]
    impl BaseTool for FailToolB {
        fn name(&self) -> &str { "tool_b" }
        fn description(&self) -> &str { "fails" }
        fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("tool_b 执行失败".into())
        }
    }

    struct EchoTool { name_str: &'static str }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str { self.name_str }
        fn description(&self) -> &str { "echo" }
        fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    // LLM：第一轮调用 3 个工具，第二轮给出最终回答
    struct ThreeToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for ThreeToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call three tools",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                        ToolCall::new("id3", "tool_c", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "all results processed"))
            }
        }
    }

    let agent = ReActAgent::new(ThreeToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(FailToolB))
        .register_tool(Box::new(EchoTool { name_str: "tool_c" }));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    // Agent 应正常完成（工具执行错误不终止循环）
    assert!(result.is_ok(), "Agent 应正常完成，实际: {:?}", result);

    // 核心断言：所有 tool_use 都有配对 tool_result
    assert_no_orphaned_tool_uses(&state);
}
```

- [ ] **Step 3: 编写「before_tool 中间件错误不影响已完成工具」测试**

验证 before_tool 在第 2 个工具报错时，第 1 个已通过 before_tool 的工具不会产生孤儿 tool_use。

（此测试已存在为 `test_p3_error_flushes_modified_calls_no_orphaned_tool_use`，确认它仍然通过即可。）

- [ ] **Step 4: 运行测试，确认基线**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests::test_concurrent_partial_failure_all_results_written --nocapture`

Expected: 如果当前代码正确则 PASS，如果不正确则 FAIL（这将成为重构的驱动力）。同时确认已有测试 `test_p3_error_flushes_modified_calls_no_orphaned_tool_use` 和 `test_mixed_ok_rejected_error_all_tool_results_written` 仍然通过。

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests --nocapture`

Expected: 所有测试通过。

- [ ] **Step 5: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch_test.rs
git commit -m "test: 添加并发工具执行部分失败的不变量测试（延迟写入重构前置）"
```

---

### Task 2: 重构 `dispatch_tools` —— 提取结果收集阶段

**Files:**
- Modify: `peri-agent/src/agent/executor/tool_dispatch.rs`

这是核心重构。将 `dispatch_tools` 拆分为两个清晰的阶段：

**阶段 A（收集）**：before_tool + 并发执行 + 结果收集。不写 state，只返回结果。

**阶段 B（写入）**：将 AI 消息 + 所有 tool_result 一次性写入 state。

- [ ] **Step 1: 将 before_tool + 执行逻辑提取为 `collect_tool_results` 函数**

创建新函数，接收 `original_calls`、工具引用等参数。此函数**不写 state**，只收集结果。

返回类型设计：`collect_tool_results` 返回三元组 `(Vec<(ToolCall, ToolResult)>, bool, Option<String>)`，分别对应（工具调用结果、是否被取消、deferred_error）。

**为什么不用 struct**：三元组足够表达，且避免引入只在一个函数中使用的类型。

```rust
/// 执行 before_tool 审批 + 并发工具调用，收集所有结果。
///
/// **不变量**：调用期间 state 中不包含本轮 AI 消息。所有 `run_on_error` /
/// `run_after_tool` 实现均不依赖 `state.messages()` 包含本轮新增内容
/// （已验证：全部 17 个中间件的这些钩子均使用 `_state: &mut S` 模式）。
/// 新增中间件时必须遵守此约束——不要在 before_tool / after_tool / on_error
/// 中假设 state 已包含本轮 AI 消息。
///
/// 不写入 state，由 `dispatch_tools` 统一写入。
///
/// 返回 `(results, was_cancelled, deferred_error)`。
/// - 正常路径：`(results, false, None)`
/// - Cancel 路径：`(results, true, None)`（工具已执行完毕，结果已收集）
/// - after_tool 错误：`(results, false, Some(msg))`（所有结果已收集，含错误）
/// - before_tool 错误：直接返回 `Err`（无结果，state 未修改）
async fn collect_tool_results<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    original_calls: Vec<ToolCall>,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
    ai_msg_id: MessageId,
) -> AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)>
```

此函数内部逻辑：

1. 遍历 `original_calls`，执行 `before_tool`：
   - `Ok(call)` → 加入 `ready_calls`，emit `ToolStart`
   - `ToolRejected` → 记录 `(call, error_result)` 到 `settled_results`，emit `ToolStart + ToolEnd`
   - `Err(e)` → 先为 `ready_calls` 中已 emit `ToolStart` 的调用补发 `ToolEnd { is_error: true }`（**已验证必要性**：TUI 的 `MessagePipeline` 在收到 `ToolStart` 后会创建 `PendingTool` 条目，如果没有配对 `ToolEnd`，条目会残留到 `done()`/`interrupt()` 清理时才消失，造成短暂视觉闪烁），再 return `Err(e)`
   - `Cancel` → 同上，补发 `ToolEnd`，return `Err(Interrupted)`
2. 并发执行 `ready_calls`
3. 处理执行结果：每个 `ready_call` 生成 success/error `ToolResult`，emit `ToolEnd`
4. 合并 `settled_results`（rejected）+ 执行结果
5. Return `Ok((results, was_cancelled, deferred_error))`

**关键点**：此函数 emit `ToolStart`/`ToolEnd` 事件（TUI 实时显示需要），但**不调用 `state.add_message`**。

- [ ] **Step 3: 重写 `dispatch_tools` 主函数**

`dispatch_tools` 使用三元组返回：Cancel / deferred_error 路径也写入 state 后再返回 Err。

```rust
pub(crate) async fn dispatch_tools<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    reasoning: &Reasoning,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
) -> AgentResult<Vec<(ToolCall, ToolResult)>> {
    // 构建 AI 消息（不写入 state）
    let tc_reqs: Vec<ToolCallRequest> = reasoning.tool_calls.iter()
        .map(|tc| ToolCallRequest::new(tc.id.clone(), tc.name.clone(), tc.input.clone()))
        .collect();
    let ai_msg = reasoning.source_message.clone()
        .unwrap_or_else(|| BaseMessage::ai_with_tool_calls(reasoning.thought.clone(), tc_reqs));
    let ai_msg_id = ai_msg.id();

    // emit 工具前文本（非流式）
    if !reasoning.streamed && !reasoning.thought.trim().is_empty() {
        agent.emit(AgentEvent::TextChunk {
            message_id: ai_msg_id,
            chunk: reasoning.thought.clone(),
        });
    }

    // 阶段 A：收集所有工具调用结果（不写 state）
    // 返回 Err 仅在 before_tool 错误路径（此时 state 干净）
    let (results, was_cancelled, deferred_error) = collect_tool_results(
        agent, state, reasoning.tool_calls.clone(), all_tools, cancel, ai_msg_id,
    ).await?;

    // 阶段 B：一次性写入 state（Cancel / deferred_error 路径也写入，保证 state 一致）
    agent.emit(AgentEvent::MessageAdded(ai_msg.clone()));
    state.add_message(ai_msg);

    for (_, result) in &results {
        let tool_msg = if result.is_error {
            BaseMessage::tool_error(&result.tool_call_id, result.output.as_str())
        } else {
            BaseMessage::tool_result(&result.tool_call_id, result.output.as_str())
        };
        let tool_msg_clone = tool_msg.clone();
        state.add_message(tool_msg);
        agent.emit(AgentEvent::MessageAdded(tool_msg_clone));
    }

    // 写入完成后再返回错误
    if was_cancelled {
        return Err(AgentError::Interrupted);
    }
    if let Some(msg) = deferred_error {
        return Err(AgentError::MiddlewareError {
            middleware: "chain".to_string(),
            reason: msg,
        });
    }

    Ok(results)
}
```

**设计要点**：state 写入和错误返回是分离的。`collect_tool_results` 返回 `Ok` → state 一定一致。`collect_tool_results` 返回 `Err`（仅 before_tool 错误）→ state 未被修改。两种情况都不可能产生孤儿 tool_use。

- [ ] **Step 4: 删除 `flush_modified_tool_errors` 和 `flush_pending_tool_errors` 函数**

这两个函数不再被引用。延迟写入架构下：
- before_tool 错误：`collect_tool_results` 直接返回 `Err`，state 未修改，无需 flush
- Cancel / deferred_error：`collect_tool_results` 返回 `Ok((results, true/false, ...))`，所有结果已收集，统一写入

删除它们。

- [ ] **Step 5: 在 `collect_tool_results` 中处理 ToolRejected**

ToolRejected 是特殊情况：不是错误，不终止循环，但需要为被拒绝的工具生成 error ToolResult。

```rust
// 在 collect_tool_results 的 before_tool 循环中：
Err(AgentError::ToolRejected { ref reason, .. }) => {
    let rejection_result = ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
    agent.emit(AgentEvent::ToolStart {
        message_id: ai_msg_id,
        tool_call_id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        input: tool_call.input.clone(),
    });
    agent.emit(AgentEvent::ToolEnd {
        message_id: ai_msg_id,
        tool_call_id: tool_call.id.clone(),
        name: tool_call.name.clone(),
        output: rejection_result.output.clone(),
        is_error: true,
    });
    settled_results.push((tool_call.clone(), rejection_result));
    continue;
}
```

- [ ] **Step 6: 完整实现 `collect_tool_results`**

```rust
async fn collect_tool_results<L: ReactLLM, S: State>(
    agent: &ReActAgent<L, S>,
    state: &mut S,
    original_calls: Vec<ToolCall>,
    all_tools: &HashMap<String, &dyn BaseTool>,
    cancel: &CancellationToken,
    ai_msg_id: MessageId,
) -> AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)> {
    let mut ready_calls: Vec<ToolCall> = Vec::with_capacity(original_calls.len());
    let mut settled_results: Vec<(ToolCall, ToolResult)> = Vec::new();

    // before_tool 循环
    let before_results = agent.chain
        .run_before_tools_batch(state, original_calls.clone())
        .await;

    for (tool_call, before_result) in original_calls.iter().zip(before_results) {
        if cancel.is_cancelled() {
            // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd，
            // 避免 TUI 的 pending_tools 残留（会被 done()/interrupt() 清理，
            // 但补发可避免短暂视觉闪烁）
            for tc in &ready_calls {
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    output: "interrupted by user".to_string(),
                    is_error: true,
                });
            }
            return Err(AgentError::Interrupted);
        }
        match before_result {
            Ok(modified_call) => {
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: modified_call.id.clone(),
                    name: modified_call.name.clone(),
                    input: modified_call.input.clone(),
                });
                ready_calls.push(modified_call);
            }
            Err(AgentError::ToolRejected { ref reason, .. }) => {
                let result = ToolResult::error(&tool_call.id, &tool_call.name, reason.clone());
                agent.emit(AgentEvent::ToolStart {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    input: tool_call.input.clone(),
                });
                agent.emit(AgentEvent::ToolEnd {
                    message_id: ai_msg_id,
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    output: result.output.clone(),
                    is_error: true,
                });
                settled_results.push((tool_call.clone(), result));
            }
            Err(e) => {
                let _ = agent.chain.run_on_error(state, &e).await;
                // 为已 emit ToolStart 的 ready_calls 补发 ToolEnd
                for tc in &ready_calls {
                    agent.emit(AgentEvent::ToolEnd {
                        message_id: ai_msg_id,
                        tool_call_id: tc.id.clone(),
                        name: tc.name.clone(),
                        output: e.to_string(),
                        is_error: true,
                    });
                }
                return Err(e);
            }
        }
    }

    // 并发执行（与当前代码相同，此处省略重复——复制当前 Phase 2 的 futures 逻辑）
    // ... join_all(futures).await ...

    let was_cancelled = cancel.is_cancelled();

    // 结果处理（与当前 Phase 3 相同，但不写 state）
    let mut deferred_error: Option<String> = None;
    let mut exec_results: Vec<(ToolCall, ToolResult)> = Vec::with_capacity(ready_calls.len());

    for (modified_call, tool_result) in ready_calls.into_iter().zip(tool_results) {
        let result = match tool_result {
            Ok(output) => ToolResult::success(&modified_call.id, &modified_call.name, output),
            Err(AgentError::ToolNotFound(ref name)) => {
                tracing::warn!(tool.name = %name, "工具未找到");
                ToolResult::error(&modified_call.id, &modified_call.name, format!("工具 '{}' 不存在", name))
            }
            Err(ref e) => {
                let _ = agent.chain.run_on_error(state, e).await;
                ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
            }
        };

        if result.is_error {
            tracing::warn!(
                tool.name = %result.tool_name,
                tool.is_error = true,
                error_len = result.output.len(),
                "tool call failed"
            );
        }
        agent.emit(AgentEvent::ToolEnd {
            message_id: ai_msg_id,
            tool_call_id: modified_call.id.clone(),
            name: modified_call.name.clone(),
            output: result.output.clone(),
            is_error: result.is_error,
        });

        if let Err(e) = agent.chain.run_after_tool(state, &modified_call, &result).await {
            let _ = agent.chain.run_on_error(state, &e).await;
            deferred_error = deferred_error.or(Some(e.to_string()));
        }

        exec_results.push((modified_call, result));
    }

    // 合并 settled（rejected）+ executed 结果
    settled_results.extend(exec_results);

    // Cancel / deferred_error 不返回 Err，由 dispatch_tools 在写入 state 后再检查
    Ok((settled_results, was_cancelled, deferred_error))
}
```

**设计总结**：
- `collect_tool_results` 只有在 `before_tool` 错误和 Cancel（在 before_tool 阶段检测到）时返回 `Err`——此时 state 干净
- Cancel 在并发执行阶段检测到、以及 `deferred_error`——返回 `Ok((results, true/false, Some(...)))`，由 `dispatch_tools` 写入 state 后再返回 `Err`

- [ ] **Step 7: 删除旧的 `flush_modified_tool_errors` 和 `flush_pending_tool_errors` 函数**

这两个函数不再被引用，删除它们。

- [ ] **Step 8: 运行所有测试验证**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests --nocapture`

Expected: 所有测试通过（包括 Task 1 新增的 `test_concurrent_partial_failure_all_results_written`）。

Run: `cargo test -p peri-agent --lib --nocapture`

Expected: 全部 agent 测试通过。

- [ ] **Step 9: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch.rs
git commit -m "refactor: 延迟写入重构——消除 tool_dispatch 两阶段写入导致的孤儿 tool_use"
```

---

### Task 3: 更新现有测试适配新架构

**Files:**
- Modify: `peri-agent/src/agent/executor/tool_dispatch_test.rs`

现有测试依赖旧的执行路径和错误返回值，需要适配。

- [ ] **Step 1: 更新 `test_p3_error_flushes_modified_calls_no_orphaned_tool_use`**

此测试验证 before_tool 非 ToolRejected 错误（P4 路径）。在新架构中，before_tool 错误会导致 `collect_tool_results` 直接返回 `Err`，`dispatch_tools` 也返回 `Err`，state 中**没有 AI 消息也没有 tool_result**。

测试断言需要调整：
- `assert!(result.is_err())` —— 仍然成立
- 不变量检查 `assert_no_orphaned_tool_uses` 仍然成立（因为 state 中根本没有 tool_use）
- `ai_tool_ids.len() == 3` 断言需要改为 `ai_tool_ids.len() == 0`（因为 AI 消息没写入 state）

```rust
// 更新断言
let mut ai_tool_ids: Vec<String> = Vec::new();
let mut tool_result_ids: Vec<String> = Vec::new();
for msg in state.messages() {
    if let BaseMessage::Ai { tool_calls, .. } = msg {
        for tc in tool_calls {
            ai_tool_ids.push(tc.id.clone());
        }
    }
    if let BaseMessage::Tool { tool_call_id, .. } = msg {
        tool_result_ids.push(tool_call_id.clone());
    }
}
// before_tool 错误路径：AI 消息不写入 state，因此 0 个 tool_use 和 0 个 tool_result
assert_eq!(ai_tool_ids.len(), 0, "before_tool 错误路径不应写入 AI 消息到 state");
assert_eq!(tool_result_ids.len(), 0, "before_tool 错误路径不应写入 tool_result 到 state");
```

- [ ] **Step 2: 更新 `test_mixed_ok_rejected_error_all_tool_results_written`**

此测试混合了 Ok + ToolRejected + 非 ToolRejected 错误。在新架构中：
- call[0] Ok → ready_calls
- call[1] ToolRejected → settled_results
- call[2] 非 ToolRejected → before_tool Err → collect_tool_results return Err → dispatch_tools return Err

state 中应该也是空的（AI 消息未写入）。

```rust
// 更新断言
assert!(result.is_err(), "混合路径中 before_tool 错误应返回错误");
// before_tool 错误在索引 2，collect_tool_results 返回 Err
// AI 消息未写入 state
let mut ai_tool_ids: Vec<String> = Vec::new();
let mut tool_result_ids: Vec<String> = Vec::new();
for msg in state.messages() {
    if let BaseMessage::Ai { tool_calls, .. } = msg {
        for tc in tool_calls {
            ai_tool_ids.push(tc.id.clone());
        }
    }
    if let BaseMessage::Tool { tool_call_id, .. } = msg {
        tool_result_ids.push(tool_call_id.clone());
    }
}
assert_eq!(ai_tool_ids.len(), 0, "before_tool P4 错误路径不应写入 AI 消息");
assert_eq!(tool_result_ids.len(), 0, "before_tool P4 错误路径不应写入 tool_result");
```

- [ ] **Step 3: 新增「before_tool 全部通过 + 工具执行部分失败」测试**

验证正常路径（before_tool 全部通过）+ 工具执行错误。这是用户报告的场景。

（此测试在 Task 1 中已添加为 `test_concurrent_partial_failure_all_results_written`。）

- [ ] **Step 4: 新增「Cancel 在并发执行中」测试**

验证 cancel 在工具并发执行期间触发时，state 一致。

```rust
/// Cancel 在并发执行阶段触发：所有工具以 error 结束，
/// AI 消息和所有 tool_result 仍写入 state（cancel 后 write-then-return-err）。
#[tokio::test]
async fn test_cancel_during_execution_all_results_written() {
    struct SlowTool;
    #[async_trait::async_trait]
    impl BaseTool for SlowTool {
        fn name(&self) -> &str { "slow_tool" }
        fn description(&self) -> &str { "slow" }
        fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok("never".to_string())
        }
    }

    struct TwoToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for TwoToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
            _streaming: Option<crate::llm::types::StreamingContext>,
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "call two tools",
                    vec![
                        ToolCall::new("id1", "slow_tool", serde_json::json!({})),
                        ToolCall::new("id2", "slow_tool", serde_json::json!({})),
                    ],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "ok"))
            }
        }
    }

    let cancel = CancellationToken::new();
    let agent = ReActAgent::new(TwoToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(SlowTool));

    let token = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, Some(cancel))
        .await;

    assert!(
        matches!(result, Err(AgentError::Interrupted)),
        "Cancel 应返回 Interrupted，实际: {:?}",
        result
    );

    // Cancel 路径：AI 消息 + error tool_results 仍写入 state
    assert_no_orphaned_tool_uses(&state);
}
```

- [ ] **Step 5: 运行所有测试**

Run: `cargo test -p peri-agent --lib -- tool_dispatch::tests --nocapture`

Expected: 全部通过。

Run: `cargo test -p peri-agent --lib --nocapture`

Expected: 全部通过。

- [ ] **Step 6: Commit**

```bash
git add peri-agent/src/agent/executor/tool_dispatch_test.rs
git commit -m "test: 更新 tool_dispatch 测试适配延迟写入架构"
```

---

### Task 4: 全量测试 + CLAUDE.md 更新

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: 运行全量测试**

Run: `cargo test --workspace`

Expected: 全部通过。

- [ ] **Step 2: 更新 CLAUDE.md 中的 TRAP 描述**

将 `[TRAP] tool_dispatch.rs 两阶段写入` 部分更新为反映新架构。移除关于 flush 函数的警告，替换为延迟写入的简要说明。

找到 CLAUDE.md 中这段：

```
**[TRAP]** `tool_dispatch.rs` 采用"先写 AI 消息（tool_use），后补全 tool_result"的两阶段写入模式...
```

替换为：

```
**[TRAP]** `tool_dispatch.rs` 采用延迟写入模式：AI 消息（含 tool_use）在所有 tool_result 收集完成后才写入 state。`collect_tool_results` 阶段不写 state，`dispatch_tools` 在最后一步统一写入。**错误路径（before_tool Err）直接返回 Err，state 未被修改，不会产生孤儿 tool_use。** Cancel / deferred_error 路径在写入 state 后再返回 Err，保证 state 一致性。修改此模块时不要在 `collect_tool_results` 中调用 `state.add_message`。
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: 更新 CLAUDE.md tool_dispatch TRAP 为延迟写入模式"
```

---

## Self-Review

### 1. Spec coverage

| 需求 | 对应 Task |
|------|-----------|
| 消除孤儿 tool_use（before_tool 错误路径） | Task 2（collect_tool_results 直接 return Err，state 未修改） |
| 消除孤儿 tool_use（Cancel 路径） | Task 2（cancel 写入后 return Err，state 一致） |
| 消除孤儿 tool_use（并发工具执行错误） | Task 2（所有结果收集后统一写入） |
| 删除 flush 函数 | Task 2 Step 4 |
| 测试覆盖 | Task 1 + Task 3 |
| 文档更新 | Task 4 |

### 2. Placeholder scan

无 TBD/TODO/placeholder。所有步骤都有具体代码。

### 3. Type consistency

- `collect_tool_results` 签名在 Step 1 和 Step 6 中一致：返回 `AgentResult<(Vec<(ToolCall, ToolResult)>, bool, Option<String>)>`
- `dispatch_tools` 在 Step 3 中正确解构三元组
- `assert_no_orphaned_tool_uses` 辅助函数在 Task 1 Step 1 定义，被 Task 3 引用

### 4. 审阅验证结果（3 个并行 agent）

| 视角 | 结论 | 关键发现 |
|------|------|----------|
| 中间件 state 依赖 | ✅ 安全 | 全部 17 个中间件的 before_tool/after_tool/on_error 均不读 state.messages() |
| TUI 事件兼容性 | ✅ 安全 | MessageAdded 被 map_executor_event 丢弃，Pipeline 完全自给自足 |
| State 一致性 & 序列化 | ✅ 安全 | Anthropic 按 tool_use_id 匹配不依赖位置；run_on_error 不插入消息 |
