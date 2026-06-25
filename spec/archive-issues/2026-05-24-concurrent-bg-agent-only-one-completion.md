> 归档于 2026-05-25，原路径 spec/issues/2026-05-24-concurrent-bg-agent-only-one-completion.md

# 并发 Background Agent 只收到一次完成通知，父 Agent 永久等待

**状态**：完成
**优先级**：高
**创建日期**：2026-05-24
**修复日期**：2026-05-24

## 问题描述

当 LLM 在一次响应中并发调用多个 `Agent` 工具（均设置 `run_in_background: true`）时，TUI 只收到其中一个 bg agent 的 `BackgroundTaskCompleted` 事件，其余 bg agent 的完成通知丢失。导致 `background_task_count` 无法归零，`pending_bg_continuation` 永远不会被设置，父 agent 卡在等待状态。用户只能手动取消。

## 症状详情

| 表现 | 说明 |
|------|------|
| 触发条件 | LLM 单轮并发调用 ≥2 个 `Agent(run_in_background: true)` |
| TUI 显示 | 只有一个 bg agent 的完成结果出现在界面，其余无反馈 |
| Agent 状态 | `background_task_count` 仍 > 0，`agent_done_pending_bg = true`，spinner 持续旋转 |
| 终止方式 | 只能手动取消，agent 不会自动续接 |
| 复现频率 | 必现 |

**数据流预期（N 个并发 bg agent）**：

1. N × `SubagentStarted(is_background: true)` → `background_task_count` += 1 × N = N
2. Agent 发出 `Done` → `handle_done` 检测到 count > 0，设 `agent_done_pending_bg = true`
3. N × `BackgroundTaskCompleted` 事件通过 `bg_event_sender` → `bg_event_rx` → TUI
4. 每个 `BackgroundTaskCompleted` 递减 `background_task_count`
5. 最后一个使 count == 0 → 设置 `pending_bg_continuation` → 自动续接

**实际行为**：只收到 1 次 `BackgroundTaskCompleted`，`background_task_count` 始终 > 0，续接永不触发。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 让 LLM 在一轮中并发调用 ≥2 个 `Agent` 工具，全部设置 `run_in_background: true`
  2. 观察 TUI：所有 bg agent 启动卡片出现，`background_task_count` 正确递增
  3. bg agent 完成后，只有第一个的结果出现，其余的完成通知不出现
  4. Agent 进入等待状态，spinner 持续旋转
- **环境**：所有模型、macOS

## 涉及文件

- `peri-middlewares/src/subagent/tool/define.rs`（470-488 行）—— `invoke_background` spawn 闭包：`registry.complete()` + `bg_event_sender.send()` 完成通知路径
- `peri-middlewares/src/subagent/background.rs`（62-81 行）—— `registry.complete()`：通过 `notification_tx` 发送结果
- `peri-acp/src/session/executor.rs`（343-355 行）—— Phase 2 bg event pump：`bg_event_rx → sink.push_event()`
- `peri-acp/src/agent/builder.rs`（256 行）—— `bg_event_tx/bg_event_rx` unbounded channel 创建
- `peri-tui/src/app/agent_events_bg.rs`（52-242 行）—— `handle_background_task_completed()`：递减 count、匹配 SubAgentGroup、设置 `pending_bg_continuation`
- `peri-tui/src/app/agent_ops/lifecycle.rs`（97-129 行）—— `handle_done()`：`background_task_count > 0` 时设 `agent_done_pending_bg`
- `peri-tui/src/app/agent_ops/polling.rs`（11-27 行）—— `poll_agent()`：消费 `pending_bg_continuation` 触发续接

## 修复方案

实施三项并发安全修复 + 全管线诊断 tracing：

1. **TOCTOU 修复** (`background.rs`) —— `register()` 单次持锁完成计数检查+插入，消除两个 concurrent `invoke_background` 竞态窗口。
2. **幽灵计数防御** (`define.rs`) —— `SubagentStarted` 事件移到 `registry.register()` 成功之后发送，注册失败不再留下永不递减的幽灵计数。
3. **同名 agent 匹配修复** (`agent_events_bg.rs`) —— 两遍查找：优先匹配 `final_result.is_none()` 的目标，兜底回退到原始逻辑。

另外在 5 个关键节点添加了 `[bg-diag]` tracing——sender、bg pump、EventSink 序列化、client pump 反序列化、TUI handler 入口——以便验证时精确诊断事件流。

## 验证清单

- [ ] TUI 触发 ≥2 个并发 background agent，确认两者都收到 `BackgroundTaskCompleted`
- [ ] `background_task_count` 归零后自动触发续接，spinner 停止
- [ ] 同名 background agent（如两个 `code-reviewer`）都有结果展示
- [ ] `RUST_LOG=info` 下 `[bg-diag]` 日志完整覆盖 5 个阶段
