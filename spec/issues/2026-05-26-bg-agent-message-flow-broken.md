# 后台 SubAgent 消息流显示失败

**状态**：Open
**优先级**：高
**类型**：Bug
**创建日期**：2026-05-26

## 问题描述

后台 SubAgent 管理栏（bg_agent_bar）已实现，能显示运行中的后台 agent 列表、工具调用次数、耗时，支持鼠标点击聚焦。但核心的消息流显示功能始终无法正常工作——后台 agent 的消息流内容丢失或显示过时数据。

## 症状详情

| 维度 | 表现 |
|------|------|
| 启动阶段 | 后台 agent 启动后，SubAgentGroup 在主视图中可见 |
| 父 agent 完成后 | 后台 agent 的消息流内容丢失或显示过时数据 |
| total_steps | 冻结时为 8，实际完成后为 12（数据过时） |
| recent_messages | 为空或过时 |
| 聚焦查看 | 聚焦后台 agent 后看不到其详细消息流 |
| 复现频率 | 必现 |

### 用户可见输出

```
❯ Agent(general-purpose) #ec826c34
  ⎿ 全部 8 步执行完毕：
❯ [Background task bg-64f9f completed] Agent: general-purpose | Tool calls: 12 | Duration: 27843ms
```

### 现象 2：并发同名后台 Agent 完成通知丢失（2026-05-26 追加）

当单次 Agent 工具调用并发启动 N（≥2）个**同名**后台 SubAgent 时，LLM 上下文中只能收到一个完成通知。

| 维度 | 表现 |
|------|------|
| 后台 agent 数量 | 3 个（同名 hello-agent） |
| UI 管理栏 | 3 个都显示完成（● completed） |
| LLM 看到的通知 | 仅 1 条 `[Background task bg-xxx completed]` |
| LLM 回复 | "两个已完成，还有一个仍在运行"（实际三个都完成了） |
| 复现频率 | 同名并发时必现 |

**触发现场**：
```
用户: 派出三个 bg 的 hello agent say hello
→ Agent 工具单次调用，batch 并发 3 个 hello-agent
→ UI: ● bg-4b66c completed / ● bg-d7538 completed / ● bg-xxx completed
→ LLM 上下文: 只收到一条完成通知，以为还有一个未完成
```

**根因**：`handle_background_task_completed()` 中的 SubAgentGroup 匹配循环（`agent_events_bg.rs:199-221`）按 `agent_name` + `is_running` 匹配，匹配到第一个后立即 `break`。第一个完成后 `is_running=false`，第二个/第三个完成事件匹配到**同一个**（已被第一个标为完成的）group，`break` 退出，后续仍 running 的 group 永远匹配不到。详见 `docs/superpowers/plans/2026-05-24-concurrent-bg-agent-completion-loss.md` Task 5。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，发送消息让 agent 通过 Agent 工具以 `run_in_background: true` 启动后台 SubAgent
  2. 后台 agent 启动后，SubAgentGroup 在主视图中可见
  3. 等��父 agent 执行完毕（`Done`）
  4. 观察后台 agent 的消息流：total_steps 冻结在旧值、recent_messages 为空或过时
  5. 点击 bg_agent_bar 聚焦后台 agent，无法看到详细消息流
- **环境**：所有模型、所有 OS

## 根因分析

### 数据流断裂点

核心问题在 `drain_subagent_stack()` 调用时机和后台数据同步：

1. **冻结时机过早**：`drain_subagent_stack()` 在父 agent `Done` 时执行，将后台 agent 的 `SubAgentState` 从 `subagent_stack` 移除并冻结到 `frozen_subagent_vms`。此后台 agent 仍在执行，但事件更新路径已断开。

2. **后续事件丢失**：`subagent_stack` 被清空后，后台 agent 的 `ToolStart`/`AssistantChunk` 等事件在 `find_running_subagent_mut()` 中找不到对应的 SubAgentState，直接被丢弃。

3. **BackgroundTaskCompleted 补救不完整**：虽然在 `handle_background_task_completed` 中同步更新了 `frozen_subagent_vms` 和 `view_messages`，但只能更新 `is_running`/`final_result`/`total_steps` 这几个标量字段，无法恢复丢失的 `recent_messages`（滑动窗口消息）。

### 架构矛盾

1. **ReAct 循环生命周期 vs 后台 agent 生命周期不匹配**：
   - `subagent_stack` 是 ReAct 循环的状态容器，在 `done()` 时被 drain
   - 后台 agent 的生命周期独立于父 agent 的 ReAct 循环
   - drain 破坏了后台 agent 的数据追踪

2. **两条数据路径冲突**：
   - **实时路径**：SubAgentState → subagent_stack → build_tail_vms（有 recent_messages 和实时更新）
   - **冻结路径**：frozen_subagent_vms → merge_frozen_subagents → reconcile（只有标量快照，无后续更新）
   - 后台 agent 从实时路径切换到冻结路径后，丢失了消息级别的追踪能力

3. **request_rebuild 的全量覆盖**：`BackgroundTaskCompleted` 先直接更新 view_messages，再调用 request_rebuild()。但 rebuild 从 pipeline 状态重建 tail_vms，覆盖了直接更新。虽然同时更新了 frozen VM，但 frozen VM 只有标量字段，无法承载 recent_messages。

### 尝试过的修复（均未彻底解决）

| 修复 | 结果 |
|------|------|
| filter_for_focus 过滤逻辑 | 聚焦时能匹配 SubAgentGroup，但内容为空 |
| drain_subagent_stack 冻结后台 agent | 保留 SubAgentGroup 不消失，但数据过时 |
| merge_frozen_subagents 改进匹配 | 正确替换，但 frozen VM 本身数据过时 |
| BackgroundTaskCompleted 同步更新 frozen VM | total_steps 修正，但 recent_messages 仍丢失 |
| 追加未匹配 frozen VM 到 tail_vms | 防止消失，但数据仍过时 |

## 可能的解决方向（需进一步排查）

1. **不 drain 后台 agent**：保留在 subagent_stack 中，让后续事件继续更新 SubAgentState。需要处理 reconcile 时的重复 SubAgentGroup 问题。
2. **独立数据通道**：为后台 agent 建立独立的消息追踪通道，不依赖 subagent_stack 的生命周期。
3. **事件重放**：在 BackgroundTaskCompleted 时，从 SubAgent 的完整事件历史重建 recent_messages。

## 涉及文件

| 文件 | 角色 |
|------|------|
| `peri-tui/src/app/message_pipeline/mod.rs` | SubAgentState 管理、drain_subagent_stack、build_tail_vms |
| `peri-tui/src/app/message_pipeline/reconcile.rs` | merge_frozen_subagents、build_rebuild_all |
| `peri-tui/src/app/agent_events_bg.rs` | handle_background_task_completed |
| `peri-tui/src/app/agent_ops/subagent.rs` | SubAgent 事件路由、subagent_stack 管理 |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | handle_done、agent 生命周期 |
| `peri-tui/src/ui/main_ui/bg_agent_bar.rs` | 后台 agent 管理栏 UI |
| `peri-middlewares/src/subagent/tool/define.rs` | invoke_background、SubagentStarted/BackgroundTaskCompleted 发送 |
| `peri-middlewares/src/subagent/background.rs` | BackgroundTaskRegistry、register() TOCTOU |
| `docs/superpowers/plans/2026-05-24-concurrent-bg-agent-completion-loss.md` | 并发完成通知丢失的完整修复计划 |

---

## 深度排查报告

### 1. 事件路由完整路径追踪

#### 1.1 事件通道架构

后台 agent 事件与父 agent 事件共享**同一个 ACP 通知通道**（`acp_notification_rx: mpsc::UnboundedReceiver<AcpNotification>`），不使用独立通道。

```
后台 agent 执行
  → ExecutorEvent（带 source_agent_id）
  → TransportEventSink.push_event()
  → AcpNotification::AgentEvent { event, session_id }
  → acp_notification_rx（unbounded channel）
  → poll_agent() → try_recv()
  → handle_acp_notification()
  → map_executor_event() → AgentEvent
  → handle_agent_event()
```

**关键点**：后台 agent 的事件能正确到达 `handle_agent_event()`，通道层面没有断裂。`agent_done_pending_bg` 标志确保父 agent `Done` 后通道不被关闭（`lifecycle.rs:100-107`）。

#### 1.2 事件路由到达 Pipeline 后的行为

事件到达 `MessagePipeline::handle_event()` 后，路由逻辑（`mod.rs:186-266`）使用 `find_running_subagent_mut(aid)` 查找目标 SubAgentState：

```rust
// mod.rs:576-579
fn find_running_subagent_mut(&mut self, instance_id: &str) -> Option<&mut SubAgentState> {
    self.subagent_stack
        .iter_mut()
        .find(|s| s.instance_id == instance_id && s.is_running)
}
```

**核心断裂点**：`drain_subagent_stack()` 在 `done()` 时被调用（`mod.rs:591`），将 `subagent_stack` 全部清空（`subagent_stack.drain(..)`）。此后 `find_running_subagent_mut()` 永远返回 `None`。

具体事件丢失路径：

| 事件类型 | 路由代码 | drain 后行为 |
|----------|----------|-------------|
| `AssistantChunk { source_agent_id: Some(aid) }` | `find_running_subagent_mut(aid)` → `push_chunk_to_subagent()` | `None`，chunk 被丢弃 |
| `ToolStart { source_agent_id: Some(aid) }` | `find_running_subagent_mut(aid)` → `push_tool_start_to_subagent()` | `None`，工具调用被丢弃 |
| `ToolEnd { source_agent_id: Some(aid) }` | `find_running_subagent_mut(aid)` → `update_tool_end_in_subagent()` | `None`，工具结果被丢弃 |
| `StateSnapshot` | `in_subagent()` 守卫 | 后台 agent 不在前台 subagent_stack 中，`in_subagent()` 返回 false，**会污染 completed** |

#### 1.3 没有"其他地方"接住这些事件

排查所有可能的事件消费点：

1. **`find_running_subagent_mut()`**：唯一的事件路由入口，依赖 `subagent_stack`
2. **`frozen_subagent_vms`**：是 `Vec<MessageViewModel>`，纯数据快照，没有事件处理能力
3. **`RunningBgAgent`**（`chat_session.rs:17-21`）：只存储 `agent_name`、`instance_id`、`started_at`，没有消息追踪字段
4. **`view_messages` 直接操作**：只有 `handle_background_task_completed()` 会操作，但只更新标量字段

**结论**：`drain_subagent_stack()` 后，后台 agent 的所有流式事件（`AssistantChunk`、`ToolStart`、`ToolEnd`）全部静默丢弃，没有任何兜底机制。

### 2. SubAgentState 生命周期分析

#### 2.1 完整生命周期

```
创建: SubAgentStart → tool_start_internal() → subagent_stack.push(SubAgentState { is_running: true })
  ↓
活跃: handle_event() 路由 → find_running_subagent_mut() → 更新 recent_messages/total_steps
  ↓
分支 A（前台 agent）: SubAgentEnd → tool_end_internal()
  → is_running = false, finalized_vm = Some(vm), frozen_subagent_vms.push(vm)
  ↓
分支 B（后台 agent）: SubAgentEnd → tool_end_internal()
  → is_running = true（保持运行）, bg_hash 解析
  → 不 finalized，不推入 frozen_subagent_vms
  ↓
销毁: done()/interrupt() → drain_subagent_stack()
  → subagent_stack.drain(..)
  → 后台 agent 条件分支（mod.rs:630-648）：推入 frozen_subagent_vms，但只保留当时快照
```

#### 2.2 `drain_subagent_stack()` 调用时机和必要性

调用点：
- `done()`（`mod.rs:591`）— 父 agent 正常完成
- `interrupt()`（`mod.rs:603`）— 父 agent 被中断
- Disconnected 路径（`polling.rs:123`）— `pipeline.done()`

**必要性分析**：
- `drain_subagent_stack()` 的设计目的是清理前台 SubAgent 的异常残留（`finalized_vm.is_none() && !is_running`）
- 对后台 agent，它创建了一个"冻结快照"保留显示
- **但对后台 agent 来说，这个快照是过早的**——agent 仍在执行，快照只包含 `drain` 时刻的数据

#### 2.3 如果不 drain 后台 agent

**好处**：
- `find_running_subagent_mut()` 继续工作
- 后续事件正常更新 `recent_messages`/`total_steps`
- 不需要任何新机制

**副作用**：
1. **`in_subagent()` 守卫问题**：`in_subagent()` 检查 `subagent_stack.last()` 是否 `is_running && !is_background`（`mod.rs:693-696`）。后台 agent 的 `is_background = true` 不会被误判为前台。**没有副作用**。
2. **`begin_round()` 清空问题**：`begin_round()`（`mod.rs:716-724`）清空 `frozen_subagent_vms` 但**不清空** `subagent_stack`。如果后台 agent 留在 stack 中，`begin_round()` 后它仍然存在。但新一轮提交时 `submit_message()` 不清理 subagent_stack。**潜在问题**：跨轮次 stack 残留。
3. **RebuildAll 重复 SubAgentGroup**：`build_tail_vms()` 的 `has_snapshot_this_round` 分支（`reconcile.rs:196-222`）先 reconcile，再 `merge_frozen_subagents`，再追加 `subagent_stack` 中未 finalized 的。如果后台 agent 同时存在于 frozen 和 stack 中，会产生重复 VM。
4. **`done()` 的 `subagent_stack` drain 假设**：`done()` 假设 drain 后 stack 为空。如果保留后台 agent，后续 `done()` 会再次 drain 它。

#### 2.4 `begin_round()` 对后台 agent 的影响

`begin_round()` 只清空 `frozen_subagent_vms`（`mod.rs:723`），不清空 `subagent_stack`。这意味着：
- 如果不 drain 后台 agent，跨轮次后 `subagent_stack` 中仍有残留
- 但 `frozen_subagent_vms` 被清空了，`merge_frozen_subagents` 无法匹配到旧轮次的 frozen VM
- 如果后台 agent 在新轮次完成，`BackgroundTaskCompleted` 更新 `view_messages` 时找不到正确的 frozen VM 来同步

### 3. 架构层面方案评估

#### 方案 A：不 drain 后台 agent

**核心思路**：在 `drain_subagent_stack()` 中跳过后台 agent（`is_background && is_running`），让其留在 `subagent_stack` 中继续接收事件。

**可行性评分**：⭐⭐⭐⭐（4/5）

**实现复杂度**：低（约 20 行改动）

**改动清单**：
1. `drain_subagent_stack()`：跳过 `is_background && is_running` 的条目
2. `build_tail_vms()`：确保 `subagent_stack` 中的后台 agent 不与 frozen VM 重复
3. `begin_round()`：需要保护后台 agent 不被意外清理（当前不影响，因为 `begin_round` 不清理 stack）
4. `BackgroundTaskCompleted`：后台 agent 完成时，从 `subagent_stack` 中标记 `is_running = false` 并 finalized，或直接移除

**风险点**：
- **跨轮次累积**：如果用户在新轮次中不涉及后台 agent，stack 中残留的后台 agent 会在每次 `build_tail_vms()` 时输出 SubAgentGroup VM，可能导致重复渲染。需要在 `build_tail_vms()` 中添加去重逻辑（检查 view_messages 是否已有匹配的 SubAgentGroup）。
- **`done()` 多次调用**：第二轮 `done()` 会再次 drain，如果后台 agent 已完成但仍在 stack 中，需要正确处理。
- **StateSnapshot 不一致**：后台 agent 的 `StateSnapshot` 在 `handle_event` 中被 `in_subagent()` 守卫拦截（`mod.rs:344-348`），但 `in_subagent()` 对后台 agent 返回 false，所以子 agent 的 snapshot 会被错误地应用到父 agent 的 completed。

**推荐度**：⭐⭐⭐⭐ 高 — 改动最小，风险可控

---

#### 方案 B：独立消息追踪通道

**核心思路**：在 `MessagePipeline` 中新增 `background_agents: HashMap<String, SubAgentState>` 或类似结构，专门追踪后台 agent 的消息流。

**可行性评分**：⭐⭐⭐⭐（4/5）

**实现复杂度**：中（约 80-120 行改动）

**改动清单**：
1. `MessagePipeline` 新增 `bg_agent_states: HashMap<String, SubAgentState>`
2. `handle_event()` 路由：当 `find_running_subagent_mut()` 返回 None 但有 `source_agent_id` 时，查找 `bg_agent_states`
3. `build_tail_vms()`：合并 `bg_agent_states` 中的 VM 到 tail
4. `handle_background_task_completed()`：从 `bg_agent_states` 移除
5. `drain_subagent_stack()`：将后台 agent 转移到 `bg_agent_states` 而非冻结

**风险点**：
- 需要在多处（`handle_event`、`build_tail_vms`、`done`、`clear`）维护新数据结构的生命周期
- HashMap 的 key 需要精确选择（`instance_id` vs `agent_name`），并发同名 agent 需要用 `instance_id`
- `begin_round()` 不应清空 `bg_agent_states`，但需要区分轮次

**推荐度**：⭐⭐⭐⭐ 高 — 架构最清晰，但实现工作量略大

---

#### 方案 C：事件缓冲重放

**核心思路**：drain 后启动事件缓冲，后续事件缓存到 Vec，`BackgroundTaskCompleted` 时重放缓冲事件重建 `recent_messages`。

**可行性评分**：⭐⭐（2/5）

**实现复杂度**：高（约 150-200 行改动）

**改动清单**：
1. `MessagePipeline` 新增 `bg_event_buffer: HashMap<String, Vec<AgentEvent>>`
2. `handle_event()` 中，当 `find_running_subagent_mut()` 返回 None 但有 `source_agent_id` 时，缓冲事件
3. `BackgroundTaskCompleted` 时，重放缓冲事件到临时 SubAgentState，构建 recent_messages
4. 清理缓冲生命周期

**风险点**：
- **内存无上限**：后台 agent 可能执行很长时间（数小时），缓冲所有事件会消耗大量内存
- **事件重放语义复杂**：ToolStart/ToolEnd 需要配对，AssistantChunk 需要合并，重放逻辑接近于重新实现一遍消息追踪
- **时序问题**：重放是离线操作，用户在缓冲期间看到的是过时数据
- **request_rebuild 覆盖**：重放后需要 rebuild，但 rebuild 可能与其他事件交错

**推荐度**：⭐⭐ 低 — 复杂度高，收益低，内存风险

---

#### 方案 D：直接追踪 RunningBgAgent

**核心思路**：在 `RunningBgAgent` 中添加 `recent_messages`、`total_steps` 等字段，每次事件到达时直接更新 `RunningBgAgent`。

**可行性评分**：⭐⭐⭐（3/5）

**实现复杂度**：中（约 60-100 行改动）

**改动清单**：
1. `RunningBgAgent` 新增 `recent_messages: Vec<MessageViewModel>`、`total_steps: usize` 等字段
2. `handle_agent_event()` 路由：当 `subagent_depth > 0` 或后台 agent 事件到达时，查找 `background_agents` 并更新
3. 渲染逻辑：从 `RunningBgAgent` 读取消息流，构建/更新 SubAgentGroup VM
4. `BackgroundTaskCompleted` 时从 `RunningBgAgent` 提取最终消息

**风险点**：
- **数据分裂**：`RunningBgAgent`（在 `ChatSession`）和 `SubAgentState`（在 `MessagePipeline`）存储重复数据，渲染时需要合并
- **跨层耦合**：`RunningBgAgent` 本是轻量级追踪结构（UI 列表显示），增加消息追踪职责会模糊其角色
- **rebuild 覆盖**：`request_rebuild()` 从 pipeline 状态重建，不读取 `RunningBgAgent`，需要在 `build_tail_vms()` 中额外合并
- **聚焦过滤**：`filter_for_focus()` 检查 `SubAgentGroup` 的 `instance_id`，`RunningBgAgent` 的数据需要映射到 VM

**推荐度**：⭐⭐⭐ 中 — 数据层分裂风险较高

---

### 4. 方案对比总结

| 维度 | A: 不 drain | B: 独立通道 | C: 事件缓冲 | D: RunningBgAgent |
|------|-------------|-------------|-------------|-------------------|
| 可行性 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ |
| 实现复杂度 | 低（~20行） | 中（~100行） | 高（~200行） | 中（~80行） |
| 架构清晰度 | 中 | 高 | 低 | 中 |
| 数据一致性风险 | 中 | 低 | 高 | 中高 |
| 内存风险 | 低 | 低 | 高 | 低 |
| 推荐度 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐⭐ |

### 5. 推荐方案：方案 A + B 渐进实施

**短期（方案 A）**：修改 `drain_subagent_stack()` 跳过后台 agent，最小改动验证效果。

**中期（方案 B）**：如果方案 A 的跨轮次管理复杂度上升，重构为独立 `bg_agent_states` HashMap。

#### 方案 A 具体实施要点

1. **`drain_subagent_stack()` 修改**（`mod.rs:611-651`）：
   ```rust
   // 跳过后台仍在运行的 agent
   let bg_agents: Vec<SubAgentState> = self.subagent_stack
       .drain(..)
       .filter(|s| s.is_background && s.is_running)
       .collect();
   // 保留其余 drain 逻辑不变
   // ...existing finalized/non-running handling...
   // 将后台 agent 放回 stack
   self.subagent_stack.extend(bg_agents);
   ```

2. **`build_tail_vms()` 去重**（`reconcile.rs:196-245`）：
   在追加 `subagent_stack` 条目时（`reconcile.rs:205-222`），检查 view_messages 中是否已有匹配的 SubAgentGroup（通过 `instance_id`），跳过重复。

3. **`BackgroundTaskCompleted` 清理**（`agent_events_bg.rs`）：
   在更新 view_messages 后，从 `subagent_stack` 中移除匹配的后台 agent（标记 `is_running = false` 或移除）。

4. **`begin_round()` 不修改**：当前不清空 `subagent_stack`，后台 agent 自然保留。

5. **`in_subagent()` 守卫验证**：确认后台 agent（`is_background = true`）不会被 `in_subagent()` 误判。当前实现（`mod.rs:693-696`）已正确排除后台 agent。

### 6. 额外发现

#### 6.1 StateSnapshot 泄漏风险

后台 agent 的 `StateSnapshot` 在 `handle_event` 中会绕过 `in_subagent()` 守卫（因为 `in_subagent()` 只排除前台 agent）。但在 `handle_agent_event()` 的 `AgentEvent::StateSnapshot` 分支（`mod.rs:260-283`），有 `subagent_depth > 0` 守卫。当父 agent 已完成（`subagent_depth` 被重置为 0），后台 agent 的 StateSnapshot **可能泄漏到父 agent 的 `completed` 和 `agent_state_messages`**。

这是一个独立 bug，但与后台 agent 数据追踪问题相关——即使解决了消息流显示，StateSnapshot 泄漏会导致上下文膨胀。

#### 6.2 `subagent_depth` 管理问题

`subagent_depth` 在 `handle_subagent_start()`（`subagent.rs:86`）递增，在 `handle_agent_event` 的 `SubAgentEnd` 分支（`mod.rs:58-62`）递减。当后台 agent 的 `SubAgentEnd` 在父 agent `Done` 之前到达时，depth 正确递减。但如果 `SubAgentEnd` 在父 `Done` 之后到达，此时 `subagent_depth` 已在 `submit_message()` 中被重置为 0（`agent_submit.rs:154`），递减会 `saturating_sub` 到 0，不会下溢。

**但问题在于**：如果后台 agent 的 `SubAgentEnd` 在新一轮提交后到达，`subagent_depth` 会先被重置为 0，然后 `SubAgentEnd` 递减仍为 0，不会影响新轮次的判断。

#### 6.3 frozen_subagent_vms 的 begin_round 清空

`begin_round()` 清空 `frozen_subagent_vms`（`mod.rs:723`）。这意味着如果后台 agent 在新轮次开始后被冻结，它的 frozen VM 会丢失。这是方案 A 不依赖 frozen VM、而是保留在 stack 中的原因之一。

#### 6.4 通道 Disconnected 的竞态窗口

`polling.rs:118-163` 的 Disconnected 处理中，当 `agent_done_pending_bg || !background_agents.is_empty()` 时静默清理。但如果后台 agent 完成事件（`BackgroundTaskCompleted`）和 spawn closure 结束（导致通道 Disconnected）之间存在竞态，可能丢失最后的完成事件。当前通过 `pre_done_bg_completions` 缓冲缓解，但只在 agent 未 Done 时生效。

---

## 架构审查报告

**审查日期**：2026-05-26
**审查范围**：peri-tui 多智能体消息追踪与生命周期管理
**审查结论**：当前设计在同步 SubAgent 场景下可工作，但存在根本性的设计缺陷，导致后台 Agent 无法被正确追踪。架构需要结构性改进，而非补丁式修复。

### 一、总体判断

**架构评级：C+（可用但有根本缺陷）**

同步 SubAgent 的消息追踪设计是合理的——单一 `subagent_stack` + reconcile + frozen VM 的三层模型，在生命周期对齐（子 agent 生命周期 ⊂ 父 agent ReAct 循环）的约束下工作良好。问题出在将同一个生命周期模型强行扩展到后台 Agent（生命周期独立于父 agent），导致了架构错配。

这不是一个"漏了某个分支"的 bug，而是 **同步/异步 SubAgent 共享同一个有界状态机** 的设计错误。后台 Agent 需要自己的状态管理，而不是寄生在同步 Agent 的生命周期容器里。

### 二、根本性设计缺陷

#### 缺陷 1：单一有界状态机承载双生命周期

```
                   同步 SubAgent          后台 SubAgent
                   ─────────────         ─────────────
生命周期边界        父 agent ReAct 循环     独立（可能跨多个 ReAct 循环）
创建时机           ToolStart(Agent)        ToolStart(Agent, bg=true)
状态容器           subagent_stack          subagent_stack（同一个！）
销毁时机           done() → drain          done() → drain（过早！）
```

`subagent_stack` 的生命周期绑定在 `done()/interrupt()` 上，这意味着它的清理粒度是 **父 agent 的 ReAct 循环**。但后台 Agent 的生命周期可以跨越多个 ReAct 循环甚至多个用户输入轮次。用同一个容器管理两种不同生命周期的实体，必然导致"该清理的没清理，不该清理的被清理了"。

#### 缺陷 2：Frozen VM 是快照而非代理

```
  SubAgentState (live)          frozen_subagent_vms (snapshot)
  ┌─────────────────────┐       ┌─────────────────────┐
  │ recent_messages: [] │──────>│ recent_messages: [] │  ← 冻结时刻的快照
  │ total_steps: 8      │       │ total_steps: 8      │  ← 不再更新
  │ is_running: true    │       │ is_running: true    │
  └─────────────────────┘       └─────────────────────┘
           ↑ 事件持续到达
           ✗ 但无人消费
```

`frozen_subagent_vms` 的设计意图是"前台 Agent 结束后保留显示数据"，这是一个**快照语义**——冻结后不再变化。对于前台 Agent（已经完成），快照是正确的。但对于后台 Agent（仍在运行），快照语义是错误的——需要的是**代理语义**（持续反映最新状态）。

当前代码尝试在 `handle_background_task_completed` 中直接修改 frozen VM 的标量字段来弥补，但：
1. 不能修改 `recent_messages`（没有增量更新源）
2. `request_rebuild()` 从 pipeline 状态重建 tail_vms 会覆盖直接修改
3. 需要同时修改 `view_messages` 和 `frozen_subagent_vms` 两处（`agent_events_bg.rs:151-234`），任何遗漏都会导致状态不一致

#### 缺陷 3：事件路由与容器生命周期耦合

```rust
// 事件路由的唯一入口
fn find_running_subagent_mut(&mut self, instance_id: &str) -> Option<&mut SubAgentState> {
    self.subagent_stack
        .iter_mut()
        .find(|s| s.instance_id == instance_id && s.is_running)
}
```

事件能否被处理，完全取决于 `subagent_stack` 中是否存在匹配条目。`drain_subagent_stack()` 清空 stack 后，所有后续事件静默丢弃。路由逻辑没有"fallback"——没有事件缓冲、没有独立通道、没有任何兜底机制。

更深层的问题是：事件路由是**推模式**（事件到达时立即处理），但状态容器是**拉模式**（rebuild 时从当前状态重建）。两者对"当前有哪些活跃 SubAgent"的理解可能不一致。

#### 缺陷 4：四层数据结构的职责重叠

一个概念上的"运行中的 SubAgent"被分散到四个数据结构中：

```
┌──────────────────────────────────────────────────────────────────┐
│                    "一个后台 Agent 的状态"                         │
│                                                                    │
│  SubAgentState          frozen_subagent_vms       RunningBgAgent  │
│  (subagent_stack)       (pipeline)                (ChatSession)   │
│  ┌──────────────┐       ┌──────────────┐        ┌────────────┐   │
│  │ agent_id     │       │ agent_id     │        │ agent_name │   │
│  │ instance_id  │       │ instance_id  │        │ instance_id│   │
│  │ task_preview │       │ task_preview │        │ started_at │   │
│  │ total_steps  │       │ total_steps  │        └────────────┘   │
│  │ recent_msgs  │       │ recent_msgs  │                         │
│  │ is_running   │       │ is_running   │                         │
│  │ is_background│       │ is_background│                         │
│  │ finalized_vm │       │ (完整 VM)    │                         │
│  └──────────────┘       └──────────────┘                         │
│                                                                    │
│  view_messages 中的 SubAgentGroup VM                               │
│  ┌──────────────────────────────────────┐                         │
│  │ agent_id, task_preview, total_steps  │                         │
│  │ recent_messages, is_running,         │                         │
│  │ final_result, is_error, is_background│                         │
│  │ bg_hash, batch_agents, instance_id   │                         │
│  └──────────────────────────────────────┘                         │
└──────────────────────────────────────────────────────────────────┘
```

这四个结构存在大量冗余字段：
- `agent_id`/`agent_name`：在三个结构中重复
- `instance_id`：在三个结构中重复
- `is_running`：在三个结构中重复
- `total_steps`：在三个结构中重复
- `recent_messages`：在两个结构中重复

**同步点在哪里？**
- SubAgentState → frozen_subagent_vms：在 `drain_subagent_stack()` 时同步（一次性）
- frozen_subagent_vms → SubAgentGroup VM：在 `build_tail_vms()` → `merge_frozen_subagents()` 时同步（每次 rebuild）
- SubAgentState → SubAgentGroup VM：在 `build_tail_vms()` 的 non-snapshot 分支直接构建
- RunningBgAgent → 无直接同步（只用于 UI 列表显示和 `is_empty()` 检查）
- BackgroundTaskCompleted → view_messages + frozen_subagent_vms：在 `handle_background_task_completed()` 中手动同步

同步模式分析：
- SubAgentState → frozen VM：**推模式**（事件驱动，drain 时触发）
- frozen VM → view_messages：**拉模式**（rebuild 时从 frozen 重建）
- BackgroundTaskCompleted → view_messages + frozen VM：**推模式**（事件到达时直接修改两处）

推/拉混合意味着同一个 SubAgent 的状态可能在不同数据结构中不一致。`handle_background_task_completed` 已经体现了这个问题——需要手动同步三个地方（view_messages、frozen_subagent_vms、background_agents），任何遗漏都会导致 UI 显示不一致。

#### 缺陷 5：RebuildAll 的全量覆盖与增量更新的矛盾

`MessagePipeline` 的核心设计哲学是**全量重建**（rebuild）：每次 `RebuildAll` 从 pipeline 的规范状态（`completed` + 流式状态）重建整个 tail。这是为了确保流式路径和恢复路径产生一致的输出。

但 `handle_background_task_completed` 违背了这个哲学——它直接修改 `view_messages` 和 `frozen_subagent_vms`，然后调用 `request_rebuild()`。Rebuild 会从 pipeline 状态重建 tail_vms，**覆盖**之前的直接修改。为了防止覆盖，需要同步修改 frozen VM，让 `merge_frozen_subagents` 替换重建后的占位符。

这是一个脆弱的约定——任何新增的直接修改路径都需要记住同步 frozen VM，否则 rebuild 会吞掉修改。

### 三、数据流对比：当前 vs 推荐

#### 当前数据流（同步 + 后台 Agent 共享路径）

```
事件源                          Pipeline 状态                         渲染层
──────                          ──────────                           ──────

ToolStart(Agent) ──────> subagent_stack.push() ──────> build_tail_vms()
                           SubAgentState                  ↓
                                   ↓                  SubAgentGroup VM
AssistantChunk ──> find_running_subagent_mut()    (每次 rebuild 重建)
                     ↓                               ↓
                push_chunk_to_subagent()         view_messages
                     ↓
                SubAgentState.recent_messages

ToolEnd(Agent) ────> tool_end_internal()
                     ↓
                前台: is_running=false, finalized_vm=Some(vm)
                后台: is_running=true, 保持

Done() ─────────> drain_subagent_stack() ───> frozen_subagent_vms
                     ↓                         (快照，不再更新)
                subagent_stack = []
                     ↓
                ⚠ 后台 Agent 数据断裂点

后台事件(继续) ──> find_running_subagent_mut() ──> None ──> 静默丢弃 ⚠

BackgroundTaskCompleted ──> 直接修改 view_messages ⚠
                            直接修改 frozen_subagent_vms ⚠
                            request_rebuild() ──> 可能覆盖直接修改 ⚠
```

#### 推荐数据流（同步/后台 Agent 分离）

```
事件源                    Pipeline 状态                         渲染层
──────                    ──────────                           ──────

                    ┌─── 同步 Agent 路径（不变）───┐
                    │                              │
ToolStart ────> subagent_stack                 build_tail_vms()
                    │                              ↓
                SubAgentState              SubAgentGroup VM
                    │                              ↓
Done() ───> drain_subagent_stack()      view_messages
            (只清理前台 Agent)
                    │
                    │
                    ├─── 后台 Agent 路径（新增）───┐
                    │                              │
SubAgentStart ──> bg_agents: HashMap<InstanceId,  │
(bg=true)           BgAgentTracker>                │
                    │                              │
后台事件 ────> bg_agents.get_mut(instance_id)     │
                    │                              ↓
                tracker.update()          bg_tracker_to_vm()
                    │                              ↓
BackgroundTaskCompleted ──> bg_agents.remove()  view_messages
                                                  (通过 build_tail_vms
                                                   统一重建，不直接修改)
```

### 四、与成熟系统的对比

#### Claude Code CLI

Claude Code 的 SubAgent 模型更简单：
- **没有独立的 UI 消息追踪层**：子 agent 的输出直接作为工具结果返回，不实时显示内部步骤
- **后台任务使用独立通知机制**：完成后注入一条 Human 消息到对话历史
- **没有 `subagent_stack` 概念**：子 agent 是原子操作（从 TUI 角度看），只有"进行中"和"已完成"两个状态

对比启示：peri-tui 的 `SubAgentState` 带有 `recent_messages` 滑动窗口，提供了更丰富的实时 UI 反馈，但代价是复杂度显著增加。这个设计决策本身不是错误的，但需要匹配的状态管理架构。

#### Cursor/Continue

Cursor 的后台任务设计：
- **独立 Task 面板**：后台任务在独立的 UI 面板中追踪，不混入主对话流
- **独立状态管理**：每个后台任务有自己的状态机（idle/running/completed/failed），不依赖父 agent 的状态
- **事件通道隔离**：后台任务的事件走独立通道，不与主 agent 事件混合

对比启示：peri-tui 的 `RunningBgAgent`（在 `ChatSession` 中）+ `bg_agent_bar` UI 已经具备了独立面板的基础，但状态追踪仍依赖 `MessagePipeline` 的 `subagent_stack`。状态管理与 UI 显示脱节——UI 有独立面板，状态没有独立管理。

### 五、推荐架构改进方向

#### 核心原则：分离同步/后台 Agent 的状态管理

```
                    ┌─────────────────────────────┐
                    │      MessagePipeline         │
                    │                              │
                    │  subagent_stack: Vec<Sub>    │  ← 只管同步 Agent
                    │  frozen_subagent_vms: Vec    │  ← 只存前台快照
                    │                              │
                    │  bg_trackers: HashMap<       │  ← 新增：后台 Agent
                    │    InstanceId,               │     独立生命周期
                    │    BgAgentTracker             │
                    │  >                           │
                    └─────────────────────────────┘
```

#### BgAgentTracker 设计

```rust
/// 后台 Agent 的独立状态追踪器
/// 生命周期：SubAgentStart(bg=true) 创建，BackgroundTaskCompleted 销毁
/// 不受 done()/interrupt()/begin_round() 影响
struct BgAgentTracker {
    agent_id: String,
    instance_id: String,
    task_preview: String,
    total_steps: usize,
    recent_messages: Vec<MessageViewModel>,  // 滑动窗口，持续更新
    bg_hash: Option<String>,
    completed: bool,       // BackgroundTaskCompleted 后设为 true
    final_result: Option<String>,
    is_error: bool,
}
```

#### 关键改动点

1. **`drain_subagent_stack()`**：不再处理后台 Agent。后台 Agent 从 `subagent_stack` 转移到 `bg_trackers`，不冻结。

2. **事件路由**：`handle_event()` 中，当 `find_running_subagent_mut()` 返回 None 且有 `source_agent_id` 时，查找 `bg_trackers`。事件不会丢弃。

3. **`build_tail_vms()`**：合并 `bg_trackers` 中的 tracker 为 SubAgentGroup VM。去重逻辑基于 `instance_id`。

4. **`handle_background_task_completed()`**：标记 `bg_trackers[id].completed = true`，设置 final_result，然后 `request_rebuild()`。不再直接修改 `view_messages` 或 `frozen_subagent_vms`。

5. **`begin_round()`**：不清空 `bg_trackers`。后台 Agent 跨轮次存活。

6. **`clear()`**：清空 `bg_trackers`（会话结束时）。

7. **清理时机**：`BackgroundTaskCompleted` 后的 rebuild 中，已完成的 tracker 从 `bg_trackers` 移除。或者保留到 `begin_round()` 后下一轮的第一次 rebuild（给用户查看结果的时间窗口）。

#### 与现有冻结机制的关系

- `frozen_subagent_vms`：**仅用于前台 Agent**，语义不变
- `subagent_stack`：**仅用于同步 Agent**，drain 时不需要特殊处理后台
- `bg_trackers`：**新增**，独立生命周期，不参与 drain/begin_round
- `RunningBgAgent`（`ChatSession`）：可考虑与 `bg_trackers` 合并，消除冗余

#### 实施建议

**短期（最小可行改动）**：在 `drain_subagent_stack()` 中跳过后台 Agent（保持 `subagent_stack` 不清理），最小化改动验证效果。对应上面的方案 A。

**中期（推荐架构）**：引入 `bg_trackers` HashMap，将后台 Agent 从 `subagent_stack` 和 `frozen_subagent_vms` 中完全分离。这样三种状态容器各司其职：
- `subagent_stack`：同步 Agent 实时追踪
- `frozen_subagent_vms`：前台 Agent 冻结快照
- `bg_trackers`：后台 Agent 独立追踪

**长期（可选）**：考虑将 `RunningBgAgent`（`ChatSession` 层）与 `bg_trackers`（`Pipeline` 层）合并，消除跨层状态冗余。

### 六、风险评估

| 维度 | 短期方案（不 drain） | 中期方案（bg_trackers） |
|------|---------------------|------------------------|
| 改动量 | ~30 行 | ~150 行 |
| 测试覆盖 | 现有测试基本覆盖 | 需要新增 bg_trackers 相关测试 |
| 回归风险 | 中（跨轮次 stack 残留） | 低（独立容器，不影响现有逻辑） |
| 长期可维护性 | 中（增加 drain 逻辑复杂度） | 高（职责清晰分离） |
| StateSnapshot 泄漏 | 需额外修复 | 天然隔离（bg_trackers 不参与 reconcile） |

**建议**：先实施短期方案验证效果，同时规划中期重构。短期方案的跨轮次残留问题会在中期方案中彻底解决。

## 七、根因修正：存储层已实现，消费侧未对接

### 7.1 关键发现

在排查过程中发现，**后台 SubAgent 的消息持久化存储已经完整实现**，但 TUI 层从未对接这个存储。

这直接违反了统一存储设计文档（`docs/superpowers/specs/2026-05-24-unified-agent-storage-design.md`）中 8.2 节的规划。

### 7.2 统一存储设计的核心原则（已实现）

设计文档确立的核心原则：

1. **Agent 是一等公民**：subagent 和主 agent 本质上是同一种实体，每个都有自己的 thread
2. **Thread 为主表**：复用 `threads` 表，`parent_thread_id` 构成父子关系树
3. **写入路径**：只写 own thread（每个 agent 的消息独立存储）
4. **消费路径**：浏览子 agent 历史时用 `load_messages(child_thread_id)` 加载其自身消息
5. **source_agent_id 即 thread_id**：事件中的 `source_agent_id` 就是持久化的 thread_id

存储层 API（`peri-agent/src/thread/sqlite_store.rs`）已实现：

| API | 功能 | 状态 |
|-----|------|------|
| `create_thread(parent_thread_id)` | 创建子 agent 的独立 thread | done |
| `save_message(thread_id, message)` | 追加消息到子 thread | done |
| `load_context(thread_id)` | 加载完整上下文（祖先快照 + 自身） | done |
| `load_messages(thread_id)` | 加载自身消息（人类阅读用） | done |
| `list_child_threads(parent_id)` | 列出子 thread | done |
| `resolve_ancestor_chain(thread_id)` | 解析祖先链 | done |

### 7.3 设计文档规划的消费路径（未实现）

设计文档 8.2 节明确规划了消费路径：

```
用户通过 /tasks 面板的 tab 切换到某个子 agent:
  1. list_child_threads(root_thread_id) 获取子 thread 列表
  2. 选中某个子 thread -> load_messages(child_thread_id) 加载其自身消息
  3. 渲染到 tab 视图
  4. 不需要 load_context（人类阅读只需自身消息）
```

关键区分：
- **继续对话**（主 agent）：用 `load_context()`（LLM 需要完整上下文）
- **浏览历史**（子 agent tab）：用 `load_messages()`（人类阅读只需自身消息）

**这条路径从未实现。** 当前的 bg_agent_bar 临时方案完全绕过了存储层，使用内存中的 `subagent_stack`/`frozen_subagent_vms` 追踪消息，导致 `drain_subagent_stack()` 后数据断裂。

### 7.4 当前代码与设计文档的偏离清单

设计文档明确说 `source_agent_id` 的语义是"持久化 thread_id"。但 TUI 层完全忽略了这一点：

| 设计文档要求 | 当前实现 | 偏离 |
|-------------|---------|------|
| 每个 agent 有 thread_id | 存储层已实现 | -- |
| source_agent_id = thread_id | ACP 层已对齐，TUI 层当临时标识用 | TUI 用 instance_id 而非 thread_id |
| 浏览子 agent 用 load_messages() | 未实现 | TUI 用 frozen_subagent_vms 内存快照 |
| list_child_threads 获取子 agent 列表 | 未实现 | TUI 用 background_agents Vec |
| /tasks 面板 Agent Threads Tab | 未实现 | bg_agent_bar 临时替代 |
| drain 不影响消息持久化 | 存储层正确 | TUI 层因 drain 导致显示断裂 |

### 7.5 问题重定义

之前的问题定义是"如何在 drain 后保留后台 agent 的实时追踪"——这是在错误层面修补。

**正确的问题定义**：**补齐统一存储设计的消费侧**。存储层已经为每个 SubAgent 建立了独立的 child thread 并持久化了消息。TUI 层需要对接 `load_messages(child_thread_id)` 来渲染子 agent 的消息，而不是在内存中用快照/冻结机制追踪。

### 7.6 纠正后的实施方案

#### 短期：bg_agent_bar 聚焦对接 SQLite

1. `RunningBgAgent` 增加 `child_thread_id: String` 字段（即 `source_agent_id`）
2. SubAgent 创建时记录 `child_thread_id` 到 `RunningBgAgent`
3. 用户点击 bg_agent_bar 聚焦后台 agent 时：
   - 调用 `load_messages(child_thread_id)` 从 SQLite 加载完整消息
   - `messages_to_view_models()` 转换为 `Vec<MessageViewModel>`
   - 发送到渲染线程
4. 取消聚焦时恢复主视图

**核心改动**：`filter_for_focus` 不再过滤内存中的 `view_messages`，而是从 SQLite 重新加载。

#### 中期：/tasks 面板 Agent Threads Tab

按设计文档 8.2-8.3 节实现：

```
Session 的 agent threads:
  * Main Agent (thread_1) [active]     <- 主 agent
  o Code Reviewer (thread_2) [done]
  o Explorer (thread_3) [done]
  o Background Task (thread_4) [cancelled]
```

- `list_child_threads(root_thread_id)` 获取列表
- 选中后 `load_messages(child_thread_id)` 加载
- 不依赖 `subagent_stack` / `frozen_subagent_vms` / `RunningBgAgent`

#### 可以废弃的临时机制

消费侧对接 SQLite 后，以下内存追踪机制可以逐步简化或废弃：

| 机制 | 当前职责 | 对接后 |
|------|---------|--------|
| `frozen_subagent_vms` | drain 后的静态替代 | 仅前台 agent 需要，后台 agent 不再使用 |
| `RunningBgAgent` | UI 列表追踪 | 可被 `list_child_threads()` 替代 |
| `bg_agent_bar` | 临时管理栏 | 可被 /tasks 面板替代 |
| `filter_for_focus` | 内存过滤 | 改为 SQLite 加载 |
| `merge_frozen_subagents` | 合并冻结快照到 reconcile | 后台 agent 不再需要此步骤 |

### 7.7 为什么这比所有内存修补方案都好

| 维度 | bg_trackers / 不 drain | SQLite 消费侧对接 |
|------|----------------------|-------------------|
| 与统一存储设计 | 无关，新增概念 | **对齐**，补齐消费侧 |
| 数据来源 | 内存（可能丢失） | 持久化（不丢失） |
| 生命周期 | 需管理创建/销毁/跨轮次 | SQLite 天然持久 |
| 历史消息 | recent_messages 滑动窗口 | **完整历史** |
| drain 影响 | 需修改 drain 逻辑 | **不影响**，存储独立于 drain |
| 实时更新 | 需要事件路由不丢弃 | StateSnapshot 自动持久化，无需额外路由 |
| 可废弃代码 | 增加复杂度 | 减少（逐步废弃 frozen VM 等临时机制） |

**根本优势**：存储层已经做了正确的事。我们不需要修内存追踪链路，只需要在正确的位置（TUI 聚焦时）读取正确的数据源（SQLite child thread）。
