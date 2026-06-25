> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-cache-percentage-disappears-after-done.md

# 状态栏缓存百分比在对话停止后消失

**状态**：Fixed + Verify
**优先级**：低
**创建日期**：2026-05-12

## 问题描述

对话运行中状态栏显示 `ctx: 45% [200k] 82%`（末尾为缓存命中率），但 agent 停止（Done）后缓存百分比部分消失，变为 `ctx: 45% [200k]`。ctx 百分比和 context window 仍正常显示。

## 症状详情

| 阶段 | 显示内容 |
|------|---------|
| agent 运行中 | `ctx: 45% [200k] 82%` |
| agent 停止后 | `ctx: 45% [200k]` |

- **必现**：每次对话停止后均出现
- **恢复条件**：下次 agent 运行并收到含 cache_read 的 usage 后重新显示

## 根因分析

状态栏使用 `last_cache_hit_rate()`（`token.rs:70`）获取缓存百分比，该方法基于 `last_usage` 中**最近一次 LLM 调用**的 `cache_read_input_tokens` 计算。

当 agent 正常 Done 时，`last_usage` 保留最后一次完整 API 调用的 usage 数据。但某些场景下最后一次调用的 `cache_read_input_tokens` 为 0 或 `None`（如中断时那次调用未完成、provider 不返回缓存字段），导致 `last_cache_hit_rate()` 返回 `None`，缓存百分比不显示。

**已有替代方案**：`cache_hit_rate()`（`token.rs:57`）基于累计的 `total_cache_read_tokens / total_input_tokens` 计算，只要会话中有过缓存命中就不会返回 `None`，更适合状态栏这种需要持久显示的场景。

## 相关代码

- `peri-tui/src/ui/main_ui/status_bar.rs:104` —— 状态栏调用 `last_cache_hit_rate()`
- `peri-agent/src/agent/token.rs:70-78` —— `last_cache_hit_rate()` 实现（基于 last_usage）
- `peri-agent/src/agent/token.rs:57-65` —— `cache_hit_rate()` 实现（基于累计值）
