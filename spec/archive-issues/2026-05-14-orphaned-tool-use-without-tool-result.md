> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-orphaned-tool-use-without-tool-result.md

# Anthropic API 400: 并发工具调用中部分 tool_result 缺失导致孤儿 tool_use

**状态**：Fixed (2026-05-15 re-fixed: P3 before_tool error path was off-by-one)
**优先级**：高
**创建日期**：2026-05-14

## 问题描述

LLM 在一次 assistant 消息中发起多个并发工具调用（如 Read + Grep + Glob），如果结果处理循环中某条路径（`run_on_error` 传播 或 `run_after_tool` 返回 Err）提前跳出，则只有部分 tool_use 的 tool_result 被写入 state。下一轮 API 请求时 Anthropic 校验不通过，报 400。

Anthropic API 的约束：assistant 消息中的每一个 `tool_use` block 都必须在**紧随其后的 user 消息**中有对应的 `tool_result`——这个"闭合跟随"要求要求所有并发工具的 tool_result 必须同时出现在下一条消息中，不能分批。

## 症状详情

```
LLM HTTP 错误 (400): API 错误 400 Bad Request: messages.33:
`tool_use` ids were found without `tool_result` blocks immediately after:
call_00_JpAjQZBIpNj4qysohdBQ6732.
Each `tool_use` block must have a corresponding `tool_result` block in the next message.
```

### 消息序列对比

**正常并发 3 工具**（AI 消息中 tool_use A/B/C → 下一条 user 消息中 tool_result A/B/C）：

```
assistant: [thinking, text, tool_use A, tool_use B, tool_use C]
user: [tool_result C, tool_result B, tool_result A]  ← 三个闭合，一块到
```

**异常（部分缺失）**：结果处理循环在处理第 2 个工具时提前跳出，导致：

```
assistant: [thinking, text, tool_use A, tool_use B, tool_use C]
user: [tool_result A]  ← 只有 A！B、C 的 tool_result 丢失
→ API 报错：tool_use B 和 C 没有闭合的 tool_result
```

## 根因分析

`peri-agent/src/agent/executor/tool_dispatch.rs`

### 核心问题：结果处理循环中的提前返回

```rust
// 阶段三：串行处理结果（tool_dispatch.rs:152-204）
for (modified_call, tool_result) in modified_calls.into_iter().zip(tool_results) {
    let result = match tool_result {
        Ok(output) => ToolResult::success(&modified_call.id, &modified_call.name, output),
        Err(AgentError::ToolNotFound(ref name)) => {
            ToolResult::error(&modified_call.id, &modified_call.name, ...)
        }
        Err(ref e) => {
            agent.chain.run_on_error(state, e).await?; // ← P3: 传播后跳出
            ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
        }
    };

    agent.emit(AgentEvent::ToolEnd { ... });

    if let Err(e) = agent.chain
        .run_after_tool(state, &modified_call, &result).await
    {
        agent.chain.run_on_error(state, &e).await?;   // ← P4: 传播后跳出
        return Err(e);
    }

    // ← tool_result 消息只在此行写入 state
    state.add_message(tool_msg);
    all_tool_calls.push((modified_call, result));
}
```

**场景**：LLM 返回 3 个 tool_calls → 并发执行 → 结果 1 正常 → 结果 2 触发 `run_on_error` 返回 Err → `?` 传播，`dispatch_tools` 提前返回。此时 state 中只有 tool 1 的 tool_result，tool 2-3 缺失。

`messages_to_anthropic` 将这 3 个 tool_use 序列化到同一条 assistant 消息，但紧随的 user 消息只有 tool 1 的 result → Anthropic API 400。

### 为什么并发场景更容易触发

- 单工具调用：出错即停止，不存在"部分结果"的中间状态
- 多工具并发：5 个工具同时执行，其中任何一个的 P3/P4 路径都会让后续工具的结果被丢弃
- 不仅仅是 Anthropic——DeepSeek 等遵循 Anthropic 消息格式的 provider 也会拒绝

### 次要路径（P1/P2）

P1（cancel 在 approval 循环中触发，第 57-58 行）和 P2（before_tool 错误，第 89-92 行）也会导致**所有** tool_result 缺失。但这些路径触发频率更低（需在 AI 消息写入后但在结果处理前触发取消）。

## 复现条件

- **复现频率**：偶发（取决于工具执行错误和取消时序）
- **触发条件**：LLM 在一次 assistant 消息中发起 **2+ 个工具调用**，且结果处理循环中某条错误传播路径被触发
- **环境**：Anthropic API 或 DeepSeek Anthropic 兼容端口，多工具并发场景

## 修复方向

**推荐方案**：将 P3/P4 的结果处理循环改为"尽最大努力处理所有结果，延迟错误传播"。

```rust
// 阶段三改造：收集所有结果，最后再决定是否报错
let mut deferred_error: Option<AgentError> = None;

for (modified_call, tool_result) in modified_calls.into_iter().zip(tool_results) {
    let result = match tool_result {
        Ok(output) => ToolResult::success(...),
        Err(e) => {
            let _ = agent.chain.run_on_error(state, &e).await;
            deferred_error = deferred_error.or(Some(e));
            ToolResult::error(...)
        }
    };

    // run_after_tool 失败也不传播，改为记录
    if let Err(e) = agent.chain.run_after_tool(state, &modified_call, &result).await {
        let _ = agent.chain.run_on_error(state, &e).await;
        deferred_error = deferred_error.or(Some(e));
    }

    // ← 始终写入 tool_result
    state.add_message(tool_msg);
    agent.emit(AgentEvent::ToolEnd { ... });
    all_tool_calls.push((modified_call, result));
}

// 所有结果写入后，再决定是否报错
if let Some(err) = deferred_error {
    return Err(err);
}
```

关键变更：
1. `run_on_error` 和 `run_after_tool` 失败**不传播**，用 `let _ =` 吞掉，改为收集到 `deferred_error`
2. 无论 `run_on_error`/`run_after_tool` 是否失败，`state.add_message(tool_msg)` 始终执行
3. 循环结束后，如果有错误才返回

**P1/P2 同理**：取消/批准错误后也应补全剩余 tool 的 error tool_result。

## 涉及文件

- `peri-agent/src/agent/executor/tool_dispatch.rs:37` — AI 消息写入
- `peri-agent/src/agent/executor/tool_dispatch.rs:152-204` — **主战场**：结果处理循环
- `peri-agent/src/agent/executor/tool_dispatch.rs:57-58` — P1：取消提前返回
- `peri-agent/src/agent/executor/tool_dispatch.rs:89-92` — P2：before_tool 错误传播
- `peri-agent/src/llm/anthropic.rs:265-300` — Tool 消息→Anthropic user 消息合并逻辑

## 关联 Issue

- `spec/issues/2026-05-14-deepseek-multi-turn-tool-result-duplication.md` — 同模块的相反问题（tool_result 重复而非缺失）
