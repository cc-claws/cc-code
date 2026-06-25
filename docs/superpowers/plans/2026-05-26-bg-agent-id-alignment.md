# 后台 SubAgent 消息流：存储层 ID 对齐 + 消费侧对接

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复后台 SubAgent 的 `instance_id`（`bg-{uuid4}`）与 SQLite `child_thread_id`（uuid7）不匹配的问题，补齐统一存储设计的消费侧——聚焦后台 agent 时从 SQLite 加载完整消息流。

**Architecture:** 核心修复在中间件层（`define.rs`）：将后台路径的 `SubagentStarted.instance_id` 改为使用 `child_thread_id` 而非 `task_id`，使 TUI 层的 `RunningBgAgent.instance_id` 与 SQLite thread ID 对齐。同时扩展 `BackgroundTaskResult` 携带 `child_thread_id`，让 TUI 完成事件能正确匹配��消费侧（`agent_render.rs`）已实现 `load_messages()` 调用，ID 修复后即可工作。

**Tech Stack:** Rust, tokio async, SQLite (sqlx), peri-agent thread store trait

---

## 根因概述

```
当前（ID 不匹配）:
  define.rs:468  task_id = "bg-{uuid4}"
  define.rs:551  bg_child_thread_id = "{uuid7}"  ← SQLite 用这个
  define.rs:695  SubagentStarted { instance_id: task_id }  ← TUI 收到 "bg-{uuid4}"
  TUI:           RunningBgAgent.instance_id = "bg-{uuid4}"
  TUI:           load_messages("bg-{uuid4}") → 空！SQLite 里是 "{uuid7}"

修复后（ID 对齐）:
  define.rs:695  SubagentStarted { instance_id: bg_child_thread_id.clone() }
  TUI:           RunningBgAgent.instance_id = "{uuid7}"
  TUI:           load_messages("{uuid7}") → 正确加载消息
```

同样的问题存在于 background fork 路径（`define.rs:909`）。

---

### Task 1: 扩展 BackgroundTaskResult 携带 child_thread_id

**Files:**
- Modify: `peri-agent/src/agent/events.rs:1-29`
- Modify: `peri-middlewares/src/subagent/tool/define.rs` (background 路径 + fork 路径)

**问题：** `BackgroundTaskResult` 只有 `task_id`（`bg-{uuid4}`），没有 `child_thread_id`（SQLite thread ID）。TUI 完成事件处理需要 `child_thread_id` 来匹配 `RunningBgAgent`。

- [ ] **Step 1: 在 BackgroundTaskResult 中添加 child_thread_id 字段**

```rust
// peri-agent/src/agent/events.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackgroundTaskResult {
    pub task_id: String,
    pub agent_name: String,
    pub prompt_summary: String,
    pub success: bool,
    pub output: String,
    pub tool_calls_count: usize,
    pub duration_ms: u64,
    /// SQLite child thread ID（uuid7），用于 TUI 聚焦时 load_messages
    pub child_thread_id: Option<String>,
}
```

- [ ] **Step 2: 更新 BackgroundTaskResult::to_notification 使用 child_thread_id**

无需修改——`to_notification` 使用 `task_id` 的 short_id 做显示，保持不变。

- [ ] **Step 3: 运行 cargo check 验证编译错误（预期：所有构造 BackgroundTaskResult 的地方缺少 child_thread_id 字段）**

Run: `cargo check -p peri-agent -p peri-middlewares 2>&1 | head -40`

- [ ] **Step 4: 修改 background 路径的 BackgroundTaskResult 构造（define.rs ~610-634）**

在 `define.rs` background 非 fork 路径中，`spawn_child_thread_id` 已在作用域内。修改 Ok 和 Err 两个分支：

```rust
// Ok 分支 (~line 616)
BackgroundTaskResult {
    task_id: spawn_task_id.clone(),
    agent_name: spawn_agent_name.clone(),
    prompt_summary: spawn_prompt_summary.clone(),
    success: true,
    output: output.text,
    tool_calls_count,
    duration_ms: start.elapsed().as_millis() as u64,
    child_thread_id: Some(spawn_child_thread_id.clone()),
}

// Err 分支 (~line 626)
BackgroundTaskResult {
    task_id: spawn_task_id.clone(),
    agent_name: spawn_agent_name.clone(),
    prompt_summary: spawn_prompt_summary.clone(),
    success: false,
    output: e.to_string(),
    tool_calls_count: 0,
    duration_ms: start.elapsed().as_millis() as u64,
    child_thread_id: Some(spawn_child_thread_id.clone()),
}
```

- [ ] **Step 5: 修改 background fork 路径的 BackgroundTaskResult 构造（define.rs ~830-860）**

同样模式，`spawn_child_thread_id` 已在作用域内：

```rust
// Ok 分支
child_thread_id: Some(spawn_child_thread_id.clone()),

// Err 分支
child_thread_id: Some(spawn_child_thread_id.clone()),
```

- [ ] **Step 6: 运行 cargo check 验证编译通过**

Run: `cargo check -p peri-agent -p peri-middlewares 2>&1 | tail -5`
Expected: 无错误

- [ ] **Step 7: Commit**

```bash
git add peri-agent/src/agent/events.rs peri-middlewares/src/subagent/tool/define.rs
git commit -m "feat(bg-agent): add child_thread_id to BackgroundTaskResult"
```

---

### Task 2: 修复 SubagentStarted 的 instance_id 使用 child_thread_id

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs` (line 695 + line 909)

**问题：** 后台路径的 `SubagentStarted` 使用 `task_id`（`bg-{uuid4}`）作为 `instance_id`，但 SQLite thread 使用 `bg_child_thread_id`（uuid7）。TUI 的 `RunningBgAgent.instance_id` 记录的是 `SubagentStarted.instance_id`，导致 `load_messages(instance_id)` 找不到 SQLite thread。

**注意：** `task_id` 仍用于 `BackgroundTaskRegistry`（`registry.register`）和 `BackgroundTaskResult.task_id`，不能移除。只改 `SubagentStarted.instance_id`。

- [ ] **Step 1: 修改 background 非 fork 路径的 SubagentStarted（define.rs ~693-697）**

将 `task_id.clone()` 改为 `bg_child_thread_id.clone()`：

```rust
// 修改前 (line 693-697):
if let Some(ref handler) = self.event_handler {
    handler.on_event(AgentEvent::SubagentStarted {
        agent_name: agent_name.clone(),
        instance_id: task_id.clone(),
        is_background: true,
    });
}

// 修改后:
if let Some(ref handler) = self.event_handler {
    handler.on_event(AgentEvent::SubagentStarted {
        agent_name: agent_name.clone(),
        instance_id: bg_child_thread_id.clone(),
        is_background: true,
    });
}
```

- [ ] **Step 2: 修改 background fork 路径的 SubagentStarted（define.rs ~907-911）**

将 `task_id.clone()` 改为 `bg_fork_child_thread_id.clone()`：

```rust
// 修改前:
handler.on_event(AgentEvent::SubagentStarted {
    agent_name: agent_name.clone(),
    instance_id: task_id.clone(),
    is_background: true,
});

// 修改后:
handler.on_event(AgentEvent::SubagentStarted {
    agent_name: agent_name.clone(),
    instance_id: bg_fork_child_thread_id.clone(),
    is_background: true,
});
```

- [ ] **Step 3: 检查 background 返回值文本中的 thread ID 引用（define.rs ~700-710, ~914-924）**

返回值文本中已使用 `bg_child_thread_id`（line 703）和 `bg_fork_child_thread_id`（line 917），这些是正确的。无需修改。

- [ ] **Step 4: 运行 cargo check**

Run: `cargo check -p peri-middlewares 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/subagent/tool/define.rs
git commit -m "fix(bg-agent): use child_thread_id as SubagentStarted instance_id

Background agents now emit child_thread_id (uuid7) instead of task_id
(bg-{uuid4}) as the instance_id in SubagentStarted events. This aligns
TUI's RunningBgAgent.instance_id with the SQLite thread ID, enabling
load_messages() to find the correct thread when focusing a background agent."
```

---

### Task 3: 更新 TUI BackgroundTaskCompleted 处理使用 child_thread_id 匹配

**Files:**
- Modify: `peri-tui/src/app/agent_events_bg.rs:52-314`
- Modify: `peri-tui/src/app/events.rs` (AgentEvent::BackgroundTaskCompleted 变体)

**问题：** `handle_background_task_completed` 当前使用 `agent_name` 匹配 SubAgentGroup，无法区分同名并发 agent。修复后 `BackgroundTaskCompleted` 事件携带 `child_thread_id`，可用于精确匹配 `instance_id`。

- [ ] **Step 1: 更新 TUI AgentEvent::BackgroundTaskCompleted 变体（events.rs）**

在 `BackgroundTaskCompleted` 变体中添加 `child_thread_id: Option<String>` 字段：

```rust
// 修改前:
BackgroundTaskCompleted {
    task_id: String,
    agent_name: String,
    success: bool,
    output: String,
    tool_calls_count: usize,
    duration_ms: u64,
},

// 修改后:
BackgroundTaskCompleted {
    task_id: String,
    agent_name: String,
    success: bool,
    output: String,
    tool_calls_count: usize,
    duration_ms: u64,
    child_thread_id: Option<String>,
},
```

- [ ] **Step 2: 更新 map_executor_event 中的 BackgroundTaskCompleted 映射（agent.rs）**

找到 `map_executor_event()` 中���射 `BackgroundTaskCompleted` 的位置，透传 `child_thread_id`：

```rust
// 修改前:
AgentEvent::BackgroundTaskCompleted { result } => {
    AgentEvent::BackgroundTaskCompleted {
        task_id: result.task_id,
        agent_name: result.agent_name,
        success: result.success,
        output: result.output,
        tool_calls_count: result.tool_calls_count,
        duration_ms: result.duration_ms,
    }
}

// 修改后:
AgentEvent::BackgroundTaskCompleted { result } => {
    AgentEvent::BackgroundTaskCompleted {
        task_id: result.task_id,
        agent_name: result.agent_name,
        success: result.success,
        output: result.output,
        tool_calls_count: result.tool_calls_count,
        duration_ms: result.duration_ms,
        child_thread_id: result.child_thread_id,
    }
}
```

- [ ] **Step 3: 更新 handle_background_task_completed 签名和调用点**

搜索 `handle_background_task_completed` 的所有调用点，添加 `child_thread_id` 参数。函数签名：

```rust
pub(crate) fn handle_background_task_completed(
    &mut self,
    task_id: String,
    agent_name: String,
    success: bool,
    output: String,
    tool_calls_count: usize,
    duration_ms: u64,
    child_thread_id: Option<String>,
) -> (bool, bool, bool)
```

更新 `handle_agent_event()` 中的调用（搜索 `BackgroundTaskCompleted` 分支）：

```rust
AgentEvent::BackgroundTaskCompleted {
    task_id,
    agent_name,
    success,
    output,
    tool_calls_count,
    duration_ms,
    child_thread_id,
} => {
    self.handle_background_task_completed(
        task_id, agent_name, success, output,
        tool_calls_count, duration_ms, child_thread_id,
    )
}
```

- [ ] **Step 4: 使用 child_thread_id 精确匹配 background_agents 移除**

在 `handle_background_task_completed` 中，`background_agents` 的移除逻辑当前用 `agent_name` 匹配（line 74-82）。改为优先用 `child_thread_id` 精确匹配：

```rust
// 修改前 (line 74-82):
if let Some(pos) = self.session_mgr.sessions[self.session_mgr.active]
    .background_agents
    .iter()
    .position(|a| a.agent_name == agent_name)
{
    ...
}

// 修改后:
let remove_pos = if let Some(ref ctid) = child_thread_id {
    // 精确匹配 instance_id
    self.session_mgr.sessions[self.session_mgr.active]
        .background_agents
        .iter()
        .position(|a| &a.instance_id == ctid)
} else {
    // 回退：按 agent_name 匹配（兼容旧版事件）
    self.session_mgr.sessions[self.session_mgr.active]
        .background_agents
        .iter()
        .position(|a| a.agent_name == agent_name)
};
if let Some(pos) = remove_pos {
    let removed = self.session_mgr.sessions[self.session_mgr.active]
        .background_agents
        .remove(pos);
    was_focused_by_id = session.focused_instance_id.as_deref() == Some(&removed.instance_id);
}
```

- [ ] **Step 5: 使用 child_thread_id 精确匹配 SubAgentGroup 更新**

将 view_messages 中的 SubAgentGroup 匹配从 `agent_name` 改为 `instance_id`（精确匹配）：

```rust
// 第一遍：精确匹配 instance_id
for vm in &mut session.messages.view_messages {
    if let MessageViewModel::SubAgentGroup {
        instance_id: vm_iid,
        is_running,
        is_background,
        total_steps,
        final_result,
        is_error,
        ..
    } = vm
    {
        if *is_background
            && *is_running
            && final_result.is_none()
            && vm_iid.as_deref() == child_thread_id.as_deref()
        {
            *is_running = false;
            *final_result = Some(output.clone());
            *is_error = !success;
            *total_steps = tool_calls_count;
            found_and_updated = true;
            break;
        }
    }
}

// 第二遍（兜底）：agent_name 匹配（兼容无 instance_id 的旧 SubAgentGroup）
// 保持原有逻辑
```

- [ ] **Step 6: 更新 was_focused 判断逻辑**

当前 `was_focused` 使用 `agent_name + instance_id` 交叉匹配（line 62-71），比较复杂。简化为直接检查 `child_thread_id` 是否等于 `focused_instance_id`：

```rust
let was_focused = child_thread_id.as_deref()
    == self.session_mgr.sessions[self.session_mgr.active]
        .focused_instance_id
        .as_deref();
```

- [ ] **Step 7: 运行 cargo check -p peri-tui**

Run: `cargo check -p peri-tui 2>&1 | tail -10`

- [ ] **Step 8: Commit**

```bash
git add peri-tui/src/app/agent_events_bg.rs peri-tui/src/app/events.rs peri-tui/src/app/agent.rs
git commit -m "fix(bg-agent): use child_thread_id for precise matching in BackgroundTaskCompleted

SubAgentGroup and background_agents now matched by instance_id (uuid7)
instead of agent_name, fixing concurrent same-type background agent matching."
```

---

### Task 4: 验证 agent_render.rs SQLite 加载路径

**Files:**
- Review: `peri-tui/src/app/agent_render.rs:41-62`

**目的：** Task 2 修复后，`focused_instance_id` 将持有 `child_thread_id`（uuid7），`resolve_render_vms()` 的 `load_messages(&tid)` 将正确找到 SQLite thread。此 Task 验证现有代码无需修改。

- [ ] **Step 1: 确认 resolve_render_vms 无需修改**

读取 `agent_render.rs:41-62`，确认：
- `focused_instance_id` 来源是 `RunningBgAgent.instance_id`
- `RunningBgAgent.instance_id` 现在来自 `SubagentStarted.instance_id`
- Task 2 将 `SubagentStarted.instance_id` 改为 `child_thread_id`
- `load_messages(child_thread_id)` 匹配 SQLite thread

**预期结论：** 现有代码无需修改，ID 对齐后自动工作。

- [ ] **Step 2: 检查 messages_to_view_models 对子 agent 消息的转换效果**

`messages_to_view_models` 将 `BaseMessage` 转为 `MessageViewModel`。后台 agent 的消息格式（Human/Ai/Tool）与主 agent 相同，转换应正常工作。确认 `cwd` 参数传递正确（`self.services.cwd`）。

- [ ] **Step 3: 记录验证结果**

如果无需修改，在 commit 信息中注明。

---

### Task 5: 更新 SubAgentGroup instance_id 在 pipeline 中的传递链路

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs` (SubAgentStart 事件处理)

**问题：** `SubAgentGroup.instance_id` 需要从 `SubAgentStart.instance_id` 正确传递。Task 2 修改了 `SubagentStarted` 的 `instance_id` 值（从 `task_id` 到 `child_thread_id`），需要确认 pipeline 的 `handle_event` 正确传递这个值。

- [ ] **Step 1: 检查 pipeline 中 SubAgentStart 事件处理**

在 `message_pipeline/mod.rs` 的 `handle_event` 中搜索 `SubAgentStart` 分支。确认 `instance_id` 从事件参数传递到 `SubAgentState`，再到 `SubAgentGroup` VM。

代码路径：
```
AgentEvent::SubAgentStart { instance_id, .. }
  → subagent_stack.push(SubAgentState { instance_id, .. })
  → build_tail_vms() → SubAgentGroup { instance_id: Some(instance_id), .. }
```

**预期：** instance_id 是透传的，修改事件来源后自动生效。无需修改 pipeline 代码。

- [ ] **Step 2: 确认 SubAgentGroup instance_id 类型为 Option<String>**

检查 `MessageViewModel::SubAgentGroup` 的 `instance_id` 字段类型。根据探索结果，它是 `Option<String>`。确认 pipeline 中的赋值逻辑使用 `Some(instance_id)`。

- [ ] **Step 3: 如果发现需要修改，提交修改**

---

### Task 6: 集成测试验证

**Files:**
- Modify: `peri-tui/src/cli_integration_test.rs` 或新建测试文件

**目的：** 验证 ID 链路修复后，聚焦后台 agent 时 `load_messages` 能正确加载消息。

- [ ] **Step 1: 运行现有测试**

Run: `cargo test -p peri-middlewares --lib -- subagent 2>&1 | tail -20`
Run: `cargo test -p peri-tui --lib -- background 2>&1 | tail -20`
Run: `cargo test -p peri-agent --lib -- thread 2>&1 | tail -20`

确保无回归。

- [ ] **Step 2: 检查 BackgroundTaskResult 序列化兼容性**

`BackgroundTaskResult` 新增了 `child_thread_id: Option<String>` 字段。由于是 `Option` 类型，旧的序列化数据（不含此字段）反序列化时会得到 `None`，向后兼容。

验证：在 `peri-agent` 中搜索 `BackgroundTaskResult` 的 `serde::Deserialize` 用法，确认 `Option` 字段的默认行为。

- [ ] **Step 3: 手动端到端验证（如果可以启动 TUI）**

测试步骤：
1. 启动 TUI，发送消息让 agent 启动后台 SubAgent
2. 观察后台 agent 启动后的 SubAgentGroup 显示
3. 按 `Ctrl+B` 打开 bg_agent_bar
4. 点击后台 agent 聚焦
5. 验证：消息流从 SQLite 正确加载并显示
6. 等待后台 agent 完成，验证完成通知和退出聚焦

---

### Task 7: 清理和诊断日志更新

**Files:**
- Modify: `peri-tui/src/app/agent_events_bg.rs` (diagnostic logs)
- Modify: `peri-middlewares/src/subagent/tool/define.rs` (diagnostic logs)

**目的：** 更新 `[bg-diag]` 日志以包含 `child_thread_id` 信息，方便后续排查。

- [ ] **Step 1: 更新 define.rs 中的 bg-diag 日志**

在 background agent spawn 后的 tracing::info 中添加 child_thread_id：

```rust
// ~line 700 附近，返回值文本之前
tracing::info!(
    task_id = %task_id,
    child_thread_id = %bg_child_thread_id,
    agent_name = %agent_name,
    "[bg-diag] background agent started"
);
```

- [ ] **Step 2: 更新 agent_events_bg.rs 中的 bg-diag 日志**

在 `handle_background_task_completed` 的诊断日志中添加 child_thread_id：

```rust
// ~line 91-99
tracing::info!(
    task_id = %task_id,
    child_thread_id = ?child_thread_id,
    agent_name = %agent_name,
    success = success,
    // ...existing fields...
    "[bg-diag] TUI: handle_background_task_completed called"
);
```

- [ ] **Step 3: 运行 cargo check 确认编译**

Run: `cargo check -p peri-middlewares -p peri-tui 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/subagent/tool/define.rs peri-tui/src/app/agent_events_bg.rs
git commit -m "chore(bg-agent): add child_thread_id to diagnostic logs"
```

---

## 自审清单

### Spec 覆盖检查

| Issue 文档要求 | 对应 Task |
|---------------|-----------|
| `instance_id` = `child_thread_id` 对齐 | Task 2 |
| `BackgroundTaskResult` 携带 `child_thread_id` | Task 1 |
| TUI 匹配逻辑使用精确 ID | Task 3 |
| `load_messages(instance_id)` 能找到 SQLite thread | Task 4 (验证无需改动) |
| SubAgentGroup instance_id 正确传递 | Task 5 (验证无需改动) |
| 诊断日志包含 child_thread_id | Task 7 |

### Placeholder 扫描

无 TBD/TODO/placeholder。所有步骤包含具体代码。

### 类型一致性检查

- `child_thread_id: Option<String>` — 在 `BackgroundTaskResult`、`AgentEvent::BackgroundTaskCompleted`、`handle_background_task_completed` 参数中一致使用 `Option<String>`
- `instance_id: String` — 在 `SubagentStarted` 事件、`SubAgentStart` AgentEvent、`RunningBgAgent`、`SubAgentGroup` 中一致使用 `String`
- `ThreadId = String` — `load_messages(id: &ThreadId)` 接受 `&String`，与 `instance_id` 类型兼容

### 遗留问题

此计划解决的是 **ID 不匹配** 这个根因。Issue 文档中提到的以下问题仍存在，但不在此计划范围内：

1. **drain_subagent_stack 后事件路由断裂**：后台 agent 的事件在 drain 后仍被丢弃（但聚焦查看时从 SQLite 加载完整消息，不依赖实时事件）
2. **bg_agent_bar 的实时 total_steps 更新**：drain 后 `find_total_steps()` 扫描 view_messages 获取过时值（但 SQLite 中的消息是完整的）
3. **frozen_subagent_vms 对后台 agent 的快照语义**：中期应引入 `bg_trackers` HashMap 替代
