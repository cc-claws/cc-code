> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-sync-subagent-events-leak-to-parent.md
# 同步子 Agent（Normal/Fork）事件溢出到主 Agent 消息流

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

Normal 和 Fork 两条同步子 Agent 路径将父 Agent 的 `event_handler` 透传给子 Agent，导致子 Agent 执行期间产生的所有事件（TextChunk、ToolStart/ToolEnd、StateSnapshot、Done 等）全部通过 `map_executor_event()` 映射到 TUI 层并混入父 Agent 的消息流。Background 路径因明确不共享 event_handler 而不受影响。

虽然 `message_pipeline` 的 `subagent_stack` 机制对 `TextChunk`/`ToolStart`/`ToolEnd`/`Done`/`Interrupted` 有 `in_subagent()` 守卫，但 `StateSnapshot` **完全没有守卫**，会直接污染父 Agent 的 `completed` 消息列表和 `agent_state_messages`。

## 症状详情

### 事件流对比

```
Background 路径（正常 ✅）：
  SubAgentTool → 不共享 event_handler → 子 agent 事件不进入父 channel
  → 只有 BackgroundTaskCompleted 事件通过 spawn closure 发送到父

Normal/Fork 路径（异常 ❌）：
  SubAgentTool → with_event_handler(Arc::clone(handler)) → 子 agent 所有事件进入父 channel
  → map_executor_event() 无差别映射 → agent_ops → pipeline
```

### 溢出的事件类型

| 事件 | pipeline 有 in_subagent() 守卫 | 是否溢出 | 影响 |
|------|------|------|------|
| TextChunk | ✅ 有 | ⚠️ 理论上不溢出 | 如 SubAgentStart 被丢弃则溢出 |
| ToolStart | ✅ 有 | ⚠️ 理论上不溢出 | 同上 |
| ToolEnd | ✅ 有 | ⚠️ 理论上不溢出 | 同上 |
| Done | ✅ 有 | ⚠️ 理论上不溢出 | 同上 |
| Interrupted | ✅ 有 | ⚠️ 理论上不溢出 | 同上 |
| **StateSnapshot** | **❌ 无** | **✅ 必然溢出** | **父 agent 消息历史被污染** |
| SubagentStarted/Stopped | N/A（核心层事件） | ⚠️ 可能干扰 | 被忽略但浪费 channel 容量 |
| ContextWarning | ❌ 无 | ⚠️ 可能溢出 | 可能误触发 auto-compact |
| LlmCallEnd (TokenUsage) | N/A | ⚠️ 可能溢出 | 可能更新错误的 token 计数 |

### StateSnapshot 溢出的具体影响

**`agent_ops.rs:793-806`**：
```rust
AgentEvent::StateSnapshot(msgs) => {
    // 直接 extend，没有检查 subagent_depth
    self.agent_state_messages.extend(msgs.clone());
    // pipeline 也没有 in_subagent() 守卫
    pipeline.handle_event(AgentEvent::StateSnapshot(msgs));
}
```

**`message_pipeline.rs:316-319`**：
```rust
AgentEvent::StateSnapshot(msgs) => {
    // 没有 in_subagent() 检查！直接 extend completed
    self.set_completed(msgs);
    vec![PipelineAction::None]
}
```

后果：
1. 父 Agent 的 `agent_state_messages` 包含子 Agent 的全部内部消息（Human/Ai/Tool）
2. 父 Agent 的 `completed` 列表被污染，后续 `messages_to_view_models()` 会将子 Agent 消息渲染为主 Agent 消息
3. 持久化时子 Agent 的消息被写入父 Agent 的对话历史

### 事件丢失导致级联溢出

`FnEventHandler` 使用 `try_send` 发送事件（`agent.rs:156`），channel 满时事件被静默丢弃。如果 `SubAgentStart`（由父 Agent 的 `ToolStart { name: "Agent" }` 映射而来）被丢弃，则 `subagent_stack` 不会被推入，后续所有子 Agent 事件的 `in_subagent()` 返回 false，导致全部溢出到父 Agent。

## 复现条件

- **复现频率**：必现（Normal/Fork 路径每次执行都会产生 StateSnapshot 溢出；其他事件溢出在 channel 拥挤时出现）
- **触发步骤**：
  1. 向主 Agent 提交一个会触发子 Agent 的任务（如 code review、并行探索）
  2. 使用 Normal 或 Fork 模式（非 Background）
  3. 观察主 Agent 的消息流——子 Agent 的文本、工具调用、甚至子 Agent 的子 Agent 消息都出现在主 Agent 的对话中
- **环境**：所有 Provider，所有模型

## 相关代码

- `peri-middlewares/src/subagent/tool.rs:319-321` —— Fork 路径：`with_event_handler(Arc::clone(handler))` 透传
- `peri-middlewares/src/subagent/tool.rs:484-486` —— Background 路径注释：明确不共享 event_handler
- `peri-middlewares/src/subagent/tool.rs:882-884` —— Normal 路径：`with_event_handler(Arc::clone(handler))` 透传
- `peri-tui/src/app/agent.rs:480-579` —— `map_executor_event()`：无差别映射所有事件
- `peri-tui/src/app/agent.rs:155-156` —— `try_send`：channel 满时静默丢弃事件
- `peri-tui/src/app/agent_ops.rs:793-806` —— `StateSnapshot` 处理：直接 extend 父 agent 状态
- `peri-tui/src/app/message_pipeline.rs:316-319` —— `StateSnapshot` pipeline 处理：无 `in_subagent()` 守卫
- `peri-tui/src/app/message_pipeline.rs:199-314` —— TextChunk/ToolStart/ToolEnd/Done/Interrupted：有 `in_subagent()` 守卫

## 关联 Issue

- `spec/issues/2026-05-13-background-task-completion-race-condition.md`（状态：Open）—— 同为子 Agent 事件处理问题，但聚焦于 Background 路径的竞态条件

## 修复记录（2026-05-13）

**根因**：`StateSnapshot`、`ContextWarning`、`LlmRetrying` 三个事件在添加 `in_subagent()`/`subagent_depth` 守卫时被遗漏。`TokenUsageUpdate` 已有守卫（`agent_ops.rs:136-143`），确认为疏忽而非设计意图。

**修复方案**：在 `agent_ops.rs` 和 `message_pipeline.rs` 中为缺少守卫的事件添加 `subagent_depth > 0` / `in_subagent()` 检查，与已有守卫模式一致。

| 文件 | 事件 | 守卫类型 |
|------|------|----------|
| `message_pipeline.rs:316-324` | `StateSnapshot` | `in_subagent()` — 跳过 `set_completed()` |
| `agent_ops.rs:801-809` | `StateSnapshot` | `subagent_depth > 0` — 跳过 `agent_state_messages.extend()` |
| `agent_ops.rs:71-78` | `ContextWarning` | `subagent_depth > 0` — 跳过 auto-compact 触发 |
| `agent_ops.rs:835-842` | `LlmRetrying` | `subagent_depth > 0` — 跳过 `retry_status` 覆盖 |
