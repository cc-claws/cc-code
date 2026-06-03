# 并发 Background Agent 完成后后续 Prompt 卡死

**状态**：Pending
**优先级**：高
**创建日期**：2026-06-03

## 问题描述

使用多个并发 Background Agent 后，所有 bg agent 正常完成、结果正常显示、会话状态正常返回，但在同一 session 中输入新的 prompt 后，spinner 永久卡住。Ctrl+C 无法恢复，必须重启整个 TUI。问题偶发，非必现，与 LLM provider 无关。

## 症状详情

| 阶段 | 表现 |
|------|------|
| 并发 bg agent 执行 | 正常执行，结果正常显示 |
| bg agent 完成后 | 会话状态正常返回，[BG: N] 已清零 |
| 输入新 prompt | spinner 启动后永久卡住 |
| Ctrl+C | 无效，无法中断 |
| 恢复方式 | 必须重启整个 TUI |

## 复现条件

- **复现频率**：偶发（非必现）
- **触发步骤**：
  1. 在同一 session 中发起多个并发 bg agent（`run_in_background: true`）
  2. 等待所有 bg agent 完成，结果正常显示
  3. 在同一 session 中输入新的 prompt
  4. 观察：spinner 卡住，agent 不响应
- **环境**：与 LLM provider 无关

## 涉及文件

- `peri-tui/src/app/agent_events_bg.rs` —— 后台任务完成后的事件处理、continuation 触发
- `peri-acp/src/session/executor.rs` —— agent 执行器，`prompt_with_bg_results` 路径
- `peri-middlewares/src/subagent/background.rs` —— 后台任务注册中心
- `peri-middlewares/src/subagent/tool/execute_bg.rs` —— bg agent 执行与完成通知

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-03 | — | Open | agent | 创建 |
| 2026-06-03 | Open | Fixed | agent | 根因定位 + 修复：Langfuse flush 不再阻塞 event pump |
| 2026-06-03 | Fixed | Pending | agent | 等待用户验证 |

## 根因分析

Event pump 在 `push_done()` 之后、`pump_done_tx.send(())` 之前等待 Langfuse flush。当 Langfuse API 不可达或超时时（HTTP 30s timeout × 重试），flush 阻塞导致 `pump_done_tx` 永远不触发 → `wait_for_pump()` 永久阻塞 → `execute_prompt()` 不返回 → ACP server 的 `prompt_lock` 不释放 → 下一个 prompt 永久等待锁。Ctrl+C 无法恢复因为新 prompt 的 cancel_token 尚未创建（还在等锁）。

**与 bg agent 的关系**：bg agent 增加了 Langfuse 事件量（SubAgent span + 工具观测），使 flush 更可能触发批量发送，增加了 Langfuse 超时的概率。问题不限于 bg agent 场景，任何 Langfuse 慢/不可达的执行都可能触发。

## 修复记录

**修复**：`peri-acp/src/session/executor.rs` — 将 `pump_done_tx.send(())` 移到 Langfuse flush 之前（fire-and-forget），使 Langfuse 完全不阻塞执行管线。`wait_for_pump` 添加 10s timeout 作为安全网。

- `executor.rs:370` — pump 完成信号前移至 `push_done()` 之后、Langfuse flush 之前；Langfuse flush 改为 `drop(handle)` 即发即弃
- `executor.rs:614` — `wait_for_pump` 添加 10s timeout
