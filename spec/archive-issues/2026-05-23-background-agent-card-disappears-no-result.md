> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-background-agent-card-disappears-no-result.md

# Background Agent 完成后 SubAgent 卡片消失且无数据回传

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-23
**修复日期**：2026-05-24

## 问题描述

通过 LLM 调用 Agent 工具并设置 `run_in_background: true` 时，SubAgent 卡片在 TUI 中短暂闪现后消失。Background agent 被标记为完成后，卡片立即消失，且父 agent 后续没有收到任何返回数据。此问题必现，导致 background agent 功能完全不可用。

## 根因分析

三层问题叠加：

1. **`SubagentStarted` 缺少 `is_background` 字段**：上次 revert（d1d125f）将 `is_background` 字段从 `SubagentStarted` 事件中删除，且 background task 启动时不发送 `SubagentStarted` 事件。导致 TUI 的 `background_task_count` 永远为 0，`handle_done()` 不设 `agent_done_pending_bg`，Done 先于完成事件到达 → continuation 永远不触发。

2. **事件通道随 executor 生命周期销毁**：background task 完成后通过 `event_handler.on_event(BackgroundTaskCompleted)` 发送到 executor 的 `event_tx` channel，但 `close_channel()` 后 channel sender 设为 None，事件被静默丢弃。

3. **`notification_rx` 同理**：`drain_notifications()` 的 `notification_rx` 绑定在 `ReActAgent` 上，`drop(executor)` 后也被销毁。

## 修复方案

三步改动：

### 1. 恢复 `SubagentStarted.is_background` + 启动时发送

- `SubagentStarted` 增加字段 `is_background: bool`
- `invoke_background()` / `invoke_background_fork()` 在 spawn 前发送 `SubagentStarted(is_background: true)`
- TUI 的 `handle_subagent_start()` 递增 `background_task_count`
- `handle_done()` 正确检测 `background_task_count > 0` → 设 `agent_done_pending_bg`

### 2. 独立 bg 事件通道（不随 executor 销毁）

- 新增 `(bg_event_tx, bg_event_rx): UnboundedChannel<ExecutorEvent>`
- 通过 `SubAgentMiddleware.with_bg_event_sender()` → `SubAgentTool.bg_event_sender` 传递
- spawn 闭包持有 sender clone，闭包结束自动 drop
- executor.rs 中 `build_agent()` 后立即启动 Phase 2 bg pump：`bg_event_rx → sink.push_event()`
- 所有 sender drop 后 pump 自然退出

### 3. 单路径交付（消除历史重复）

- `drain_notifications()` 只做 `state.add_message()`，不 emit 事件
- 事件交付统一通过 `bg_event_sender`（唯一路径）
- 执行期间：state 注入（drain_notifications）+ 事件通知（bg pump）
- 执行后：只有事件通知（bg pump），TUI handler 负责注入 agent_state_messages

## 修复后数据流

```
invoke_background():
  SubagentStarted(is_background: true) → event_tx → TUI → background_task_count++ ✓
  tokio::spawn → 持有 bg_event_sender clone

Agent Done:
  TUI → handle_done → background_task_count > 0 → agent_done_pending_bg = true ✓

Background task completes (可在任意时间):
  registry.complete() → notification_tx → drain_notifications → state.add_message (仅执行中)
  bg_event_sender.send(BGCompleted) → bg_event_rx → Phase 2 pump → TUI ✓

TUI 处理 BGCompleted:
  agent_done_pending_bg = true → push to agent_state_messages → set continuation ✓

Auto continuation:
  poll_agent → pending_bg_continuation.take() → submit_message → 新 agent 轮次 ✓
```

## 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-agent/src/agent/events.rs` | `SubagentStarted.is_background` + `BackgroundTaskResult::to_notification()` |
| `peri-agent/src/agent/events_test.rs` | 测试适配 `is_background` |
| `peri-agent/src/agent/executor/final_answer.rs` | `drain_notifications()` 只做 state 注入，不 emit |
| `peri-agent/src/agent/executor/mod.rs` | `notification_rx` 改为 `pub` |
| `peri-middlewares/src/subagent/tool/define.rs` | `bg_event_sender` 字段 + spawn 闭包发送 BGCompleted + SubagentStarted(bg) |
| `peri-middlewares/src/subagent/mod.rs` | `bg_event_sender` 字段 + `with_bg_event_sender()` + build_tool 透传 |
| `peri-acp/src/agent/builder.rs` | 创建 `(bg_event_tx, bg_event_rx)` → SubAgentMiddleware → AcpAgentOutput |
| `peri-acp/src/session/executor.rs` | Phase 2 bg pump: build_agent 后立即启动异步转发 |
| `peri-tui/src/app/agent.rs` | `map_executor_event` 透传 `is_background` |

## 历史上下文

- 上次修复（6da1dff）引入独立 `bg_event_sender` 通道，但因双路径交付（notification_tx + bg_sender）导致历史消息重复，被 revert（d1d125f）
- 本次修复采用单路径交付策略：`bg_event_sender` 是唯一事件通道，`notification_tx` 仅用于 state 注入，消除重复根因
