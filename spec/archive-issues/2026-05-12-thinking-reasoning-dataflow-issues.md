> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-thinking-reasoning-dataflow-issues.md

# Thinking/Reasoning 数据流问题：占位 thinking 缺 signature + AiReasoning 死代码

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-12
**修复日期**：2026-05-15

## 现象 1 修复记录（2026-05-12）

删除了 `anthropic.rs:402-428` 的占位 thinking block 注入逻辑。根据 Anthropic 官方文档，API 只要求保留已有的 thinking blocks（含合法加密 signature），不要求凭空注入。伪造 thinking block 无合法 signature 会被 API 验证拒绝。之前轮次的 thinking blocks 会被 API 自动剥离。

## 问题描述

Thinking/Reasoning 在整个数据流中存在一个潜在 Bug 和一处死代码。

1. **Anthropic extended thinking 占位 thinking block 缺少 `signature` 字段**：当历史消息不含 thinking block 但有 tool_use 时，代码注入占位 thinking block，但没有 `signature` 字段。Anthropic API 要求 thinking block 必须有 signature，可能导致 400 错误。
2. **`AiReasoning` 事件从未被 emit**：executor 层（`tool_dispatch.rs`、`final_answer.rs`）始终 emit `TextChunk` 而非 `AiReasoning`，导致 TUI 的 `push_reasoning()`、`current_ai_reasoning` 缓冲区、`ContentBlockView::Reasoning` 流式路径全部为死代码。

## 症状详情

### 现象 1：占位 thinking block 缺 signature

**位置**：`peri-agent/src/llm/anthropic.rs:412-418`

```rust
arr.insert(0, json!({
    "type": "thinking",
    "thinking": "(thinking)",
    // 缺少 "signature" 字段
}));
```

**触发条件**：extended thinking 模式下，历史消息中有 assistant 消息不含 thinking block（来自非 thinking 会话或跨模型迁移）且有 tool_use。

**期望行为**：注入合法的 thinking block（带 signature），或找到其他方式满足 API 要求。

**实际行为**：注入无 signature 的 thinking block。如果 Anthropic API 对历史消息中的 thinking block 强制校验 signature 存在性，会返回 400 错误。

### 现象 2：AiReasoning 死代码链

**涉及文件**：

| 文件 | 位置 | 说明 |
|------|------|------|
| `peri-agent/src/agent/events.rs:18` | `AiReasoning(String)` 定义 | 事件已定义但从未 emit |
| `peri-tui/src/app/message_pipeline.rs:195-206` | `AiReasoning` 分支 | 接收处理逻辑完整但永远不会执行 |
| `peri-tui/src/app/message_pipeline.rs:334-336` | `push_reasoning()` | 追加到 `current_ai_reasoning`，但此缓冲区始终为空 |
| `peri-tui/src/app/message_pipeline.rs:538-539` | `has_streaming_content()` | 检查 `current_ai_reasoning`，始终 false |
| `peri-tui/src/app/message_pipeline.rs:563-566` | `build_streaming_bubble()` | 构建 `ContentBlockView::Reasoning`，永远不会执行 |
| `peri-tui/src/ui/message_view.rs:447` | `ContentBlockView::Reasoning` | 视图变体定义，流式路径不可达 |

**原因**：当前使用非流式 API（`"stream": false`），LLM 返回完整响应后通过 `source_message` 保留 reasoning blocks，不需要流式推理事件。`AiReasoning` 是为未来流式 API 预留的接口。

**影响**：不是 bug，但增加了代码复杂度和维护负担。TUI 中 reasoning 的显示完全依赖 StateSnapshot → reconcile 路径（从 `BaseMessage` 的 `ContentBlock::Reasoning` 转换），而非流式路径。

## 复现条件

- **现象 1**：使用 Anthropic extended thinking 模型（如 claude-sonnet-4），在一个开启了 thinking 的会话中，加载了来自非 thinking 会话的历史消息（含 tool_use 的 assistant 消息）。
- **现象 2**：任何使用场景下都不会触发（死代码）。

## 相关代码

- `peri-agent/src/llm/anthropic.rs:402-428` — extended thinking 占位 thinking 注入逻辑
- `peri-agent/src/llm/openai.rs:646-650` — OpenAI generate_reasoning 提取 thought（仅 Text blocks，正确）
- `peri-agent/src/agent/executor/tool_dispatch.rs:29-44` — tool_dispatch 明确选择 TextChunk 而非 AiReasoning
- `peri-agent/src/agent/executor/final_answer.rs:76-89` — final_answer 同样选择 TextChunk
- `peri-agent/src/agent/events.rs:18` — AiReasoning 事件定义
- `peri-tui/src/app/message_pipeline.rs:195-206,334-336,538-539,563-566` — TUI 接收链
- `peri-tui/src/ui/message_view.rs:447` — Reasoning 视图变体
