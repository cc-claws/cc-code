# 并发 SubAgent 唯一实例 ID 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让并发同类型 SubAgent 拥有唯一实例 ID，修复事件路由到错误卡片的问题。

**Architecture:** 在 `SubAgentTool::invoke()` 内生成 `instance_id`（UUID），通过 `SubagentStarted`/`SubagentStopped` 事件传递到 TUI 映射层。TUI 映射层从 `SubagentStarted`（而非 `ToolStart`）创建 `SubAgentState`，用 `instance_id` 替代 `subagent_type` 作为路由 key。`SourceAgentIdHandler` 同步使用 `instance_id`，使子 Agent 事件能精确路由到对应的 `SubAgentState`。

**Tech Stack:** Rust, tokio, uuid

---

## 文件结构

| 操作 | 文件 | 职责 |
|------|------|------|
| 修改 | `peri-agent/src/agent/events.rs` | `SubagentStarted`/`SubagentStopped` 增加 `instance_id` 字段 |
| 修改 | `peri-middlewares/src/subagent/tool/define.rs` | `invoke()`/`invoke_fork()`/`invoke_background()` 生成并传递 `instance_id` |
| 修改 | `peri-tui/src/app/events.rs` | `SubAgentStart`/`SubAgentEnd` 增加 `instance_id` 字段 |
| 修改 | `peri-tui/src/app/agent.rs` | 映射层：`SubagentStarted` → `SubAgentStart`（创建 SubAgentState），`ToolStart(name="Agent")` → 普通 ToolStart |
| 修改 | `peri-tui/src/app/message_pipeline/mod.rs` | `SubAgentState` 增加 `instance_id`，路由/匹配/pending_tools 全部改用 `instance_id` |
| 修改 | `peri-tui/src/app/agent_ops.rs` | 透传 `instance_id` 到 pipeline |
| 修改 | `peri-tui/src/app/message_pipeline/message_pipeline_test.rs` | 更新现有测试 + 新增回归测试 |

---

### Task 1: 扩展 `SubagentStarted`/`SubagentStopped` 事件（peri-agent）

**Files:**
- Modify: `peri-agent/src/agent/events.rs:84-90`

- [ ] **Step 1: 给 `SubagentStarted` 和 `SubagentStopped` 增加 `instance_id` 字段**

```rust
// events.rs:84 — SubagentStarted 变体
/// 子 agent 开始执行
SubagentStarted {
    agent_name: String,
    /// 唯一实例标识符（用于并发同类型 SubAgent 路由）
    instance_id: String,
},
// events.rs:86 — SubagentStopped 变体
/// 子 agent 执行完成
SubagentStopped {
    agent_name: String,
    result: String,
    is_error: bool,
    /// 唯一实例标识符
    instance_id: String,
},
```

- [ ] **Step 2: 搜索所有 `SubagentStarted`/`SubagentStopped` 的构造点，确认编译**

Run: `cargo build -p peri-agent 2>&1 | head -40`

Expected: 编译错误指向所有需要更新 `SubagentStarted`/`SubagentStopped` 构造的位置。

- [ ] **Step 3: 修复所有构造点（添加 `instance_id` 字段）**

除了 `define.rs`（Task 2 会处理），其他构造点先用空字符串占位（测试文件等）。搜索：

```bash
grep -rn "SubagentStarted" --include="*.rs" | grep -v "define.rs"
grep -rn "SubagentStopped" --include="*.rs" | grep -v "define.rs"
```

对每个结果添加 `instance_id: String::new()` 或 `instance_id: "test".into()`。

- [ ] **Step 4: 验证 `peri-agent` 编译通过**

Run: `cargo build -p peri-agent 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(agent): add instance_id field to SubagentStarted/Stopped events"
```

---

### Task 2: `SubAgentTool::invoke()` 生成并传递 `instance_id`（peri-middlewares）

**Files:**
- Modify: `peri-middlewares/src/subagent/tool/define.rs:691-822`（invoke 普通路径）
- Modify: `peri-middlewares/src/subagent/tool/define.rs:205-303`（invoke_fork 路径）
- Modify: `peri-middlewares/src/subagent/tool/define.rs:305-479`（invoke_background 路径）
- Modify: `peri-middlewares/src/subagent/tool/define.rs:481-600`（invoke_background_fork 路径）

- [ ] **Step 1: 在 `invoke()` 正常路径中生成 `instance_id` 并传递**

在 `define.rs` 的 `invoke()` 方法中（约第 691 行，`let agent_id = match &subagent_type {...}` 之后），添加：

```rust
// 在 agent_id 赋值之后（约第 699 行后）添加：
let instance_id = format!("sub_{}", &uuid::Uuid::new_v4().to_string()[..8]);
```

修改 `SourceAgentIdHandler` 创建（约第 749-754 行）：

```rust
// 修改前：
} else if let Some(handler) = &self.event_handler {
    let tagged = Arc::new(SourceAgentIdHandler::new(
        Arc::clone(handler),
        agent_id.clone(),
    ));
    agent_builder = agent_builder.with_event_handler(tagged);
}

// 修改后：
} else if let Some(handler) = &self.event_handler {
    let tagged = Arc::new(SourceAgentIdHandler::new(
        Arc::clone(handler),
        instance_id.clone(),  // 用 instance_id 替代 agent_id
    ));
    agent_builder = agent_builder.with_event_handler(tagged);
}
```

修改 `SubagentStarted` 发射（约第 760 行）：

```rust
// 修改前：
handler.on_event(AgentEvent::SubagentStarted {
    agent_name: agent_id.clone(),
});

// 修改后：
handler.on_event(AgentEvent::SubagentStarted {
    agent_name: agent_id.clone(),
    instance_id: instance_id.clone(),
});
```

修改 `SubagentStopped` 发射（约第 799 行）：

```rust
// 修改前：
handler.on_event(AgentEvent::SubagentStopped {
    agent_name: agent_id.clone(),
    result: output_summary.clone(),
    is_error: stopped_is_error,
});

// 修改后：
handler.on_event(AgentEvent::SubagentStopped {
    agent_name: agent_id.clone(),
    result: output_summary.clone(),
    is_error: stopped_is_error,
    instance_id: instance_id.clone(),
});
```

- [ ] **Step 2: 在 `invoke_fork()` 路径中做同样修改**

在 `invoke_fork()` 方法中（约第 205 行），在 LLM 创建之后添加：

```rust
let instance_id = format!("sub_{}", &uuid::Uuid::new_v4().to_string()[..8]);
```

修改 `SourceAgentIdHandler` 创建（约第 239-244 行）：

```rust
// 修改前：
let tagged = Arc::new(SourceAgentIdHandler::new(
    Arc::clone(handler),
    "fork".to_string(),
));

// 修改后：
let tagged = Arc::new(SourceAgentIdHandler::new(
    Arc::clone(handler),
    instance_id.clone(),  // 用唯一 ID 替代硬编码 "fork"
));
```

修改 `SubagentStarted`（约第 248 行）：

```rust
handler.on_event(AgentEvent::SubagentStarted {
    agent_name: "fork".to_string(),
    instance_id: instance_id.clone(),
});
```

修改 `SubagentStopped`（约第 279 行）：

```rust
handler.on_event(AgentEvent::SubagentStopped {
    agent_name: "fork".to_string(),
    result: output_summary.clone(),
    is_error: stopped_is_error,
    instance_id: instance_id.clone(),
});
```

- [ ] **Step 3: 在 `invoke_background()` 和 `invoke_background_fork()` 路径中做同样修改**

两个方法遵循相同模式：
1. 方法开头生成 `instance_id`
2. `SourceAgentIdHandler` 使用 `instance_id`（仅 `invoke_background` 有这个分支，因为 background spawn 了 tokio task，但 `invoke_background` 没有 `SourceAgentIdHandler`——background 路径通过 `child_handler_factory` 或直接用 `event_handler`）

对 `invoke_background()`：
- 在 `let agent_name = agent_id.clone();` 之后（约第 360 行）添加 `let instance_id = format!("sub_{}", &uuid::Uuid::new_v4().to_string()[..8]);`
- `SubagentStarted` 发射处添加 `instance_id: instance_id.clone()`
- 在 `tokio::spawn` 内部的 `BackgroundTaskCompleted` 之后（如有 `SubagentStopped` 发射）添加 `instance_id`。注意：background 路径的 `SubagentStopped` 通过 `fire_subagent_lifecycle_hooks_static` 发射 hooks，但没有通过 `event_handler` 发射 `AgentEvent::SubagentStopped`。检查是否有遗漏。

对 `invoke_background_fork()`：
- 同上模式

- [ ] **Step 4: 验证 `peri-middlewares` 编译通过**

Run: `cargo build -p peri-middlewares 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(subagent): generate unique instance_id in SubAgentTool::invoke()"
```

---

### Task 3: 扩展 TUI `SubAgentStart`/`SubAgentEnd` 事件

**Files:**
- Modify: `peri-tui/src/app/events.rs:67-77`

- [ ] **Step 1: 给 `SubAgentStart` 和 `SubAgentEnd` 增加 `instance_id` 字段**

```rust
// events.rs — SubAgentStart
/// SubAgent 开始执行（由 SubagentStarted 映射而来，携带唯一实例 ID）
SubAgentStart {
    agent_id: String,
    /// 唯一实例标识符（并发同类型 SubAgent 路由用）
    instance_id: String,
    task_preview: String,
    is_background: bool,
},
// events.rs — SubAgentEnd
/// SubAgent 执行结束
SubAgentEnd {
    result: String,
    is_error: bool,
    agent_id: Option<String>,
    /// 唯一实例标识符
    instance_id: Option<String>,
},
```

- [ ] **Step 2: 搜索所有 `AgentEvent::SubAgentStart` 和 `AgentEvent::SubAgentEnd` 的构造/匹配点**

Run: `grep -rn "SubAgentStart\|SubAgentEnd" peri-tui/src/ --include="*.rs"`
对每个匹配点添加 `instance_id` 字段。大部分是测试文件，用 `"test-instance".into()` 或 `None` 占位。

- [ ] **Step 3: 验证 TUI 编译**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 编译成功（因为映射层和 pipeline 还没改，构造点已用占位值填充）

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): add instance_id to SubAgentStart/SubAgentEnd events"
```

---

### Task 4: 重构 TUI 映射层——`SubagentStarted` 创建 `SubAgentState`

**Files:**
- Modify: `peri-tui/src/app/agent.rs:29-49`（ToolStart Agent 映射）
- Modify: `peri-tui/src/app/agent.rs:157-170`（SubagentStarted/Stopped 映射）

这是核心改动。当前 `ToolStart(name="Agent")` → `SubAgentStart`（创建 SubAgentState），需要改为 `SubagentStarted` → `SubAgentStart`（创建 SubAgentState）。

- [ ] **Step 1: 修改 `ToolStart(name="Agent")` 映射——改为普通 ToolStart**

```rust
// agent.rs — 替换第 29-49 行
// 修改前：
// ExecutorEvent::ToolStart { name, input, .. } if name == "Agent" => {
//     ...
//     AgentEvent::SubAgentStart { agent_id, task_preview, is_background }
// }

// 修改后：移除这个 match arm，让 ToolStart(name="Agent") 走普通 ToolStart 分支
// （直接删除第 29-49 行的整个 match arm）
```

删除后，`ToolStart(name="Agent")` 会匹配到后面的通用 `ExecutorEvent::ToolStart` 分支（第 50-63 行），生成普通 `AgentEvent::ToolStart`。

- [ ] **Step 2: 修改 `SubagentStarted` 映射——改为 `SubAgentStart`**

```rust
// agent.rs — 替换第 157-161 行
// 修改前：
ExecutorEvent::SubagentStarted { agent_name } => AgentEvent::SubagentLifecycle {
    agent_name,
    started: true,
},

// 修改后：
ExecutorEvent::SubagentStarted { agent_name, instance_id } => {
    // SubagentStarted 携带唯一 instance_id，是创建 SubAgentState 的权威事件源。
    // task_preview 和 is_background 从 instance_id 上下文无法获取，
    // 但 pipeline 的 tool_start_internal 会从 SubAgentStart 事件中提取这些信息。
    // 此处我们仍然需要 agent_name 作为 display name。
    // 注意：SubAgentStart 仍需要 task_preview 和 is_background。
    // 方案：先发送 SubAgentStart（pipeline 会创建 SubAgentState），
    //       然后额外发送 SubagentLifecycle 用于 spinner。
    // 但由于我们不再从 ToolStart 获取 task_preview，
    // 需要在 SubagentStarted 事件中携带这些信息，或在此处留空。
    //
    // 最简方案：SubAgentStart 的 task_preview 留空字符串，
    // 实际 preview 已在 ToolStart(name="Agent") 时通过 pending_tool 展示。
    AgentEvent::SubAgentStart {
        agent_id: agent_name.clone(),
        instance_id,
        task_preview: String::new(),  // preview 已在 ToolStart 中展示
        is_background: false,         // 从后续事件推断
    }
}
```

等一下——`SubagentStarted` 不携带 `is_background` 信息。但看 `define.rs`，background agent 走 `invoke_background()` 路径，它不通过 `SubagentStarted` 发射事件给 pipeline（background agent 通过 `BackgroundTaskCompleted` 通知）。所以 `SubagentStarted` 永远来自前台 agent，`is_background` 始终为 `false`。

- [ ] **Step 3: 修改 `SubagentStopped` 映射——改为 `SubAgentEnd`**

```rust
// agent.rs — 替换第 163-170 行
// 修改前：
ExecutorEvent::SubagentStopped {
    agent_name,
    result,
    is_error,
} => AgentEvent::SubAgentEnd {
    agent_id: Some(agent_name),
    result,
    is_error,
},

// 修改后：
ExecutorEvent::SubagentStopped {
    agent_name,
    result,
    is_error,
    instance_id,
} => AgentEvent::SubAgentEnd {
    agent_id: Some(agent_name),
    instance_id: Some(instance_id),
    result,
    is_error,
},
```

- [ ] **Step 4: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 编译成功（pipeline 层还没改，但类型已对齐）

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor(tui): map SubagentStarted→SubAgentStart with instance_id"
```

---

### Task 5: Pipeline 改用 `instance_id` 路由

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:84-97`（`SubAgentState`）
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:286-296`（`SubAgentStart` 处理）
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:297-322`（`SubAgentEnd` 处理）
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:393-449`（`tool_start_internal`）
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:451-510`（`tool_end_internal`）
- Modify: `peri-tui/src/app/message_pipeline/mod.rs:576-581`（`find_running_subagent_mut`）

- [ ] **Step 1: `SubAgentState` 增加 `instance_id` 字段**

```rust
// mod.rs — SubAgentState 结构体（第 84 行）
pub(crate) struct SubAgentState {
    agent_id: String,       // subagent_type，仅用于显示
    instance_id: String,    // 唯一实例标识符，用于路由
    task_preview: String,
    total_steps: usize,
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    finalized_vm: Option<MessageViewModel>,
    is_background: bool,
    bg_hash: Option<String>,
}
```

- [ ] **Step 2: `SubAgentStart` 处理——传递 `instance_id` 到 `tool_start_internal`**

```rust
// mod.rs — SubAgentStart 分支（第 286 行）
AgentEvent::SubAgentStart {
    agent_id,
    instance_id,
    task_preview,
    is_background,
} => {
    let input =
        serde_json::json!({"subagent_type": &agent_id, "prompt": &task_preview});
    // 用 instance_id 作为 tc_id，保证唯一
    self.tool_start_internal(&instance_id, "Agent", input, is_background);
    vec![PipelineAction::None]
}
```

- [ ] **Step 3: `SubAgentEnd` 处理——用 `instance_id` 匹配**

```rust
// mod.rs — SubAgentEnd 分支（第 297 行）
AgentEvent::SubAgentEnd {
    result,
    is_error,
    agent_id: _,
    instance_id,
} => {
    let tc_id = if let Some(ref iid) = instance_id {
        // 按 instance_id 精确查找 RUNNING 的 SubAgent
        self.subagent_stack
            .iter()
            .find(|s| s.instance_id == *iid && s.is_running)
            .map(|s| s.instance_id.clone())
            .unwrap_or_else(|| "subagent_end".to_string())
    } else {
        // 防御性回退
        self.subagent_stack
            .last()
            .map(|s| s.instance_id.clone())
            .unwrap_or_else(|| "subagent_end".to_string())
    };
    self.tool_end_internal(&tc_id, "Agent", &result, is_error);
    vec![PipelineAction::None]
}
```

- [ ] **Step 4: `tool_start_internal` —— `SubAgentState` 存 `instance_id`，用 `instance_id` 做 key**

```rust
// mod.rs — tool_start_internal 中 name == "Agent" 分支（第 404 行）
if name == "Agent" {
    let agent_id = input["subagent_type"]
        .as_str()
        .unwrap_or("Agent")
        .to_string();
    let task_preview: String = input["prompt"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(40)
        .collect();
    // tool_call_id 现在是 instance_id（唯一）
    let instance_id = tool_call_id.to_string();
    self.subagent_stack.push(SubAgentState {
        agent_id: agent_id.clone(),
        instance_id: instance_id.clone(),
        task_preview: task_preview.clone(),
        total_steps: 0,
        recent_messages: Vec::new(),
        is_running: true,
        finalized_vm: None,
        is_background,
        bg_hash: Some(instance_hash(&instance_id)),  // 用 instance_id 生成唯一 hash
    });
    // 批次检测逻辑不变...
```

`pending_tools.insert` 的 key 现在是 `instance_id`（即 `tool_call_id` 参数），已自动唯一。

- [ ] **Step 5: `tool_end_internal` —— 用 `instance_id` 匹配 SubAgent**

```rust
// mod.rs — tool_end_internal 中 name == "Agent" 分支（第 459 行）
if name == "Agent" {
    // tool_call_id 现在是 instance_id，直接用它匹配
    if let Some(sub) = self
        .subagent_stack
        .iter_mut()
        .find(|s| s.instance_id == tool_call_id && s.is_running)
    {
        // 冻结逻辑不变...
        // 但把所有 s.agent_id 改为 s.agent_id（display name），s.instance_id 用于匹配
    }
    // 批次检测逻辑不变...
}
```

注意：`tool_end_internal` 内部使用 `target_agent_id = tool_call_id.strip_prefix("subagent_")` 的逻辑需要删除。现在 `tool_call_id` 就是 `instance_id`，直接用 `find(|s| s.instance_id == tool_call_id && s.is_running)` 匹配。

- [ ] **Step 6: `find_running_subagent_mut` —— 改用 `instance_id` 匹配**

```rust
// mod.rs — find_running_subagent_mut（第 577 行）
fn find_running_subagent_mut(&mut self, instance_id: &str) -> Option<&mut SubAgentState> {
    self.subagent_stack
        .iter_mut()
        .find(|s| s.instance_id == instance_id && s.is_running)
}
```

**重要**：所有调用 `find_running_subagent_mut` 的地方传入的参数是 `source_agent_id`（来自 `SourceAgentIdHandler`），现在这个值是 `instance_id`（唯一），不再是 `subagent_type`。所以匹配逻辑自然正确。

- [ ] **Step 7: `drain_subagent_stack` 中同样使用 `instance_id`**

`drain_subagent_stack` 中构建 `MessageViewModel::SubAgentGroup` 时，`agent_id` 字段用于显示名称，保持从 `sub.agent_id`（即 `subagent_type`）取值。`instance_id` 不需要暴露到 ViewModel。

检查 `MessageViewModel::SubAgentGroup` 中是否有 `instance_id` 相关字段——如果没有，不需要改。如果有 `agent_id` 字段，确保它仍然使用 `sub.agent_id`（display name）。

- [ ] **Step 8: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | tail -10`
Expected: 编译成功

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "fix(pipeline): route SubAgent events by unique instance_id instead of agent type"
```

---

### Task 6: 更新 `agent_ops.rs` 透传 `instance_id`

**Files:**
- Modify: `peri-tui/src/app/agent_ops.rs:199-239`（SubAgentStart 处理）
- Modify: `peri-tui/src/app/agent_ops.rs:267-322`（SubAgentEnd 处理）

- [ ] **Step 1: `SubAgentStart` 处理——透传 `instance_id`**

```rust
// agent_ops.rs — SubAgentStart 分支（第 199 行）
AgentEvent::SubAgentStart {
    agent_id,
    instance_id,
    task_preview,
    is_background,
} => {
    // ... existing logic unchanged (subagent_depth, langfuse tracer) ...
    // Langfuse tracer 的 on_subagent_start 仍用 agent_id（display name），不受影响
    tracer.lock().on_subagent_start(&agent_id, &task_preview);

    // Pipeline 透传 instance_id
    let actions = self.session_mgr.sessions[self.session_mgr.active]
        .messages
        .pipeline
        .handle_event(AgentEvent::SubAgentStart {
            agent_id,
            instance_id,
            task_preview,
            is_background,
        });
    // ... rest unchanged ...
}
```

只需要在 match arm 解构中加上 `instance_id` 并透传即可。

- [ ] **Step 2: `SubAgentEnd` 处理——透传 `instance_id`**

```rust
// agent_ops.rs — SubAgentEnd 分支（第 267 行）
AgentEvent::SubAgentEnd {
    result,
    is_error,
    agent_id,
    instance_id,  // 新增
} => {
    // ... existing logic unchanged ...
    let actions = self.session_mgr.sessions[self.session_mgr.active]
        .messages
        .pipeline
        .handle_event(AgentEvent::SubAgentEnd {
            result,
            is_error,
            agent_id,
            instance_id,  // 透传
        });
    // ... rest unchanged ...
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "fix(agent_ops): pass through instance_id in SubAgentStart/End"
```

---

### Task 7: 更新测试 + 新增回归测试

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`
- Modify: `peri-middlewares/src/subagent/tool/tool_test.rs`

- [ ] **Step 1: 更新所有现有测试中的 `SubAgentStart` 构造**

所有 `AgentEvent::SubAgentStart { agent_id, task_preview, is_background }` 添加 `instance_id` 字段。对已有测试，用 `agent_id.clone()` 作为 `instance_id`（因为测试中没有并发场景，用类型名作为 ID 即可）。

搜索并更新：

```bash
grep -n "SubAgentStart {" peri-tui/src/app/message_pipeline/message_pipeline_test.rs
```

每个构造点添加 `instance_id: "<unique-test-id>".into(),`。对于并发测试 `test_concurrent_subagents_route_by_source_agent_id`，使用不同的 `instance_id`。

- [ ] **Step 2: 更新 `test_concurrent_subagents_route_by_source_agent_id` 测试**

```rust
#[test]
fn test_concurrent_subagents_route_by_source_agent_id() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 启动两个并发 SubAgent（不同 instance_id，相同 agent_id/type）
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "explore".into(),
        instance_id: "sub_abc12345".into(),  // 唯一 ID
        task_preview: "task a".into(),
        is_background: false,
    });
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "explore".into(),           // 相同类型
        instance_id: "sub_def67890".into(),   // 不同唯一 ID
        task_preview: "task b".into(),
        is_background: false,
    });

    // Agent A 的 ToolStart → source_agent_id = instance_id
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_a1".into(),
        name: "Read".into(),
        display: "Read".into(),
        args: "a.rs".into(),
        input: json!({"file_path": "/tmp/a.rs"}),
        source_agent_id: Some("sub_abc12345".into()),  // 用 instance_id 路由
    });
    // Agent B 的 ToolStart → source_agent_id = instance_id
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_b1".into(),
        name: "Grep".into(),
        display: "Grep".into(),
        args: "pattern".into(),
        input: json!({"pattern": "fn main"}),
        source_agent_id: Some("sub_def67890".into()),  // 用 instance_id 路由
    });

    // 验证：两个 agent 各自收到了正确的工具调用
    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_abc12345")
        .unwrap();
    assert_eq!(sub_a.recent_messages.len(), 1);
    if let MessageViewModel::ToolBlock { tool_name, .. } = &sub_a.recent_messages[0] {
        assert_eq!(tool_name, "Read", "explore 实例 A 应包含 Read 工具调用");
    } else {
        panic!("实例 A 的 recent_messages[0] 应为 ToolBlock");
    }

    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_def67890")
        .unwrap();
    assert_eq!(sub_b.recent_messages.len(), 1);
    if let MessageViewModel::ToolBlock { tool_name, .. } = &sub_b.recent_messages[0] {
        assert_eq!(tool_name, "Grep", "explore 实例 B 应包含 Grep 工具调用");
    } else {
        panic!("实例 B 的 recent_messages[0] 应为 ToolBlock");
    }

    // ToolEnd 路由验证
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_a1".into(),
        name: "Read".into(),
        output: "content of a".into(),
        is_error: false,
        source_agent_id: Some("sub_abc12345".into()),
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_b1".into(),
        name: "Grep".into(),
        output: "match in b".into(),
        is_error: false,
        source_agent_id: Some("sub_def67890".into()),
    });

    // 验证 ToolEnd 结果路由正确
    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_abc12345")
        .unwrap();
    if let MessageViewModel::ToolBlock { content, .. } = &sub_a.recent_messages[0] {
        assert_eq!(content, "content of a", "实例 A 的 ToolEnd 应路由到实例 A");
    }

    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_def67890")
        .unwrap();
    if let MessageViewModel::ToolBlock { content, .. } = &sub_b.recent_messages[0] {
        assert_eq!(content, "match in b", "实例 B 的 ToolEnd 应路由到实例 B");
    }

    // AssistantChunk 精确路由
    let _ = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "chunk for a".into(),
        source_agent_id: Some("sub_abc12345".into()),
    });
    let _ = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "chunk for b".into(),
        source_agent_id: Some("sub_def67890".into()),
    });

    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_abc12345")
        .unwrap();
    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_def67890")
        .unwrap();
    assert_eq!(sub_a.recent_messages.len(), 2, "实例 A 应有 2 个 recent_message");
    assert_eq!(sub_b.recent_messages.len(), 2, "实例 B 应有 2 个 recent_message");
}
```

- [ ] **Step 3: 新增回归测试——同类型并发 SubAgent 显示不同 ID**

```rust
/// 回归测试：并发同类型 SubAgent 应生成不同 bg_hash
#[test]
fn test_concurrent_same_type_subagents_have_different_hash() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "explore".into(),
        instance_id: "sub_aaa11111".into(),
        task_preview: "task a".into(),
        is_background: false,
    });
    let _ = pipeline.handle_event(AgentEvent::SubAgentStart {
        agent_id: "explore".into(),
        instance_id: "sub_bbb22222".into(),
        task_preview: "task b".into(),
        is_background: false,
    });

    let sub_a = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_aaa11111")
        .unwrap();
    let sub_b = pipeline
        .subagent_stack
        .iter()
        .find(|s| s.instance_id == "sub_bbb22222")
        .unwrap();

    // 两个实例的 bg_hash 应不同
    assert_ne!(
        sub_a.bg_hash, sub_b.bg_hash,
        "同类型并发 SubAgent 应有不同 bg_hash"
    );
    // agent_id 应相同（都是 "explore"，用于显示）
    assert_eq!(sub_a.agent_id, sub_b.agent_id);
}
```

- [ ] **Step 4: 更新 headless 测试中的 `SubAgentStart` 构造**

搜索 `peri-tui/src/ui/headless_test.rs` 中所有 `SubAgentStart` 构造，添加 `instance_id` 字段（用 `"test-instance".into()` 即可）。

- [ ] **Step 5: 运行全部测试验证**

Run: `cargo test -p peri-tui --lib -- message_pipeline 2>&1 | tail -20`
Expected: 所有 pipeline 测试通过

Run: `cargo test -p peri-middlewares --lib -- subagent 2>&1 | tail -10`
Expected: 所有 subagent 测试通过

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "test: update tests for instance_id routing, add regression test for same-type concurrent SubAgents"
```

---

### Task 8: 全量编译 + 集成验证

**Files:**
- 无新文件

- [ ] **Step 1: 全量编译**

Run: `cargo build 2>&1 | tail -10`
Expected: 全部 crate 编译成功

- [ ] **Step 2: 全量测试**

Run: `cargo test 2>&1 | tail -30`
Expected: 全部测试通过

- [ ] **Step 3: 更新 issue 状态**

将 `spec/issues/2026-05-19-concurrent-subagent-duplicate-id.md` 状态改为 `Fixed`，添加修复提交。

- [ ] **Step 4: Final commit**

```bash
git add -A && git commit -m "fix(subagent): concurrent same-type SubAgents now have unique instance IDs for event routing"
```
