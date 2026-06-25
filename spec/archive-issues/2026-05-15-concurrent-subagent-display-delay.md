> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-concurrent-subagent-display-delay.md

# 并发前台 SubAgent 调用时 UI 感知延迟，SubAgentGroup 卡片不可见

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-15
**修复日期**：2026-05-15

## 问题描述

当 LLM 调用前台 SubAgent（非 background）时，UI 上无法实时看到 Agent 调用发生。即使 SubAgent 已在后台运行（spinner 可见），对应的 SubAgentGroup 消息卡片也不会出现在消息列表中。单 Agent 调用有延迟，多 Agent 并发调用时完全看不到任何调用痕迹。用户感知到"Agent 没有在运行"，实际上后台已经在执行。

## 根因分析（2026-05-15 更新）

### 真正根因：`build_tail_vms()` 在 has_snapshot=true 时排除运行中 SubAgent

ReAct 循环第一轮 LLM 响应后，`set_completed()` 设 `has_snapshot_this_round = true`。当第二轮 LLM 调用 Agent 工具时：

```
build_tail_vms() has_snapshot=true 分支：
  1. messages_to_view_models() reconcile → 生成 completed 消息的 VM
  2. merge_frozen_subagents() → 仅替换已 frozen 的 SubAgentGroup
  3. ❌ subagent_stack 中运行中的 SubAgent 完全被跳过
```

`merge_frozen_subagents()` 只做**替换**（按位置匹配），不**追加**。运行中的 SubAgent（刚由 SubAgentStart 推入 stack，尚未 frozen）永远不会出现在 tail_vms 中。

**用户验证**：用户报告"结束有显示了"—— SubAgentEnd 将 SubAgent frozen 后，`frozen_subagent_vms` 有内容，`merge_frozen_subagents` 才能匹配到 reconcile 路径生成的 SubAgentGroup 占位符进行替换显示。

### 修复（已实施）

在 `has_snapshot=true` 分支中，`merge_frozen_subagents` 之后追加 `subagent_stack` 中未 frozen 的运行中 SubAgentGroup：

```
peri-tui/src/app/message_pipeline.rs:753-769
```
`finalized_vm.is_none()` 确保只追加运行中的，避免与 frozen 重复。

## 症状详情

### 触发条件

- **必现**：每次 SubAgent 调用都出现（单 Agent 也延迟）
- **并发加剧**：2 个以上并发前景 SubAgent 时，**完全看不到** SubAgentGroup 卡片出现
- **表象**：spinner 继续转动（Responding 或 ToolUse 模式），但消息区无 Agent 调用痕迹
- **后台实际状态**：SubAgent 确实在执行（最终结果会以 SubAgentEnd 形式出现）

### 事件流分析

当前 SubAgent 的 UI 感知依赖两条事件路径：

**路径 A（TUI 实际使用的）**：`ToolStart { name: "Agent" }` → `map_executor_event()` → `AgentEvent::SubAgentStart` → `handle_agent_event()` → `request_rebuild()`

**路径 B（被丢弃的）**：SubAgent 中间件发出的 `SubagentStarted` → `map_executor_event()` → **返回 None**，未处理

```
peri-tui/src/app/agent.rs:611-616
ExecutorEvent::SubagentStarted { .. }
| ExecutorEvent::SubagentStopped { .. }
| ... => return None,  // "not yet handled in TUI"
```

### 核心问题：并发 SubAgent 事件路由错误

`MessagePipeline::in_subagent()` 和所有子事件路由函数都只检查 `subagent_stack.last()`：

```
peri-tui/src/app/message_pipeline.rs:590-594
pub fn in_subagent(&self) -> bool {
    self.subagent_stack
        .last()
        .is_some_and(|s| s.is_running && !s.is_background)
}

peri-tui/src/app/message_pipeline.rs:476-491
fn subagent_tool_start(&mut self, ...) {
    if let Some(sub) = self.subagent_stack.last_mut() {  // ← 总是更新最后一个
        ...
        sub.recent_messages.push(vm);
    }
}

peri-tui/src/app/message_pipeline.rs:495-500
pub fn subagent_push_chunk(&mut self, chunk: &str) {
    if let Some(sub) = self.subagent_stack.last_mut() {  // ← 总是推给最后一个
        ...
    }
}
```

**并发场景下的事件错配**：
```
时间线 ────────────────────────────────────────►

Agent1 ToolStart → SubAgentStart(1) → subagent_stack = [Agent1]
Agent2 ToolStart → SubAgentStart(2) → subagent_stack = [Agent1, Agent2]

Agent1 开始执行 → ToolStart("Read") → in_subagent() → subagent_stack.last() → Agent2.recent_messages.⚡推入
Agent2 开始执行 → AssistantChunk("hello") → in_subagent() → subagent_stack.last() → Agent2.recent_messages ✓

Agent1 继续执行 → ToolEnd("Read") → subagent_stack.last() → Agent2.recent_messages.⚡更新
Agent2 继续执行 → ToolStart("Write") → subagent_stack.last() → Agent2.recent_messages ✓

结果: Agent1.recent_messages = []  (空，所有事件被 Agent2 抢走)
      Agent2.recent_messages = [Read(ToolBlock), AssistantBubble("hello"), Write(ToolBlock)]
          ↑ 混杂了两个 agent 的事件
```

### Pipeline 状态与显示时序

SubAgentStart 触发时的 `build_tail_vms()` 行为取决于 `has_snapshot_this_round`：

| `has_snapshot_this_round` | SubAgentGroup 来源 | 说明 |
|---------------------------|-------------------|------|
| `false`（正常）| `subagent_stack` 直接构建 | ✅ SubAgentGroup 应被包含 |
| `true`（已有 StateSnapshot）| `merge_frozen_subagents(&self.frozen_subagent_vms)` | ⚠️ 运行中的 SubAgent（未 frozen）**不会被合并** |

正常流程中 `has_snapshot` 在 `begin_round()` 时重置为 false，经过 `set_completed()`（StateSnapshot 到达）后变为 true。SubAgentStart 通常在 StateSnapshot 之前到达，所以 `has_snapshot = false` 路径应正常工作。

### Spinner 状态未更新

SubAgentStart 事件不更新 spinner 状态（`agent_ops.rs:9-40`）。spinner 保持在 AssistantChunk 或上次 ToolStart 设置的模式（Responding / ToolUse）。用户看不出"切换到 Agent 调用"的状态变化。

### 事件通道容量

`FnEventHandler` 使用 `try_send`（非阻塞），通道容量 32：

```
peri-tui/src/app/agent.rs:156
if let Err(e) = tx_event.try_send(msg) {
    // Full → drop; Closed → warn
}
```

并发 SubAgent 产生大量事件（每个 SubAgent 的 TextChunk、ToolStart、ToolEnd 等），高负载下可能触发事件丢弃，使渲染状态与执行状态进一步不同步。

## 涉及文件

| 文件 | 行数范围 | 说明 |
|------|---------|------|
| `peri-tui/src/app/message_pipeline.rs` | 470-500, 590-594 | SubAgent 事件路由 `subagent_tool_start()`, `subagent_push_chunk()`, `in_subagent()` — 只使用 `last()` |
| `peri-tui/src/app/message_pipeline.rs` | 748-775 | `build_tail_vms()` SubAgentGroup 构建 — `has_snapshot_this_round` 分支 |
| `peri-tui/src/app/agent_ops.rs` | 9-40 | `handle_agent_event` SubAgentStart 分支 — 不更新 spinner |
| `peri-tui/src/app/agent.rs` | 482-501 | `map_executor_event` ToolStart(Agent) → SubAgentStart |
| `peri-tui/src/app/agent.rs` | 611-616 | `SubagentStarted`/`SubagentStopped` 被丢弃（"not yet handled in TUI"）|
| `peri-tui/src/app/agent.rs` | 156 | `try_send` 事件发送 — 通道满时丢弃 |
| `peri-middlewares/src/subagent/tool.rs` | 351-356, 892-897 | 中间件层 `SubagentStarted` 事件发射 |

## 影响分析

1. **用户体验**：用户看不到 Agent 调用发生，误以为系统卡住
2. **多 Agent 工作流**：并发 SubAgent 的核心使用场景（并行探索文件 + 并行 code review）完全无法被用户观察
3. **调试困难**：用户无法判断 Agent 是否被调用、运行到哪一步

## 期望改进方向

1. 并发 SubAgent 事件正确路由到对应的 SubAgentState（而非 `last()`）
2. SubAgentStart 立即产生可见的 UI 反馈（SubAgentGroup 卡片 + spinner 状态更新）
3. 考虑利用 `SubagentStarted`/`SubagentStopped` 生命周期事件（当前被丢弃），实现更精准的 UI 状态同步
