> 归档于 2026-05-20，原路径 spec/issues/2026-05-19-acp-stopreason-hardcoded-endturn.md

# ACP StopReason 全部硬编码为 EndTurn

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-19
**修复日期**：2026-05-19

## 问题描述

`acp_server.rs` 中 `execute_prompt` 返回的 `PromptResponse` 固定使用 `StopReason::EndTurn`，无论 agent 实际结束原因是什么——用户取消、达到最大轮次、token 溢出等都返回 `EndTurn`。IDE 客户端无法区分正常完成和异常终止，无法给出对应的 UI 反馈。

## 症状详情

```rust
// acp_server.rs:774 (修复前)
let resp = PromptResponse::new(StopReason::EndTurn);
```

ACP 规范定义的 StopReason 枚举：

| StopReason | 含义 | 修复前映射 | 修复后映射 |
|------------|------|---------|---------|
| `EndTurn` | 正常完成 | 唯一返回值 | 默认值，正常完成或其他可恢复错误 |
| `Cancelled` | 用户取消 | ❌ 未使用 | ✅ cancel.is_cancelled() 或 AgentError::Interrupted |
| `MaxTurnRequests` | 达到最大循环次数 | ❌ 未使用 | ✅ AgentError::MaxIterationsExceeded |
| `MaxTokens` | 达到 token 限制 | ❌ 未使用 | 未映射（需 LLM 层 plumb） |
| `Refusal` | 模型拒绝 | ❌ 未使用 | 未映射（需 LLM 层 plumb） |

实际场景中 `executor::execute_prompt()` 返回的 `PromptResult` 只包含 `ok: bool` 和 `messages`，不携带终止原因的语义信息。`ok=false` 时可能是取消也可能是错误，无法区分。

## 修复方案

1. 在 `peri-acp/src/session/executor.rs` 新增 `PromptStopReason` 枚举（EndTurn/Cancelled/MaxTurnRequests），扩展 `PromptResult` 增加 `stop_reason` 字段
2. `execute_prompt()` 中根据 `AgentError` 变体 + `cancel.is_cancelled()` 计算 stop_reason
3. `acp_server.rs` 和 `acp_stdio.rs` 根据 `result.stop_reason` 映射为 ACP `StopReason`

## 涉及文件

- `peri-acp/src/session/executor.rs` — 新增 `PromptStopReason` 枚举，`PromptResult.stop_reason` 字段，两处 return 站点计算 stop_reason
- `peri-tui/src/acp_server.rs` — 使用 `result.stop_reason` 替代硬编码 `EndTurn`
- `peri-tui/src/acp_stdio.rs` — 同上
