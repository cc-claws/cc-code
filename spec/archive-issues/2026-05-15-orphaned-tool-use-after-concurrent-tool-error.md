> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-orphaned-tool-use-after-concurrent-tool-error.md

# stop_reason 与内容不一致导致孤儿 tool_use 触发 Anthropic API 400

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-15
**修复日期**：2026-05-15

## 问题描述

第三方 LLM provider（如 DeepSeek）通过 Anthropic 兼容端口返回 `stop_reason: end_turn`，但响应内容实际包含 `tool_use` blocks。Anthropic 适配器的 `generate_reasoning` 仅根据 `stop_reason` 判断路由，将含 `tool_use` 的 `source_message` 走最终回答路径写入 state，导致无配对 `tool_result`。下一轮 API 请求触发 400 错误。

## 症状详情

```
LLM HTTP 错误 (400): API 错误 400 Bad Request: messages.15: `tool_use` ids were found
without `tool_result` blocks immediately after: call_00_LahGsr8aIx8ZhtqTqOPW3747.
Each `tool_use` block must have a corresponding `tool_result` block in the next message.
```

### 根因分析

DeepSeek API 的 `stop_reason` 字段与响应内容不一致：

- API 返回 `stop_reason: "end_turn"`（而非 `"tool_use"`）
- 但响应 content 中包含 `{"type": "tool_use", ...}` blocks
- `StopReason::from_anthropic("end_turn")` → `StopReason::EndTurn`
- `generate_reasoning` 的 `if response.stop_reason == StopReason::ToolUse` 判断为 false
- 进入 `else` 分支：`Reasoning::with_answer()` + `source_message = Some(response.message)`
- `handle_final_answer` 将含 `tool_use` blocks 的 `source_message` 直接写入 state
- 无配对 `tool_result` → 下一轮 API 400

### 代码路径

```
DeepSeek 返回 stop_reason=end_turn + tool_use content
  → StopReason::from_anthropic("end_turn") = EndTurn
    → generate_reasoning: else 分支
      → Reasoning::with_answer() (tool_calls 为空)
        → needs_tool_call() = false
          → handle_final_answer
            → state.add_message(source_message)  // 含 tool_use，无 tool_result
              → 下一轮 API 400
```

### 复现条件

- **复现频率**：偶发（取决于 DeepSeek 的 stop_reason 返回行为）
- **触发步骤**：
  1. 使用 DeepSeek API（Anthropic 兼容端口）
  2. LLM 返回包含 tool_use blocks 的响应
  3. DeepSeek 返回 `stop_reason: end_turn`（而非预期的 `tool_use`）
  4. Agent 将含 tool_use 的消息写入 state 但无 tool_result
  5. 下一轮 API 请求 → 400
- **环境**：DeepSeek API（api.deepseek.com），Anthropic 兼容模式

## 修复记录

### 修复一：延迟写入重构（架构防御）

| 提交 | 内容 |
|------|------|
| 3e18700 | `tool_dispatch.rs` 从两阶段写入重构为延迟写入：`collect_tool_results` 先收集所有结果不写 state，`dispatch_tools` 最后统一写入 AI 消息 + 所有 tool_result |

此修复消除了 `tool_dispatch.rs` 中的架构脆弱性（4 次因此 bug 修复），但**不是本次 400 错误的直接原因**。

### 修复二：stop_reason 内容一致性检查（根因修复）

| 提交 | 内容 |
|------|------|
| d10dd40 | `anthropic/invoke.rs`：`generate_reasoning` 的 else 分支增加 `has_tool_calls()` 检查，当 `stop_reason != ToolUse` 但内容含 tool_use 时走工具调用路径 |
| bd8d4c7 | `react_adapter.rs`：`BaseModelReactLLM` 的 `generate_reasoning` 同样增加 `has_tool_calls()` 防御 + 新增 2 个单元测试 |

修复逻辑：将原来的 `else { ... }` 拆分为 `else if response.message.has_tool_calls() { ... } else { ... }`，在 `stop_reason` 不一致时仍能正确路由到工具调用路径，同时打 `tracing::warn` 日志。

## 历史修复记录

| 提交 | 修复内容 |
|------|----------|
| f138b21 | tool_dispatch flush 路径修复 |
| 7f3ad00 | tool_dispatch flush 路径修复 |
| 8d6bb1b | tool_dispatch flush 路径修复 |
| 3e18700 | tool_dispatch 延迟写入重构（架构防御） |
| d10dd40 | anthropic adapter stop_reason 一致性检查（根因修复） |
| bd8d4c7 | react_adapter stop_reason 一致性检查 + 测试 |

## 涉及文件

- `peri-agent/src/llm/anthropic/invoke.rs` — Anthropic 适配器 `generate_reasoning` 的 else 分支
- `peri-agent/src/llm/react_adapter.rs` — 通用 `BaseModelReactLLM` 的 `generate_reasoning` 的 else 分支
- `peri-agent/src/agent/executor/tool_dispatch.rs` — 延迟写入重构
- `peri-agent/src/agent/executor/final_answer.rs` — `handle_final_answer` 写入 `source_message`

## 关联 Issue

- `spec/issues/2026-05-15-tool-execution-error-stops-agent.md`（Fixed）—— after_tool 中间件错误设 deferred_error 导致 Agent 停止，与孤儿 tool_use 是同一模块的不同错误路径
- `spec/issues/2026-05-14-deepseek-multi-turn-tool-result-duplication.md`（Fixed）—— 不同根因（StateSnapshot 重复）但同样导致 tool_use/tool_result 不匹配
