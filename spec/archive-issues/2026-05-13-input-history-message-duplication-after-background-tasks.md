> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-input-history-message-duplication-after-background-tasks.md
# 后台 Agent 完成后 input_history 消息重复导致 Prompt Cache 失效

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13
**修复提交**：`f6adb82` fix: 消除后台任务通知在 agent_state_messages 中的重复写入

## 问题描述

当主 Agent 使用后台 Agent（`run_in_background: true`）并等待结果返回后继续对话时，发送给 LLM API 的 `input_history` 中出现大量重复消息。一次包含 3 个后台 Agent 的会话中，34 条消息有 16 条是重复的（占比 47%），导致 prompt cache 命中率降至 0%、token 浪费约 20,000+、以及 tool_call_id 全局唯一性被破坏。

## 症状详情

### 消息重复模式

日志来源：ZAI API 代理记录（请求 ID `85e28921-62c5-47f5-9478-e2cddbeaa00a`，时间 `2026-05-13T13:01:55Z`）

```
msg[1]  user:       "派出 3 个 agent 调查 prompt cache..."
msg[2]  assistant:  tool_calls: [Agent×3]
msg[3]  tool_result: Agent dispatch result 1
msg[4]  tool_result: Agent dispatch result 2
msg[5]  tool_result: Agent dispatch result 3
msg[6]  assistant:  "3 个 agent 已并行派出，等待结果。"
─────────── 以下全部是 msg[1-6] 的完整副本 ───────────
msg[7]  user:       "派出 3 个 agent 调查 prompt cache..."   ← 重复 msg[1]
msg[8]  assistant:  tool_calls: [Agent×3, 相同 call IDs]     ← 重复 msg[2]
msg[9]  tool_result: Agent dispatch result 1                 ← 重复 msg[3]
msg[10] tool_result: Agent dispatch result 2                 ← 重复 msg[4]
msg[11] tool_result: Agent dispatch result 3                 ← 重复 msg[5]
msg[12] assistant:  "3 个 agent 已并行派出，等待结果。"      ← 重复 msg[6]
─────────── 后台任务结果 ───────────
msg[13] tool_result: [后台任务 bg-0320b 完成] 完整报告（3113 字符）
msg[14] tool_result: [后台任务 bg-9c958 完成] 完整报告（3113 字符）
msg[15] tool_result: [后台任务 bg-69e97 完成] 完整报告
─────────── 结果的副本（内容截断） ───────────
msg[16] tool_result: [后台任务 bg-0320b 完成] 完整报告       ← 重复 msg[13]
msg[17] tool_result: [后台任务 bg-9c958 完成] 完整报告       ← 重复 msg[14]
msg[18] tool_result: [后台任务 bg-69e97 完成] 完整报告       ← 重复 msg[15]
msg[19] tool_result: [后台任务 bg-9c958 完成] 截断版（92字符） ← 重复+截断
msg[20] assistant:  汇总报告
msg[21] tool_result: [后台任务 bg-9c958 完成] 截断版（92字符） ← 重复+截断
msg[22] tool_result: [后台任务 bg-9c958 完成] 截断版（92字符） ← 重复+截断
msg[23] assistant:  汇总报告                                     ← 重复 msg[20]
...（第二轮同样出现重复）
```

### 连锁影响

| 影响 | 严重度 | 详情 |
|------|--------|------|
| Prompt Cache 0% 命中 | 高 | `prompt_tokens=40,737`，`cached_tokens=0`。重复消息改变了消息序列结构，缓存前缀完全无法匹配 |
| tool_call_id 重复 | 高 | msg[8] 复用了 msg[2] 的 `call_668048fe...` 等 3 个 ID。违反 API 协议要求 ID 全局唯一 |
| Token 浪费 | 中 | 16 条重复消息浪费约 20,000+ tokens（占总量 49%） |
| 后台任务结果截断 | 中 | msg[19/22]（92 字符）vs 原始 msg[15]（3,113 字符），第二次注入时内容被截断，调查报告内容丢失 |

### 与已有 Issue 的区别

| 已有 Issue | 区别 |
|------------|------|
| `sync-subagent-events-leak-to-parent`（Fixed） | 那个是 Normal/Fork 路径子 Agent StateSnapshot 溢出到父消息流；本 issue 是父 Agent **自身消息被完整复制** |
| `background-task-completion-race-condition`（Open） | 那个是后台任务完成后 continuation 不触发；本 issue 是 continuation 正常触发但消息被重复 |

## 复现条件

- **复现频率**：基于日志分析确认存在，未验证当前代码是否仍可复现
- **触发步骤**：
  1. 向主 Agent 提交一个会发起后台 Agent 的任务（如并行 code review、多方向调查）
  2. 使用 `run_in_background: true` 模式
  3. 等待后台任务完成后主 Agent 继续处理结果
  4. 检查发送给 LLM API 的 input_history 是否出现消息重复
- **环境**：Provider: ZAI，Model: glm-5.1，日志时间 2026-05-13 21:01 CST

## 根因分析

后台任务通知通过两条独立路径写入 `agent_state_messages`，导致每条通知被写入 2 次：

- **路径 A**（executor 侧）：`SubAgent` 后台 tokio task 完成 → `BackgroundTaskRegistry.complete()` → `notification_tx` channel → `drain_notifications()` → `state.add_message()` → `StateSnapshot` 事件 → TUI `extend agent_state_messages`
- **路径 B**（TUI 侧）：同一完成事件 → `handler.on_event(BackgroundTaskCompleted)` → `handle_background_task_completed()` → **无条件** `push` 到 `agent_state_messages`

两条路径使用相同的 8 字符截断 `task_id` 格式（`&result.task_id[..8.min(result.task_id.len())]`），因此产生的消息内容字节级一致。

## 修复方案

**提交**：`f6adb82` — 两条路径互斥，保证每条通知恰好写入 1 次。

| 路径 | 修改 | 行为 |
|------|------|------|
| 路径 A | 恢复 `drain_notifications()` 注入（此前 `8ccc1e4` 将其改为 no-op） | executor 运行期间收到通知 → 写入 state → StateSnapshot 同步到 TUI |
| 路径 B | `handle_background_task_completed` 增加 `agent_done_pending_bg` 守卫 | 仅当 executor 已退出但仍有后台任务未完成时兜底写入 |

互斥条件：`agent_done_pending_bg` 在 executor `Done` 时设为 `true`，此时 executor 不再运行（路径 A 不活跃），路径 B 兜底生效；executor 运行期间 `agent_done_pending_bg = false`，路径 B 不写入，由路径 A 负责。

### 边界情况

executor 退出后、`Done` 事件处理前的极短窗口内到达的通知可能暂时滞留在 `notification_rx` channel 中，不会被任何路径消费。此窗口极短（微秒级），不影响正确性——通知会在下一轮 agent run 的 `history` 中被包含。

## 相关代码

- `peri-tui/src/app/message_pipeline.rs` —— MessagePipeline，管理 `completed` 消息列表和 `round_start_vm_idx`/`drain` 逻辑
- `peri-tui/src/app/agent_ops.rs` —— 事件处理（StateSnapshot、BackgroundTaskCompleted 等）
- `peri-tui/src/app/agent_events_bg.rs` —— 后台任务完成通知处理
- `peri-agent/src/agent/state.rs:86` —— `into_messages()` 将 Agent 状态转换为 `Vec<BaseMessage>` 发送给 LLM

## 关联 Issue

- `spec/issues/2026-05-13-sync-subagent-events-leak-to-parent.md`（状态：Fixed）—— 同为子 Agent 事件处理问题，但现象和根因不同
- `spec/issues/2026-05-13-background-task-completion-race-condition.md`（状态：Open）—— 同为后台任务相关，但聚焦于 continuation 竞态
- `spec/issues/2026-05-13-prompt-cache-hit-rate-risks.md` —— Prompt Cache 命中率风险报告，本 issue 是其中一个实际触发场景
