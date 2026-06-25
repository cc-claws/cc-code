# 并发 SubAgent 工具调用路由修复 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复并发 SubAgent 时内部工具调用事件全部路由到最后一个 SubAgent 的问题，使每个 SubAgent 的工具调用记录正确显示在各自的 SubAgentGroup 中。

**Architecture:** 核心思路：利用每个子 Agent 的 `Agent` ToolStart 事件的 `tool_call_id` 作为"父级调用 ID"，在 TUI 事件映射层（`map_executor_event`）注入 `agent_id` 到所有后续子 Agent 事件中。Pipeline 侧通过 `agent_id` 直接匹配 `subagent_stack` 中的条目，而非 `last_mut()`。分两层修改：事件映射层添加 `source_agent_id` 字段，Pipeline 路由层改用精确查找。

**Tech Stack:** Rust, tokio async, peri-agent events, TUI message pipeline

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `peri-tui/src/app/events.rs` | TUI `AgentEvent` 枚举定义，添加可选 `source_agent_id` 字段 |
| `peri-tui/src/app/agent.rs` | `map_executor_event` 事件映射，注入 `source_agent_id` |
| `peri-tui/src/app/message_pipeline.rs` | Pipeline 路由逻辑，`last_mut()` → `find_by_agent_id` |

---

## 方案选择

**已排除方案：**
- `parking_lot::Mutex<HashMap>` 传递映射 → 死锁/hang
- 编码在 `format_subagent_result` 字符串后缀 → hang
- 在 `map_executor_event` 提取后缀 → hang

**选定方案：事件携带 `source_agent_id`**

在 TUI 的 `AgentEvent` 中为子 Agent 产生的事件（`ToolStart`/`ToolEnd`/`AssistantChunk`/`AiReasoning`/`Done`/`Interrupted`/`StateSnapshot`）添加可选字段 `source_agent_id: Option<String>`。当 TUI 事件映射层检测到子 Agent 正在运行时，通过一个栈变量 `active_agent_id` 将当前活跃的 `agent_id` 附加到事件上。

**关键洞察：** 子 Agent 是**顺序**被 `tool_dispatch` 并发调用的，但每个子 Agent 内部的事件是交错到达的。然而 `peri-agent` 的 `ExecutorEvent` 本身不区分来源——所有子 Agent 共享父 Agent 的 `event_handler`。因此需要在 TUI 映射层通过"当前活跃 tool_call_id → agent_id"的映射来确定事件归属。

**更精确的方案：** 使用 `tool_call_id` 前缀匹配。当 `Agent` 工具的 `ToolStart` 到达时，记录 `tool_call_id → agent_id` 映射；子 Agent 产生的 `ToolStart`/`ToolEnd` 的 `tool_call_id` 虽然不同，但我们可以通过"栈顶活跃 SubAgent 的 tool_call_id"来追踪。

**最终方案：** 在 `map_executor_event` 中维护一个 `active_subagent_tool_call_ids: HashMap<String, String>`（`tool_call_id → agent_id`）。当 `Agent` ToolStart 到达时插入映射；当子 Agent 的 ToolStart/ToolEnd/TextChunk 到达时，检查其 `message_id`（子 Agent 事件携带的 `message_id` 与父 Agent 的 `tool_call_id` 无关，**无法直接关联**）。

**最终最终方案（可行）：** 放弃从事件内容推导归属。改为在 `peri-agent` 层面的 `AgentEvent` 中添加 `source_agent_id` 字段。子 Agent 构建时注入唯一的 `agent_id`，通过 `FnEventHandler` 闭包捕获并在每个事件中附加。这是最干净的方案。

---

### Task 1: 在 peri-agent 的 AgentEvent 中添加 source_agent_id

**Files:**
- Modify: `peri-agent/src/agent/events.rs:16-89`

**背景：** `peri-agent` 的 `AgentEvent` 是核心事件类型，被 `AgentEventHandler::on_event` 消费。子 Agent 通过 `with_event_handler(Arc::clone(handler))` 透明转发事件。目前无法区分事件来自哪个子 Agent。

**方案：** 给需要区分的事件变体添加 `source_agent_id: Option<String>` 字段。默认为 `None`（父 Agent 事件），子 Agent 通过包装 handler 注入。

- [ ] **Step 1: 修改 AgentEvent 枚举，为关键事件变体添加 source_agent_id**

在 `peri-agent/src/agent/events.rs` 中，为以下变体添加 `source_agent_id: Option<String>` 字段：

```rust
/// 工具调用开始
ToolStart {
    message_id: crate::messages::MessageId,
    tool_call_id: String,
    name: String,
    input: serde_json::Value,
    /// 子 Agent 标识（None = 父 Agent 事件）
    source_agent_id: Option<String>,
},
/// 工具调用结束
ToolEnd {
    message_id: crate::messages::MessageId,
    tool_call_id: String,
    name: String,
    output: String,
    is_error: bool,
    source_agent_id: Option<String>,
},
/// AI 输出文字（非流式）
TextChunk {
    message_id: crate::messages::MessageId,
    chunk: String,
    source_agent_id: Option<String>,
},
/// AI 推理内容
AiReasoning(String),  // 保持不变（推理内容无需路由）
/// 状态快照
StateSnapshot(Vec<crate::messages::BaseMessage>),  // 保持不变
```

注意：`AiReasoning` 和 `StateSnapshot` 保持不变。`AiReasoning` 在 Pipeline 中仅用于 arm throttle（不路由到特定 SubAgent）。`StateSnapshot` 在 `in_subagent()` 时直接忽略。

- [ ] **Step 2: 更新所有 emit! 调用点，添加 source_agent_id: None**

搜索 `peri-agent/src/` 中所有 `AgentEvent::ToolStart {`、`AgentEvent::ToolEnd {`、`AgentEvent::TextChunk {` 的构造处，添加 `source_agent_id: None`。

主要位置：
- `tool_dispatch.rs` 中的 `dispatch_tools` 和 `collect_tool_results`
- `executor/mod.rs` 中的 TextChunk emit
- 其他可能 emit 这些事件的中间件

```bash
grep -rn "AgentEvent::ToolStart {" peri-agent/src/
grep -rn "AgentEvent::ToolEnd {" peri-agent/src/
grep -rn "AgentEvent::TextChunk {" peri-agent/src/
```

- [ ] **Step 3: 运行 cargo build 确认编译通过**

```bash
cargo build -p peri-agent
```

Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add peri-agent/src/agent/events.rs peri-agent/src/agent/executor/
git commit -m "feat(peri-agent): add source_agent_id field to ToolStart/ToolEnd/TextChunk events"
```

---

### Task 2: 创建子 Agent 事件包装器

**Files:**
- Modify: `peri-middlewares/src/subagent/tool.rs:144-172`

**背景：** `SubAgentTool` 的 `event_handler` 字段是 `Option<Arc<dyn AgentEventHandler>>`，直接 clone 给子 Agent 使用。需要包装这个 handler，在转发时注入 `source_agent_id`。

- [ ] **Step 1: 创建 SourceAgentIdHandler 包装器**

在 `peri-middlewares/src/subagent/tool.rs` 文件顶部（imports 后）添加：

```rust
/// 事件处理器包装器：为子 Agent 事件注入 source_agent_id
///
/// 当子 Agent 共享父 Agent 的 event_handler 时，事件无法区分来源。
/// 此包装器在转发前为 ToolStart/ToolEnd/TextChunk 事件注入 source_agent_id。
struct SourceAgentIdHandler {
    inner: Arc<dyn AgentEventHandler>,
    agent_id: String,
}

impl SourceAgentIdHandler {
    fn new(inner: Arc<dyn AgentEventHandler>, agent_id: String) -> Self {
        Self { inner, agent_id }
    }
}

impl AgentEventHandler for SourceAgentIdHandler {
    fn on_event(&self, event: AgentEvent) {
        let tagged = match event {
            AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                ..
            } => AgentEvent::ToolStart {
                message_id,
                tool_call_id,
                name,
                input,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                ..
            } => AgentEvent::ToolEnd {
                message_id,
                tool_call_id,
                name,
                output,
                is_error,
                source_agent_id: Some(self.agent_id.clone()),
            },
            AgentEvent::TextChunk {
                message_id,
                chunk,
                ..
            } => AgentEvent::TextChunk {
                message_id,
                chunk,
                source_agent_id: Some(self.agent_id.clone()),
            },
            // 其他事件（AiReasoning、StateSnapshot 等）原样转发
            other => other,
        };
        self.inner.on_event(tagged);
    }
}
```

- [ ] **Step 2: 在子 Agent 构建时使用包装器**

修改 `SubAgentTool::invoke`（Normal 路径）中注册 event_handler 的部分（约 line 884）：

```rust
// 旧代码：
// if let Some(handler) = &self.event_handler {
//     agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
// }

// 新代码：
if let Some(handler) = &self.event_handler {
    let tagged = Arc::new(SourceAgentIdHandler::new(
        Arc::clone(handler),
        agent_id.clone(),
    ));
    agent_builder = agent_builder.with_event_handler(tagged);
}
```

同样修改 `invoke_fork`（Fork 路径）中注册 event_handler 的部分（约 line 345）：

```rust
// 旧代码：
// if let Some(handler) = &self.event_handler {
//     agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
// }

// 新代码：
if let Some(handler) = &self.event_handler {
    let tagged = Arc::new(SourceAgentIdHandler::new(
        Arc::clone(handler),
        "fork".to_string(),
    ));
    agent_builder = agent_builder.with_event_handler(tagged);
}
```

**注意：** Background 路径不需要修改——background agent 已经注释说明不共享 event_handler。

- [ ] **Step 3: 同时修改 SubagentStarted/SubagentStopped 事件的 emit**

在 `invoke` Normal 路径中（约 line 892-916），`SubagentStarted` 和 `SubagentStopped` 是 `peri-agent` 层的事件，直接 emit 到 handler。这些事件目前不携带 `source_agent_id`（它们是生命周期事件，不需要路由到特定 SubAgent）。保持不变。

但需要确认 TUI 侧 `map_executor_event` 正确处理这些事件。查看 TUI events.rs 的 `SubagentLifecycle` 事件，它是独立的 TUI 事件，不经过 peri-agent 层。没有冲突。

- [ ] **Step 4: 运行 cargo build 确认编译通过**

```bash
cargo build -p peri-middlewares
```

Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add peri-middlewares/src/subagent/tool.rs
git commit -m "feat(subagent): wrap event_handler to inject source_agent_id for child agents"
```

---

### Task 3: 更新 TUI AgentEvent 添加 source_agent_id

**Files:**
- Modify: `peri-tui/src/app/events.rs:8-129`
- Modify: `peri-tui/src/app/agent.rs:567-650`

- [ ] **Step 1: 更新 TUI AgentEvent 枚举**

在 `peri-tui/src/app/events.rs` 中，为 `ToolStart`、`ToolEnd` 添加 `source_agent_id: Option<String>` 字段：

```rust
/// 工具调用开始（参数已就绪）
ToolStart {
    tool_call_id: String,
    name: String,
    display: String,
    args: String,
    input: serde_json::Value,
    /// 子 Agent 来源标识，用于并发 SubAgent 事件路由
    source_agent_id: Option<String>,
},
/// 工具调用结果
ToolEnd {
    tool_call_id: String,
    name: String,
    output: String,
    is_error: bool,
    source_agent_id: Option<String>,
},
```

`AssistantChunk` 不需要修改——在 Pipeline 中 `subagent_push_chunk` 也使用 `last_mut()`，但 chunk 事件没有 `source_agent_id`。我们需要在 `AssistantChunk` 中也添加 `source_agent_id`。

```rust
AssistantChunk {
    chunk: String,
    source_agent_id: Option<String>,
},
```

注意：TUI `AgentEvent` 和 `peri-agent` `AgentEvent` 是**不同的类型**。TUI 的 `AgentEvent` 在 `events.rs` 中定义，由 `map_executor_event` 从 `peri-agent` 的 `ExecutorEvent` 映射而来。

- [ ] **Step 2: 更新 map_executor_event 映射函数**

在 `peri-tui/src/app/agent.rs` 的 `map_executor_event` 中，更新映射以传递 `source_agent_id`：

```rust
// Agent ToolStart → SubAgentStart（在通用 ToolStart 分支之前）
ExecutorEvent::ToolStart { name, input, .. } if name == "Agent" => {
    let agent_id = input
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("fork")
        .to_string();
    // ... 其余不变
    AgentEvent::SubAgentStart {
        agent_id,
        task_preview,
        is_background,
    }
}
// 通用 ToolStart
ExecutorEvent::ToolStart {
    tool_call_id,
    name,
    input,
    source_agent_id,
    ..
} => AgentEvent::ToolStart {
    tool_call_id,
    name: name.clone(),
    display: format_tool_name(&name),
    args: format_tool_args(&name, &input, Some(cwd)).unwrap_or_default(),
    input: input.clone(),
    source_agent_id,
},
// Agent ToolEnd → SubAgentEnd
ExecutorEvent::ToolEnd {
    name,
    output,
    is_error,
    source_agent_id,
    ..
} if name == "Agent" => AgentEvent::SubAgentEnd {
    result: output,
    is_error,
},
// 通用 ToolEnd
ExecutorEvent::ToolEnd {
    tool_call_id,
    name,
    output,
    is_error,
    source_agent_id,
    ..
} => AgentEvent::ToolEnd {
    tool_call_id,
    name,
    output,
    is_error,
    source_agent_id,
},
// TextChunk
ExecutorEvent::TextChunk { chunk: text, source_agent_id, .. } => AgentEvent::AssistantChunk {
    chunk: text,
    source_agent_id,
},
```

- [ ] **Step 3: 更新 TUI 中所有构造 AgentEvent::ToolStart/ToolEnd/AssistantChunk 的地方**

搜索 TUI 代码中所有构造这些变体的位置：

```bash
grep -rn "AgentEvent::ToolStart {" peri-tui/src/
grep -rn "AgentEvent::ToolEnd {" peri-tui/src/
grep -rn "AgentEvent::AssistantChunk" peri-tui/src/
```

所有构造处添加 `source_agent_id: None`。

- [ ] **Step 4: 运行 cargo build 确认编译通过**

```bash
cargo build -p peri-tui
```

Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/events.rs peri-tui/src/app/agent.rs
git commit -m "feat(tui): add source_agent_id to TUI AgentEvent for subagent routing"
```

---

### Task 4: 修改 Pipeline 路由逻辑

**Files:**
- Modify: `peri-tui/src/app/message_pipeline.rs:195-270`, `:475-515`, `:594-600`

**背景：** 这是核心修复。将所有 `subagent_stack.last_mut()` 替换为按 `source_agent_id` 查找的方法。

- [ ] **Step 1: 添加 find_subagent_by_id 辅助方法**

在 `MessagePipeline` 的 `impl` 块中添加：

```rust
/// 根据 source_agent_id 查找 subagent_stack 中的对应 SubAgent
fn find_subagent_by_id_mut(
    &mut self,
    agent_id: &str,
) -> Option<&mut SubAgentState> {
    self.subagent_stack
        .iter_mut()
        .find(|s| s.agent_id == agent_id)
}

/// 判断指定 agent_id 的 SubAgent 是否正在运行
fn is_subagent_running(&self, agent_id: &str) -> bool {
    self.subagent_stack
        .iter()
        .any(|s| s.agent_id == agent_id && s.is_running && !s.is_background)
}
```

- [ ] **Step 2: 修改 handle_event 中的事件路由**

将 `handle_event` 中的路由逻辑从 `in_subagent()` + `last_mut()` 改为基于 `source_agent_id` 的路由：

```rust
AgentEvent::AssistantChunk { chunk, source_agent_id } => {
    if !chunk.is_empty() {
        if let Some(ref aid) = source_agent_id {
            if let Some(sub) = self.find_subagent_by_id_mut(aid) {
                self.subagent_push_chunk_to(sub, &chunk);
            }
        } else if self.in_subagent() {
            // 兼容：无 source_agent_id 时回退到 last_mut()
            if let Some(sub) = self.subagent_stack.last_mut() {
                self.subagent_push_chunk_to(sub, &chunk);
            }
        } else {
            self.push_chunk(&chunk);
        }
        self.throttle_armed = true;
    }
    vec![PipelineAction::None]
}
AgentEvent::AiReasoning(text) => {
    if self.in_subagent() {
        self.throttle_armed = true;
    } else {
        self.push_reasoning(&text);
        self.throttle_armed = true;
    }
    vec![PipelineAction::None]
}
AgentEvent::ToolStart {
    tool_call_id,
    name,
    display: _,
    args: _,
    input,
    source_agent_id,
} => {
    self.throttle_armed = false;
    if let Some(ref aid) = source_agent_id {
        if let Some(sub) = self.find_subagent_by_id_mut(aid) {
            self.subagent_tool_start_to(sub, &tool_call_id, &name, input);
        }
    } else if self.in_subagent() {
        // 兼容回退
        if let Some(sub) = self.subagent_stack.last_mut() {
            self.subagent_tool_start_to(sub, &tool_call_id, &name, input);
        }
    } else {
        self.tool_start_internal(&tool_call_id, &name, input, false);
    }
    vec![PipelineAction::None]
}
AgentEvent::ToolEnd {
    tool_call_id,
    name,
    output,
    is_error,
    source_agent_id,
} => {
    self.throttle_armed = false;
    if let Some(ref aid) = source_agent_id {
        if let Some(sub) = self.find_subagent_by_id_mut(aid) {
            self.subagent_tool_end_to(sub, &tool_call_id, &output, is_error);
        }
    } else if self.in_subagent() {
        // 兼容回退
        if let Some(sub) = self.subagent_stack.last_mut() {
            self.subagent_tool_end_to(sub, &tool_call_id, &output, is_error);
        }
    } else {
        self.tool_end_internal(&tool_call_id, &name, &output, is_error);
    }
    vec![PipelineAction::None]
}
```

- [ ] **Step 3: 重构 subagent 方法为接受 &mut SubAgentState 参数**

将 `subagent_tool_start`、`subagent_push_chunk` 和 `ToolEnd` 更新逻辑重构为接受显式 `&mut SubAgentState` 参数：

```rust
/// SubAgent 内部工具调用（路由进指定 SubAgentGroup）
fn subagent_tool_start_to(
    &mut self,
    sub: &mut SubAgentState,
    tool_call_id: &str,
    name: &str,
    input: serde_json::Value,
) {
    let display = tool_display::format_tool_name(name);
    let args = tool_display::format_tool_args(name, &input, Some(&self.cwd));
    let vm = MessageViewModel::tool_block_with_id(
        tool_call_id.to_string(),
        name.to_string(),
        display,
        args,
        false,
    );
    sub.total_steps += 1;
    if sub.recent_messages.len() >= 4 {
        sub.recent_messages.remove(0);
    }
    sub.recent_messages.push(vm);
}

/// SubAgent 内部 chunk（路由进指定 SubAgentGroup）
fn subagent_push_chunk_to(&mut self, sub: &mut SubAgentState, chunk: &str) {
    match sub.recent_messages.last_mut() {
        Some(m) if m.is_assistant() => m.append_chunk(chunk),
        _ => {
            sub.total_steps += 1;
            if sub.recent_messages.len() >= 4 {
                sub.recent_messages.remove(0);
            }
            let mut bubble = MessageViewModel::assistant();
            bubble.append_chunk(chunk);
            sub.recent_messages.push(bubble);
        }
    }
}

/// SubAgent 内部 ToolEnd 更新（路由进指定 SubAgentGroup）
fn subagent_tool_end_to(
    &mut self,
    sub: &mut SubAgentState,
    tool_call_id: &str,
    output: &str,
    is_error: bool,
) {
    for vm in sub.recent_messages.iter_mut().rev() {
        if let MessageViewModel::ToolBlock {
            tool_call_id: tc_id,
            content,
            is_error: err,
            ..
        } = vm
        {
            if tc_id == tool_call_id {
                *content = output.to_string();
                *err = is_error;
                break;
            }
        }
    }
}
```

删除旧的 `subagent_tool_start`、`subagent_push_chunk` 方法（如果还存在旧的调用者，保留并标记 deprecated）。

- [ ] **Step 4: 保留 tool_end_internal 中的 SubAgentEnd 路由**

`tool_end_internal` 中处理 `Agent` ToolEnd 的逻辑（约 line 429-434）需要保留，因为它处理 SubAgentGroup 的 `finalized_vm` 构建。此处使用 `last_mut()` 是正确的——因为 `SubAgentEnd` 事件会正确匹配到栈顶（同一轮中 Agent ToolEnd 是顺序返回的）。

检查并确认 `tool_end_internal` 中 Agent 工具的 `last_mut()` 是否需要改为精确匹配。查看代码：

```rust
if name == "Agent" {
    if let Some(sub) = self.subagent_stack.last_mut() {
        // 处理 SubAgentEnd...
    }
}
```

`SubAgentEnd` 事件在 `map_executor_event` 中由 `ExecutorEvent::ToolEnd { name: "Agent", .. }` 映射而来。在 Pipeline 中，`SubAgentEnd` 走的是 `tool_end_internal` 路径（因为 TUI 的 `SubAgentEnd` 不携带 `source_agent_id`，它是通过 `SubAgentStart` 的 `agent_id` 来关联的）。

查看当前 SubAgentEnd 处理路径：

```rust
AgentEvent::SubAgentEnd { result, is_error } => {
    let tc_id = self
        .subagent_stack
        .last()
        .map(|s| format!("subagent_{}", s.agent_id))
        .unwrap_or_else(|| "subagent_end".to_string());
    self.tool_end_internal(&tc_id, "Agent", &result, is_error);
    vec![PipelineAction::None]
}
```

这里 `last()` 取的是栈顶 SubAgent。**在并发场景下这也有问题**——如果 SubAgent A 先完成但 SubAgent B 后完成，SubAgent A 的 End 事件到达时栈顶可能是 B。

**修复：** `SubAgentEnd` 需要携带 `agent_id` 来精确匹配。但 `SubAgentEnd` 是从 `ExecutorEvent::ToolEnd { name: "Agent" }` 映射的，而该事件已有 `source_agent_id`。需要将 `source_agent_id` 传递到 `SubAgentEnd` 中。

在 TUI events.rs 中修改 `SubAgentEnd`：

```rust
/// SubAgent 执行结束（由 Agent ToolEnd 映射而来）
SubAgentEnd {
    agent_id: Option<String>,
    result: String,
    is_error: bool,
},
```

在 `map_executor_event` 中：

```rust
ExecutorEvent::ToolEnd {
    name,
    output,
    is_error,
    source_agent_id,
    ..
} if name == "Agent" => AgentEvent::SubAgentEnd {
    agent_id: source_agent_id,
    result: output,
    is_error,
},
```

在 Pipeline 的 `handle_event` 中修改 `SubAgentEnd` 处理：

```rust
AgentEvent::SubAgentEnd { agent_id, result, is_error } => {
    let tc_id = if let Some(ref aid) = agent_id {
        // 精确匹配
        self.subagent_stack
            .iter()
            .find(|s| s.agent_id == *aid)
            .map(|s| format!("subagent_{}", s.agent_id))
            .unwrap_or_else(|| "subagent_end".to_string())
    } else {
        // 兼容回退
        self.subagent_stack
            .last()
            .map(|s| format!("subagent_{}", s.agent_id))
            .unwrap_or_else(|| "subagent_end".to_string())
    };
    self.tool_end_internal(&tc_id, "Agent", &result, is_error);
    vec![PipelineAction::None]
}
```

- [ ] **Step 5: 同样修复 SubAgentStart 的并发问题**

`SubAgentStart` 在 Pipeline 中通过 `tool_start_internal` 注册 pending_tool，此时会 push 到 `subagent_stack`。查看当前代码：

```rust
AgentEvent::SubAgentStart {
    agent_id,
    task_preview,
    is_background,
} => {
    let input = serde_json::json!({"subagent_type": &agent_id, "prompt": &task_preview});
    let tc_id = format!("subagent_{}", agent_id);
    self.tool_start_internal(&tc_id, "Agent", input, is_background);
    vec![PipelineAction::None]
}
```

`tool_start_internal` 内部 push 到 `pending_tools`，但在 `SubAgentEnd → tool_end_internal` 中会通过 `pending_tools.remove` 查找。这里 `tc_id = format!("subagent_{}", agent_id)` 是唯一的（因为每个子 Agent 的 `agent_id` 不同），所以 `tool_start_internal` 和 `tool_end_internal` 之间通过唯一的 `tc_id` 正确关联，不需要修改。

**确认：** `tool_start_internal` 中调用 `self.subagent_push_pending_to_stack` 将 pending SubAgent push 到 stack。需要检查这个方法是否也使用 `last_mut()`：

```bash
grep -n "subagent_push_pending_to_stack" peri-tui/src/app/message_pipeline.rs
```

如果没有这个方法名，搜索 push 到 `subagent_stack` 的位置。

- [ ] **Step 6: 检查 `in_subagent()` 方法**

当前 `in_subagent()` 只检查栈顶是否正在运行。在并发场景下，如果 SubAgent A 运行中但 B 也运行中，`in_subagent()` 返回 true。这对于 `Done`、`Interrupted`、`StateSnapshot` 事件的过滤是正确的（这些事件在子 Agent 运行时都应被忽略）。保持不变。

- [ ] **Step 7: 运行 cargo build 确认编译通过**

```bash
cargo build -p peri-tui
```

Expected: 编译成功

- [ ] **Step 8: Commit**

```bash
git add peri-tui/src/app/message_pipeline.rs peri-tui/src/app/events.rs peri-tui/src/app/agent.rs
git commit -m "fix(tui): route concurrent subagent events by source_agent_id instead of last_mut()"
```

---

### Task 5: 处理 ACP 模式的事件映射

**Files:**
- Modify: `peri-tui/src/acp/event_mapper.rs`（如果存在）

- [ ] **Step 1: 搜索 ACP 事件映射代码**

```bash
find peri-tui/src/acp -name "*.rs" | head -20
```

检查 ACP 模式是否也有类似的事件映射需要更新。如果 ACP 使用相同的 `AgentEvent` 类型，那么 `event_mapper.rs` 中的映射函数也需要传递 `source_agent_id`。

- [ ] **Step 2: 更新 ACP 事件映射（如适用）**

如果 ACP 有自己的事件映射，添加 `source_agent_id` 传递。如果 ACP 复用 `map_executor_event`，则无需额外修改。

- [ ] **Step 3: 运行 cargo build 确认**

```bash
cargo build -p peri-tui
```

---

### Task 6: 验证修复——编译 + 手动测试

**Files:** 无新文件

- [ ] **Step 1: 全量编译**

```bash
cargo build
```

Expected: 所有 crate 编译成功

- [ ] **Step 2: 运行现有测试**

```bash
cargo test -p peri-agent
cargo test -p peri-middlewares
cargo test -p peri-tui --lib
```

Expected: 所有测试通过

- [ ] **Step 3: 手动测试——并发 SubAgent 工具调用路由**

1. 启动 TUI：`cargo run -p peri-tui`
2. 让父 Agent 在同一轮中并发调用 2 个不同类型的 Agent 工具
3. SubAgent 全部完成后，展开各 SubAgentGroup
4. **验证：** 每个 SubAgentGroup 内部都有完整的工具调用记录

- [ ] **Step 4: 手动测试——单个 SubAgent 仍正常工作**

1. 启动 TUI
2. 让父 Agent 调用 1 个 Agent 工具
3. **验证：** SubAgent 展开后有完整的工具调用记录

- [ ] **Step 5: Commit（如有修复）**

```bash
git add -A
git commit -m "fix: address test failures from subagent routing changes"
```

---

### Task 7: 清理 + 最终验证

**Files:** 无新文件

- [ ] **Step 1: 运行 clippy**

```bash
cargo clippy -p peri-agent -p peri-middlewares -p peri-tui -- -D warnings
```

Expected: 无 warning

- [ ] **Step 2: 运行 fmt**

```bash
cargo fmt --check
```

Expected: 无差异

- [ ] **Step 3: 全量测试**

```bash
cargo test
```

Expected: 所有测试通过

---

## Self-Review 检查清单

**1. Spec 覆盖：**
- 并发 SubAgent 工具调用路由错误 → Task 4（核心修复）
- 背景色移除 → 已在之前 commit 修复，本计划不涉及
- `last_mut()` 路由问题 → Task 4 中全面替换
- SubAgentEnd 精确匹配 → Task 4 Step 4

**2. Placeholder 扫描：**
- 无 TBD/TODO
- 所有代码步骤包含完整代码
- 所有命令包含预期输出

**3. 类型一致性：**
- `source_agent_id: Option<String>` 在 peri-agent `AgentEvent`、TUI `AgentEvent`、`SubAgentEnd` 三处类型一致
- `SourceAgentIdHandler` 使用 `Arc<dyn AgentEventHandler>` 与现有接口匹配
- `find_subagent_by_id_mut` 返回 `Option<&mut SubAgentState>` 与旧 `last_mut()` 返回类型匹配
