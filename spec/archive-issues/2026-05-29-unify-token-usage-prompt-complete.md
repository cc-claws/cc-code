> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-unify-token-usage-prompt-complete.md
# 统一 Token Usage 传递：引入 prompt_complete 事件

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-29

## 问题���述

当前 token usage 通过 Category ② 双路径传递：`peri/agent_event` 发送完整 `LlmCallEnd`（含 input_tokens/output_tokens），`session/update` 发送有损 `UsageUpdate`（仅 total+size，丢失 thoughtTokens 等字段）。TUI 忽略 session/update 路径只用直传，IDE 只能拿到不完整的 UsageUpdate。两套通道传递同一种数据，冗余且不一致。

## 期望行为

引入统一的 `prompt_complete` 事件，在每轮 LLM 调用完成后携带完整 usage 数据：

```json
{
    "type": "prompt_complete",
    "payload": {
        "stopReason": "end_turn",
        "usage": {
            "totalTokens": 24342,
            "inputTokens": 107,
            "outputTokens": 6,
            "thoughtTokens": 37,
            "cachedReadTokens": 24192
        },
        "_meta": {}
    }
}
```

所有前端（TUI / IDE / stdio）从同一来源获取完整、一致的 token 用量。

## 当前架构问题

| 问题 | 说明 |
|------|------|
| 双路径冗余 | Category ② 同时发 `peri/agent_event` + `session/update`，TUI 和 IDE 各消费不同路径 |
| IDE 数据不完整 | `UsageUpdate` 仅含 `(tokens, size)`，丢失 inputTokens/outputTokens/thoughtTokens 分项 |
| cachedReadTokens 丢失 | 当前 UsageUpdate 不传递缓存命中 token 数，前端无法展示 Anthropic prompt cache 节省量 |
| thoughtTokens 丢失 | 当前 `UsageUpdate` 无法传递推理 token 用量，IDE 无法展示 thinking 成本 |
| stopReason 缺失 | 当前 `LlmCallEnd` 虽有 model 字段但无 stopReason，前端无法区分 end_turn/tool_use/stop 等终止原因 |
| 语义分散 | token usage、上下文警告、重试通知都走 Category ② `both()` 模式，职责不清 |

## 改进方向

1. 新增 `prompt_complete` SessionUpdate 变体（或 ExecutorEvent），统一携带 stopReason + 完整 usage
2. 废弃 Category ② 双路径模式，改为单路径：TUI 和 IDE 都从 `session/update` 获取完整数据
3. `ContextWarning` / `LlmRetrying` 等非 usage 事件归入 Category ③（仅 TUI 需要）

## 涉及文件

- `peri-acp/src/event/mapper.rs` —— Category ② 映射逻辑
- `peri-acp/src/session/event_sink.rs` —— TransportEventSink 双路径发送
- `peri-agent/src/agent/events.rs` —— ExecutorEvent 定义
- `peri-tui/src/app/agent.rs` —— map_executor_event Category ② 分支
