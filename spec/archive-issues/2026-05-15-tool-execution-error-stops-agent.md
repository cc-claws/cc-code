> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-tool-execution-error-stops-agent.md

# 工具调用参数错误（如 Read - Missing file_path）导致 Agent 停止而非自动重试

**状态**：Fixed — 部分修复：ToolNotFound/ToolExecutionFailed 已不再设 deferred_error，但 after_tool 中间件错误（run_after_tool 返回 Err）仍设 deferred_error 导致 Agent 停止。
**优先级**：高
**创建日期**：2026-05-15

## 问题描述

当 LLM 发起工具调用但参数不完整（如 `Read` 缺少 `file_path` 参数），工具执行返回错误。期望行为是 Agent 将错误 ToolResult 反馈给 LLM 继续下一轮循环（LLM 自行修正参数），但实际行为是 Agent 以 `MiddlewareError` 直接停止，不再继续。

## 症状详情

```
MiddlewareError { middleware: "chain", reason: "Tool execution failed: Read - Missing file_path parameter" }
```

Agent 在工具调用失败后直接终止，用户需手动重新提问。LLM 没有机会看到错误信息并自行修正参数。

### 控制流分析

```
dispatch_tools() 阶段三结果处理循环（tool_dispatch.rs:175-247）
  └─ tool_result = Err(ToolExecutionFailed)
     └─ run_on_error（吞掉）
     └─ deferred_error = Some(...)     ← 问题：工具执行错误被当作 MiddlewareError
     └─ ToolResult::error → 写入 state
  └─ [其他工具结果写入完成]
  └─ if deferred_error.is_some() → return Err(MiddlewareError)  ← 循环终止
```

## 根因（原分析，代码已部分修复）

`peri-agent/src/agent/executor/tool_dispatch.rs` 阶段三结果处理循环

~~原问题代码（已修复）~~：

```rust
Err(ref e) => {
    let _ = agent.chain.run_on_error(state, e).await;
    deferred_error = deferred_error.or(Some(e.to_string())); // 已移除
    ToolResult::error(&modified_call.id, &modified_call.name, e.to_string())
}
```

**截至 2026-05-15**：ToolNotFound（行 179-186）和 ToolExecutionFailed（行 188-192）**已不再设置 deferred_error**，错误 ToolResult 正常写入，Agent 继续循环。✅

**仍残留**：after_tool 中间件错误（行 211-218）仍设 deferred_error：

```rust
if let Err(e) = agent.chain.run_after_tool(state, &modified_call, &result).await {
    let _ = agent.chain.run_on_error(state, &e).await;
    deferred_error = deferred_error.or(Some(e.to_string())); // ← 仍存在
}
```

循环结束后 deferred_error 为 Some 时返回 MiddlewareError 终止 Agent（行 242-247）。

## 期望行为

| 错误来源 | 当前行为 | 期望行为 |
|----------|----------|----------|
| 工具执行失败（`ToolExecutionFailed`） | ✅ 仅创建 error ToolResult → 继续循环 | 已修复 |
| 工具不存在（`ToolNotFound`） | ✅ 仅创建 error ToolResult → 继续循环 | 已修复 |
| after_tool 中间件错误（`run_after_tool` 返回 Err） | ⚠ 设置 deferred_error → 停止循环 | 仅创建 error ToolResult → 继续循环 |

## 涉及文件

- `peri-agent/src/agent/executor/tool_dispatch.rs:178-181` —— ToolNotFound 错误处理
- `peri-agent/src/agent/executor/tool_dispatch.rs:187-191` —— ToolExecutionFailed 错误处理
- `peri-agent/src/agent/executor/tool_dispatch.rs:211-218` —— after_tool 中间件错误处理
- `peri-agent/src/agent/executor/tool_dispatch.rs:242-247` —— deferred_error 终止点
