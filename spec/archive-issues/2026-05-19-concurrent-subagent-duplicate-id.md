> 归档于 2026-05-20，原路径 spec/issues/2026-05-19-concurrent-subagent-duplicate-id.md

# 并发同类型 SubAgent 共享相同 ID 导致事件路由错误

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-19

## 问题描述

父 Agent 并发调用多个相同类型的 Agent 工具时，所有 SubAgent 实例被分配了相同的 ID（如 `#20eb3c86`）。由于 TUI 消息管线通过 ID 匹配事件到对应的 SubAgent 卡片，ID 重复导致第二个及后续 SubAgent 的全部事件（Grep/Shell 等）被错误路由到第一个 SubAgent 的卡片中。

## 症状详情

用户输入示例（并发调用两个 `Agent(explore)`）：

```
❯ Agent(explore) #20eb3c86
  验证 peri-acp 中是否已实现以下 ACP session 生命周期方法。
  ● Grep(SessionUpdate::)
  ✗ Shell(find ... agent-client-protocol ...)
    ⎿ ✗ 工具 'Bash' 不存在
  ✗ Shell(find ... agent-client-protocol* ...)
    ⎿ ✗ 工具 'Bash' 不存在
  ● Grep(use agent_client_protocol)

❯ Agent(explore) #20eb3c86
  验证 peri-acp 中 `session/update` 通知的所有变体实现
```

| 现象 | 说明 |
|------|------|
| ID 重复 | 两个 `Agent(explore)` 均显示 `#20eb3c86` |
| 内容错位 | 第二个 Agent 的所有工具调用结果（Grep、Shell）只显示在第一个 Agent 卡片内 |
| 第二个卡片空 | 第二个 Agent 卡片仅显示初始描述，不显示任何工具执行结果 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 父 Agent 并发调用两个或多个相同类型的 Agent 工具（如两个 `Agent(explore)`）
  2. 观察生成的 SubAgent ID——全部相同
  3. 观察两个卡片的内容——第二个 Agent 的事件出现在第一个卡片里
- **已验证场景**：相同类型的并发 Agent（如两个 `explore`）
- **未验证场景**：不同类型的并发 Agent（如 `explore` + `code`）

## 根因分析

整条链路没有任何一层生成或传递唯一实例 ID。每一层都用 `subagent_type`（如 `"explore"`）作为标识符，而 LLM 生成的唯一 `tool_call_id` 在映射层被丢弃。

### 数据流追踪（4 层）

**第 1 层：SubAgent 工具执行** — `peri-middlewares/src/subagent/tool/define.rs:749-753`

`SourceAgentIdHandler` 的 `agent_id` 直接使用 `subagent_type`，不是唯一实例 ID：

```rust
SourceAgentIdHandler::new(Arc::clone(handler), agent_id.clone())  // agent_id = "explore"
```

→ 两个并发 `Agent(explore)` 的所有事件都带 `source_agent_id = Some("explore")`。

**第 2 层：TUI 事件映射** — `peri-tui/src/app/agent.rs:30-48`

`ExecutorEvent::ToolStart { name: "Agent", tool_call_id, .. }` 映射时用 `..` **丢弃了唯一的 `tool_call_id`**，只用 `subagent_type` 生成 `SubAgentStart`：

```rust
ExecutorEvent::ToolStart { name, input, .. } if name == "Agent" => {
    let agent_id = input.get("subagent_type")...;  // "explore"
    AgentEvent::SubAgentStart { agent_id, ... }     // 没有传 tool_call_id
}
```

**第 3 层：Pipeline SubAgentState 创建** — `peri-tui/src/app/message_pipeline/mod.rs:293, 405-424`

`SubAgentStart` 触发 `tool_start_internal` 时 `tc_id = format!("subagent_{}", agent_id)` = `"subagent_explore"`。两个并发 SubAgent 得到相同的 `tc_id`：

- `pending_tools.insert("subagent_explore", ...)` → 第二次 insert **覆盖**第一次
- `bg_hash: Some(instance_hash("subagent_explore"))` → 两个都生成 `#20eb3c86`
- `SubAgentState.agent_id` 都是 `"explore"`

**第 4 层：事件路由** — `peri-tui/src/app/message_pipeline/mod.rs:577-581`

`find_running_subagent_mut` 按 `agent_id` 查找，总是返回**第一个**匹配项：

```rust
fn find_running_subagent_mut(&mut self, agent_id: &str) -> Option<&mut SubAgentState> {
    self.subagent_stack.iter_mut()
        .find(|s| s.agent_id == agent_id && s.is_running)  // "explore" 匹配第一个
}
```

### 根因总结

LLM 为每个工具调用生成唯一的 `tool_call_id`，但这条链路的每一层都用 `subagent_type`（类型名）替代了唯一标识：

| 层级 | 文件 | 问题 |
|------|------|------|
| 事件注入 | `subagent/tool/define.rs:749` | `source_agent_id` = subagent_type，不是实例 ID |
| 事件映射 | `app/agent.rs:30` | `..` 丢弃了 `tool_call_id` |
| Pipeline 状态创建 | `message_pipeline/mod.rs:293` | `tc_id` = `subagent_{type}`，不唯一 |
| Pipeline 路由 | `message_pipeline/mod.rs:577` | 按 type 匹配，`find()` 返回第一个 |

### 次生影响

1. **事件路由**：所有事件都路由到第一个 SubAgentState
2. **pending_tools 覆盖**：相同 key 导致第二次 insert 覆盖第一次，SubAgentEnd 可能无法正确匹配
3. **显示哈希**：两个卡片显示相同的 `#20eb3c86`
4. **第二个卡片**：收不到任何事件，保持初始空状态

## 修复方向

将唯一的 `tool_call_id` 贯穿整条链路，替代 `subagent_type` 作为实例标识：

1. **`define.rs`**：`SourceAgentIdHandler` 使用 `tool_call_id`（而非 `subagent_type`）作为 `source_agent_id`
2. **`agent.rs` 映射层**：捕获 `tool_call_id`，传递给 `SubAgentStart` 和 `SubAgentEnd`
3. **`events.rs`（TUI）**：`SubAgentStart`/`SubAgentEnd` 增加 `instance_id` 字段
4. **`mod.rs` pipeline**：用 `instance_id` 做路由和 `pending_tools` key，`agent_id` 保留仅用于显示

## 涉及文件

- `peri-middlewares/src/subagent/tool/define.rs` —— `SourceAgentIdHandler` 的 `agent_id` 使用 `subagent_type`
- `peri-middlewares/src/subagent/tool/mod.rs` —— `SourceAgentIdHandler` 定义
- `peri-tui/src/app/agent.rs:30-48` —— 映射层丢弃 `tool_call_id`
- `peri-tui/src/app/events.rs:67-71` —— `SubAgentStart` 缺少 `instance_id` 字段
- `peri-tui/src/app/message_pipeline/mod.rs:293,405-424,577-581` —— `tc_id` 生成、`SubAgentState` 创建、路由匹配

## 相关 Issue

- `spec/issues/2026-05-18-subagent-duplicate-state-on-completion.md`（已完成修复）——SubAgent 完成瞬间的重复卡片问题，与本 issue 的 ID 重复是不同问题
