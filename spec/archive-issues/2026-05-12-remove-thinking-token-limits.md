> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-remove-thinking-token-limits.md

# 删除 Anthropic Extended Thinking 的 token 限制

**状态**：Done
**优先级**：低
**创建日期**：2026-05-12
**解决日期**：2026-05-12

## 问题描述

当前 `ChatAnthropic` 为 Extended Thinking 模式设置了两层 token 限制：`budget_tokens` 最小值 1024，以及 `max_tokens` 必须大于 `budget_tokens` 的强制提升。这两处限制应当全部删除，让用户自由配置 thinking token 预算。

## 症状详情

| 限制 | 位置 | 行为 |
|------|------|------|
| `budget_tokens` 最小 1024 | `peri-agent/src/llm/anthropic.rs:53` | `budget_tokens.max(1024)`，传入更小的值被静默提升 |
| `max_tokens` 强制提升 | `peri-agent/src/llm/anthropic.rs:405-406` | 当 `max_tokens <= thinking_budget` 时自动设为 `thinking_budget + 4096` |

## 涉及文件

- `peri-agent/src/llm/anthropic.rs:50-56` — `with_extended_thinking` 方法，删除 `.max(1024)`
- `peri-agent/src/llm/anthropic.rs:404-407` — `max_tokens` 强制提升逻辑，整块删除
