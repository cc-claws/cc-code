# 增量 Hash：MessageViewModel 内联 content_hash 消除 rebuild() 全量重算

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `MessageViewModel` 中新增 `content_hash: u64` 字段，构造和内容变更时自动计算 hash。`RenderTask::rebuild()` 直接读取 `vm.content_hash()` 而非每次重新计算，消除前缀稳定区的重复 hash 开销。

**Architecture:** 三阶段改造：(1) `MessageViewModel` 增加 `content_hash` 字段和 `recompute_hash()` 方法；(2) 所有构造点和变更点调用 `recompute_hash()`；(3) `rebuild()` 替换 `compute_hash()` 为 `vm.content_hash()` 读取。

**Tech Stack:** Rust, std::hash::{Hash, Hasher, DefaultHasher}

---

## 分析

### 当前 hash 计算路径

```
rebuild()
  → messages.iter().map(Self::compute_hash).collect()   // 每条消息全量重算
    → vm.hash(&mut DefaultHasher::new())
```

即使前缀区消息完全未变，每次 `Rebuild`/`Resize`/`ToggleDiff` 仍需遍历全部消息计算 hash 用于 prefix_stable_len 比较。200+ 条消息时开销可观。

### 优化策略

- `content_hash` 在 `MessageViewModel` **构造时**一次性计算（复用现有 `Hash` impl）
- **变更点**（`append_chunk`、`is_streaming = false`、`is_running = false` 等）调用 `recompute_hash()` 增量更新
- `rebuild()` 中的 `new_hashes` 直接从 `vm.content_hash()` 收集，O(1) per message 而非 O(content_size)

### 变更点清单

| 变更点 | 文件 | 触发方式 |
|--------|------|----------|
| 所有工厂方法 (`user()`, `assistant()`, `tool_block()` 等) | `message_view/mod.rs` | 构造后 `recompute_hash()` |
| `from_base_message_with_cwd()` | `message_view/mod.rs` | 构造后 `recompute_hash()` |
| `append_chunk()` | `message_view/mod.rs` | 修改后 `recompute_hash()` |
| `build_streaming_bubble()` | `message_pipeline/transform.rs` | 构造后 `recompute_hash()` |
| `build_tool_start_vm()` | `message_pipeline/transform.rs` | 构造后 `recompute_hash()` |
| `build_tail_vms()` 中的 `SubAgentGroup` 构造 | `message_pipeline/reconcile.rs` | 构造后 `recompute_hash()` |
| `completed_tools` → `ToolBlock` 构造 | `message_pipeline/reconcile.rs` | 构造后 `recompute_hash()` |
| `drain_subagent_stack()` | `message_pipeline/mod.rs` | 构造后 `recompute_hash()` |
| `tool_end_internal()` → `frozen_vm` | `message_pipeline/mod.rs` | 构造后 `recompute_hash()` |
| `lifecycle.rs` → `is_streaming = false` | `agent_ops/lifecycle.rs` | 修改后 `recompute_hash()` |
| `agent_events_bg.rs` → `is_running = false` 等 | `agent_events_bg.rs` | 修改后 `recompute_hash()` |
| `message_render.rs` → `state.collapsed` | `message_render.rs` | **无需**（此 collapsed 是 widget state，不回写 VM） |
| `aggregate_tool_groups()` / `aggregate_batch_groups()` | `message_view/aggregate.rs` | 变更后 `recompute_hash()` |

---

### Task 1: 在 MessageViewModel 中新增 content_hash 字段

**Files:**
- Modify: `peri-tui/src/ui/message_view/mod.rs`

- [ ] **Step 1: 在 `MessageViewModel` enum 外部新增 `content_hash` 字段 — 使用 newtype wrapper**

`MessageViewModel` 是 enum，不能直接加字段。需要将其包装在一个 struct 中：

```rust
/// 渲染层的视图模型，从 BaseMessage/AgentEvent 转换而来。
/// 包含语义 content_hash，在构造/变更时自动计算。
#[derive(Debug, Clone)]
pub struct MessageViewModel {
    /// 语义 hash，涵盖所有影响渲染输出的字段
    content_hash: u64,
    /// 实际内容
    inner: MessageViewModelInner,
}
```

将现有 `MessageViewModel` enum 重命名为 `MessageViewModelInner`（`pub(crate)` 可见性），外层 `MessageViewModel` struct 保持 `pub`。

**但是**，这会导致所有 match 解构都需要改写为 `vm.inner => ...`，改动面太大。

**替代方案（推荐）：** 使用单独的 `content_hash` 字段与一个 `Cell` 或直接在每次变更后手动更新。由于 `MessageViewModel` 已经是 `#[derive(Debug, Clone)]`，且大部分使用模式是构造后推入 vec、或 `&mut` 修改，我们采用 **struct-of-enum 模式但最小化改动**：

实际上，仔细看代码，`MessageViewModel` 是一个 enum，所有外部代码都直接 match 它。包装成 struct 会改动数百处。

**最终方案：在每个变体中加 `content_hash: u64` 字段。**

虽然每个变体都要加一个 `u64` 字段（8 字节），但避免了所有 match 解构的改动（match `..` 已经忽略了未列出的字段）。这是最小侵入方案。

修改 `MessageViewModel` 的每个变体，新增 `content_hash: u64` 字段：

```rust
pub enum MessageViewModel {
    UserBubble {
        content: String,
        rendered: Text<'static>,
        content_hash: u64,  // 新增
    },
    AssistantBubble {
        blocks: Vec<ContentBlockView>,
        is_streaming: bool,
        collapsed: bool,
        content_hash: u64,  // 新增
    },
    ToolBlock {
        tool_name: String,
        tool_call_id: String,
        display_name: String,
        args_display: Option<String>,
        content: String,
        is_error: bool,
        collapsed: bool,
        color: Color,
        diff_lines: Option<Vec<Line<'static>>>,
        content_hash: u64,  // 新增
    },
    SystemNote {
        content: String,
        content_hash: u64,  // 新增
    },
    CacheWarning {
        content: String,
        content_hash: u64,  // 新增
    },
    ToolCallGroup {
        category: ToolCategory,
        tools: Vec<ToolEntry>,
        collapsed: bool,
        content_hash: u64,  // 新增
    },
    SubAgentGroup {
        agent_id: String,
        task_preview: String,
        total_steps: usize,
        recent_messages: Vec<MessageViewModel>,
        is_running: bool,
        collapsed: bool,
        final_result: Option<String>,
        is_error: bool,
        is_background: bool,
        bg_hash: Option<String>,
        batch_agents: Vec<AgentSummary>,
        instance_id: Option<String>,
        content_hash: u64,  // 新增
    },
}
```

- [ ] **Step 2: 新增 `content_hash()` getter 和 `recompute_hash()` 方法**

在 `impl MessageViewModel` 块中添加：

```rust
impl MessageViewModel {
    /// 返回预计算的语义 hash
    pub fn content_hash(&self) -> u64 {
        match self {
            MessageViewModel::UserBubble { content_hash, .. } => *content_hash,
            MessageViewModel::AssistantBubble { content_hash, .. } => *content_hash,
            MessageViewModel::ToolBlock { content_hash, .. } => *content_hash,
            MessageViewModel::SystemNote { content_hash, .. } => *content_hash,
            MessageViewModel::CacheWarning { content_hash, .. } => *content_hash,
            MessageViewModel::ToolCallGroup { content_hash, .. } => *content_hash,
            MessageViewModel::SubAgentGroup { content_hash, .. } => *content_hash,
        }
    }

    /// 重新计算语义 hash（内容变更后调用）
    pub fn recompute_hash(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        let hash = hasher.finish();
        match self {
            MessageViewModel::UserBubble { content_hash, .. } => *content_hash = hash,
            MessageViewModel::AssistantBubble { content_hash, .. } => *content_hash = hash,
            MessageViewModel::ToolBlock { content_hash, .. } => *content_hash = hash,
            MessageViewModel::SystemNote { content_hash, .. } => *content_hash = hash,
            MessageViewModel::CacheWarning { content_hash, .. } => *content_hash = hash,
            MessageViewModel::ToolCallGroup { content_hash, .. } => *content_hash = hash,
            MessageViewModel::SubAgentGroup { content_hash, .. } => *content_hash = hash,
        }
    }
}
```

**注意：** `recompute_hash()` 调用 `self.hash()`，而 `Hash` impl 不能包含 `content_hash` 字段本身（否则循环依赖）。因此需要修改 `Hash` impl，确保它 **不** 写入 `content_hash`。

- [ ] **Step 3: 修改 `Hash` impl，排除 `content_hash` 字段**

当前 `Hash` impl 已经手动列举每个变体的语义字段。`content_hash` 作为 `..` 匹配的额外字段，在 `Hash` impl 中不会被访问（因为每个 match arm 都明确列出了参与 hash 的字段）。但需要确认 `rendered` 和 `color` 仍不参与 hash。

检查现有 `Hash` impl：每个变体的 match arm 只 hash 语义字段，`rendered`/`color` 已经被排除。`content_hash` 同理不会被列入 hash 字段列表。**无需修改 `Hash` impl。**

- [ ] **Step 4: 修改 `PartialEq` impl，排除 `content_hash` 字段**

现有 `PartialEq` impl 的每个 match arm 只比较语义字段，不包含 `content_hash`。由于 `content_hash` 是派生字段（从语义字段计算而来），无需比较。**无需修改 `PartialEq` impl。**

- [ ] **Step 5: 编写 `content_hash` 计算的单元测试**

在 `peri-tui/src/ui/message_view/message_view_test.rs` 末尾添加：

```rust
// ── content_hash 测试 ──

#[test]
fn test_content_hash_user_bubble_consistent() {
    let vm = MessageViewModel::user("Hello".to_string());
    // 相同内容构造两次，hash 一致
    let vm2 = MessageViewModel::user("Hello".to_string());
    assert_eq!(vm.content_hash(), vm2.content_hash(), "相同内容的 hash 应一致");
}

#[test]
fn test_content_hash_user_bubble_different() {
    let vm1 = MessageViewModel::user("Hello".to_string());
    let vm2 = MessageViewModel::user("World".to_string());
    assert_ne!(vm1.content_hash(), vm2.content_hash(), "不同内容的 hash 应不同");
}

#[test]
fn test_content_hash_system_note() {
    let vm = MessageViewModel::system("test note".to_string());
    assert_ne!(vm.content_hash(), 0, "hash 不应为 0（几乎不可能碰撞）");
}

#[test]
fn test_content_hash_matches_compute_hash() {
    // 验证 content_hash 与 RenderTask::compute_hash 结果一致
    let vm = MessageViewModel::user("test content".to_string());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    vm.hash(&mut hasher);
    let expected = hasher.finish();
    assert_eq!(vm.content_hash(), expected, "content_hash 应与手动 compute_hash 一致");
}

#[test]
fn test_content_hash_assistant_bubble_changes_on_blocks() {
    let mut vm = MessageViewModel::assistant();
    let hash1 = vm.content_hash();
    vm.append_chunk("new text");
    // append_chunk 内部会 recompute_hash
    assert_ne!(vm.content_hash(), hash1, "append_chunk 后 hash 应变化");
}

#[test]
fn test_content_hash_assistant_bubble_changes_on_streaming() {
    let mut vm = MessageViewModel::assistant();
    let hash_streaming = vm.content_hash();
    // 模拟 is_streaming 变化
    if let MessageViewModel::AssistantBubble { is_streaming, .. } = &mut vm {
        *is_streaming = false;
    }
    vm.recompute_hash();
    assert_ne!(vm.content_hash(), hash_streaming, "is_streaming 变化后 hash 应不同");
}

#[test]
fn test_content_hash_subagent_group() {
    let vm = MessageViewModel::subagent_group("explorer".to_string(), "explore".to_string());
    assert_ne!(vm.content_hash(), 0);
}

#[test]
fn test_content_hash_cache_warning() {
    let vm = MessageViewModel::cache_warning("low cache".to_string());
    assert_ne!(vm.content_hash(), 0);
}

#[test]
fn test_recompute_hash_idempotent() {
    let mut vm = MessageViewModel::user("stable".to_string());
    let h1 = vm.content_hash();
    vm.recompute_hash();
    let h2 = vm.content_hash();
    assert_eq!(h1, h2, "未修改内容时 recompute_hash 应幂等");
}
```

- [ ] **Step 6: 运行测试验证基础结构**

```bash
cargo test -p peri-tui --lib message_view_test -- --nocapture
```

Expected: 所有新增 test PASS（此时工厂方法会编译失败，需要在 Task 2 中修复）。

---

### Task 2: 修改所有构造点 — 工厂方法

**Files:**
- Modify: `peri-tui/src/ui/message_view/mod.rs`

所有工厂方法在构造 enum variant 时新增 `content_hash: 0`，然后调用 `recompute_hash()`。

- [ ] **Step 1: 修改 `user()` 工厂方法**

```rust
pub fn user(content: String) -> Self {
    let mut vm = MessageViewModel::UserBubble {
        content,
        rendered: Text::raw(""),
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 2: 修改 `assistant()` 工厂方法**

```rust
pub fn assistant() -> Self {
    let mut vm = MessageViewModel::AssistantBubble {
        blocks: Vec::new(),
        is_streaming: true,
        collapsed: false,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 3: 修改 `tool_block()` 和 `tool_block_with_id()` 工厂方法**

```rust
pub fn tool_block(
    tool_name: String,
    display: String,
    args: Option<String>,
    is_error: bool,
) -> Self {
    Self::tool_block_with_id(String::new(), tool_name, display, args, is_error)
}

pub fn tool_block_with_id(
    tool_call_id: String,
    tool_name: String,
    display: String,
    args: Option<String>,
    is_error: bool,
) -> Self {
    let color = if is_error {
        theme::ERROR
    } else {
        tool_color(&tool_name)
    };
    let mut vm = MessageViewModel::ToolBlock {
        tool_call_id,
        tool_name,
        display_name: display,
        args_display: args,
        content: String::new(),
        is_error,
        collapsed: true,
        color,
        diff_lines: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 4: 修改 `system()` 工厂方法**

```rust
pub fn system(content: String) -> Self {
    let mut vm = MessageViewModel::SystemNote { content, content_hash: 0 };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 5: 修改 `cache_warning()` 工厂方法**

```rust
pub fn cache_warning(content: String) -> Self {
    let mut vm = MessageViewModel::CacheWarning { content, content_hash: 0 };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 6: 修改 `subagent_group()` 工厂方法**

```rust
pub fn subagent_group(agent_id: String, task_preview: String) -> Self {
    let mut vm = MessageViewModel::SubAgentGroup {
        agent_id,
        task_preview,
        total_steps: 0,
        recent_messages: Vec::new(),
        is_running: true,
        collapsed: false,
        final_result: None,
        is_error: false,
        is_background: false,
        bg_hash: None,
        batch_agents: Vec::new(),
        instance_id: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 7: 修改 `from_base_message_with_cwd()` — 所有构造分支**

在 `from_base_message_with_cwd()` 函数中，每个构造 `MessageViewModel` 变体的地方：
- 新增 `content_hash: 0` 字段
- 在 `return` 前调用 `recompute_hash()`

具体修改点：

1. `BaseMessage::Human` 分支 — `UserBubble` 构造
2. `BaseMessage::Ai` 分支 — `AssistantBubble` 构造
3. `BaseMessage::Tool` 分支 — `Agent` 工具路径的 `return MessageViewModel::SubAgentGroup { ... }`
4. `BaseMessage::Tool` 分支 — 非 Agent 工具的 `MessageViewModel::ToolBlock { ... }`
5. `BaseMessage::System` 分支 — `SystemNote` 构造

每个修改模式相同，例如 UserBubble：

```rust
BaseMessage::Human { content, .. } => {
    let raw = content.text_content();
    let mut vm = MessageViewModel::UserBubble {
        content: raw,
        rendered: Text::raw(""),
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

对于 `return MessageViewModel::SubAgentGroup { ... }` 情况（Agent 工具路径）：

```rust
let mut vm = MessageViewModel::SubAgentGroup {
    agent_id,
    task_preview,
    total_steps: parse_subagent_tool_count(&raw_content),
    recent_messages: Vec::new(),
    is_running: false,
    collapsed: false,
    final_result: Some(raw_content),
    is_error: *is_error,
    is_background,
    bg_hash,
    batch_agents: Vec::new(),
    instance_id: None,
    content_hash: 0,
};
vm.recompute_hash();
return vm;
```

- [ ] **Step 8: 修改 `append_chunk()` — 变更后 recompute_hash**

```rust
pub fn append_chunk(&mut self, chunk: &str) {
    if let MessageViewModel::AssistantBubble {
        blocks, collapsed, ..
    } = self
    {
        if *collapsed && !chunk.is_empty() {
            *collapsed = false;
        }
        if let Some(ContentBlockView::Text { raw, dirty, .. }) = blocks.last_mut() {
            raw.push_str(chunk);
            *dirty = true;
            self.recompute_hash();
            return;
        }
        let mut raw = String::new();
        raw.push_str(chunk);
        blocks.push(ContentBlockView::Text {
            raw,
            rendered: Text::raw(""),
            dirty: true,
            rendered_prefix_len: 0,
            rendered_prefix_lines: 0,
        });
        self.recompute_hash();
    }
}
```

- [ ] **Step 9: 运行 message_view_test 验证**

```bash
cargo test -p peri-tui --lib message_view_test -- --nocapture
```

Expected: 所有测试通过。

---

### Task 3: 修改 MessagePipeline 中的构造点

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/transform.rs`
- Modify: `peri-tui/src/app/message_pipeline/reconcile.rs`
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 修改 `build_streaming_bubble()` (transform.rs)**

```rust
pub fn build_streaming_bubble(&self) -> MessageViewModel {
    let mut vm = MessageViewModel::AssistantBubble {
        blocks,
        is_streaming: true,
        collapsed: false,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 2: 修改 `build_tool_start_vm()` (transform.rs)**

```rust
pub(crate) fn build_tool_start_vm(
    &self,
    tool_call_id: &str,
    name: &str,
    input: &serde_json::Value,
) -> MessageViewModel {
    let display_name = tool_display::format_tool_name(name);
    let args_display = tool_display::format_tool_args(name, input, Some(&self.cwd));
    let mut vm = MessageViewModel::ToolBlock {
        tool_name: name.to_string(),
        tool_call_id: tool_call_id.to_string(),
        display_name,
        args_display,
        content: String::new(),
        is_error: false,
        collapsed: true,
        color: tool_color(name),
        diff_lines: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 3: 修改 `build_tail_vms()` 中的所有 SubAgentGroup 构造 (reconcile.rs)**

`reconcile.rs` 中有多处 `MessageViewModel::SubAgentGroup { ... }` 构造，每处都需要：
- 新增 `content_hash: 0`
- 构造后调用 `recompute_hash()`

涉及位置：
1. 行 ~244: `completed_tools` → `ToolBlock` 构造
2. 行 ~274: `has_snapshot_this_round` 中的 SubAgentGroup（`sub.finalized_vm.is_none()` 分支）
3. 行 ~295-311: `else` 分支中的 SubAgentGroup 构造

每个构造的修改模式：

```rust
let mut vm = MessageViewModel::ToolBlock {
    tool_name: ct.name.clone(),
    tool_call_id: ct.tool_call_id.clone(),
    display_name: display,
    args_display: args,
    content: ct.output.clone(),
    is_error: ct.is_error,
    collapsed: true,
    color: if ct.is_error { theme::ERROR } else { tool_color(&ct.name) },
    diff_lines,
    content_hash: 0,
};
vm.recompute_hash();
tail_vms.push(vm);
```

SubAgentGroup 同理。

- [ ] **Step 4: 修改 `tool_end_internal()` 中的 frozen_vm 构造 (mod.rs)**

在 `tool_end_internal()` 方法中，`SubAgentGroup` frozen VM 构造处：

```rust
let mut vm = MessageViewModel::SubAgentGroup {
    agent_id: sub.agent_id.clone(),
    task_preview: sub.task_preview.clone(),
    total_steps: sub.total_steps,
    recent_messages: std::mem::take(&mut sub.recent_messages),
    is_running: false,
    collapsed: false,
    final_result: Some(output.to_string()),
    is_error,
    is_background: false,
    bg_hash: sub.bg_hash.clone(),
    batch_agents: Vec::new(),
    instance_id: Some(sub.instance_id.clone()),
    content_hash: 0,
};
vm.recompute_hash();
sub.finalized_vm = Some(vm.clone());
self.frozen_subagent_vms.push(vm);
```

- [ ] **Step 5: 修改 `drain_subagent_stack()` 中的 SubAgentGroup 构造 (mod.rs)**

`drain_subagent_stack()` 中有两处 `MessageViewModel::SubAgentGroup { ... }` 构造（异常残留 + 后台 agent），每处都加 `content_hash: 0` + `recompute_hash()`。

- [ ] **Step 6: 修改 `update_tool_end_in_subagent()` 中的字段更新 (mod.rs)**

在 `update_tool_end_in_subagent()` 中，修改 `content` 和 `is_error` 后需要 recompute_hash：

```rust
fn update_tool_end_in_subagent(
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
                vm.recompute_hash();
                break;
            }
        }
    }
}
```

- [ ] **Step 7: 运行 message_pipeline_test 验证**

```bash
cargo test -p peri-tui --lib message_pipeline_test -- --nocapture
```

Expected: 所有测试通过。

---

### Task 4: 修改 agent_ops 中的直接变更点

**Files:**
- Modify: `peri-tui/src/app/agent_ops/lifecycle.rs`
- Modify: `peri-tui/src/app/agent_events_bg.rs`

- [ ] **Step 1: 修改 `lifecycle.rs` 中的 `is_streaming = false`**

在设置 `*is_streaming = false` 后调用 `recompute_hash()`：

```rust
if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
    self.session_mgr.sessions[self.session_mgr.active]
        .messages
        .view_messages
        .last_mut()
{
    *is_streaming = false;
    // 注意：last_mut() 返回 &mut MessageViewModel，需要在此处 recompute
    // 但借用检查要求在 if let 块内完成
}
// 需要改为：
if let Some(vm) = self.session_mgr.sessions[self.session_mgr.active]
    .messages
    .view_messages
    .last_mut()
{
    if let MessageViewModel::AssistantBubble { is_streaming, .. } = vm {
        *is_streaming = false;
    }
    vm.recompute_hash();
}
```

- [ ] **Step 2: 修改 `agent_events_bg.rs` 中的字段变更后 recompute_hash**

两处变更（第一遍 instance_id 精确匹配 + 第二遍 agent_name 匹配）：

第一遍（行 ~183-195）：
```rust
if *is_background && *is_running && instance_id.as_deref() == Some(ctid.as_str()) {
    *is_running = false;
    *final_result = Some(output.clone());
    *is_error = !success;
    *total_steps = tool_calls_count;
    found_and_updated = true;
    // 在外层循环的 &mut vm 上调用 recompute_hash
    break;
}
```

需要将循环改为获取 `&mut MessageViewModel` 并在修改后 recompute：

```rust
for vm in session.messages.view_messages.iter_mut() {
    if let MessageViewModel::SubAgentGroup {
        instance_id,
        is_background,
        is_running,
        final_result,
        is_error,
        total_steps,
        ..
    } = vm
    {
        if *is_background
            && *is_running
            && instance_id.as_deref() == Some(ctid.as_str())
        {
            *is_running = false;
            *final_result = Some(output.clone());
            *is_error = !success;
            *total_steps = tool_calls_count;
            vm.recompute_hash();
            found_and_updated = true;
            break;
        }
    }
}
```

第二遍（行 ~225-239）同理，在 `best_idx` 匹配后：
```rust
if let Some(idx) = best_idx {
    let vm = &mut session.messages.view_messages[idx];
    if let MessageViewModel::SubAgentGroup {
        is_running,
        total_steps,
        final_result,
        is_error,
        ..
    } = vm
    {
        *is_running = false;
        *final_result = Some(output.clone());
        *is_error = !success;
        *total_steps = tool_calls_count;
        vm.recompute_hash();
        found_and_updated = true;
    }
}
```

- [ ] **Step 3: 运行编译验证**

```bash
cargo build -p peri-tui 2>&1 | head -80
```

Expected: 编译通过，无 error。

---

### Task 5: 修改 aggregate 函数中的构造点

**Files:**
- Modify: `peri-tui/src/ui/message_view/aggregate.rs`

- [ ] **Step 1: 修改 `aggregate_tail_tool_groups()` 中的 `ToolCallGroup` 构造（行 60-64）**

将：
```rust
result.push(MessageViewModel::ToolCallGroup {
    category: cat,
    tools: entries,
    collapsed: true,
});
```

替换为：
```rust
let mut vm = MessageViewModel::ToolCallGroup {
    category: cat,
    tools: entries,
    collapsed: true,
    content_hash: 0,
};
vm.recompute_hash();
result.push(vm);
```

- [ ] **Step 2: 修改 `aggregate_batch_groups()` 中的 clone + 变更（行 142-153）**

`aggregate_batch_groups()` 不直接构造新 VM，而是 `clone()` 后修改 `batch_agents` 和 `collapsed`。修改后需 recompute：

将：
```rust
let mut merged = messages[run_start].clone();
if let MessageViewModel::SubAgentGroup {
    ref mut batch_agents,
    ref mut collapsed,
    ..
} = merged
{
    *batch_agents = batch_summaries;
    *collapsed = true;
}
result.push(merged);
```

替换为：
```rust
let mut merged = messages[run_start].clone();
if let MessageViewModel::SubAgentGroup {
    ref mut batch_agents,
    ref mut collapsed,
    ..
} = merged
{
    *batch_agents = batch_summaries;
    *collapsed = true;
}
merged.recompute_hash();
result.push(merged);
```

- [ ] **Step 3: 运行 message_view_test 验证**

```bash
cargo test -p peri-tui --lib message_view_test -- --nocapture
```

---

### Task 6: 修改 RenderTask::rebuild() — 替换全量 hash 计算

**Files:**
- Modify: `peri-tui/src/ui/render_thread.rs`

- [ ] **Step 1: 替换 `rebuild()` 中的 hash 计算**

将：
```rust
// 在渲染前计算 hash（render_one 会修改 dirty 等字段）
let new_hashes: Vec<u64> = messages.iter().map(Self::compute_hash).collect();
```

替换为：
```rust
// 直接读取预计算的 content_hash，无需重算
let new_hashes: Vec<u64> = messages.iter().map(|vm| vm.content_hash()).collect();
```

- [ ] **Step 2: 保留 `compute_hash()` 方法（向后兼容/调试用）**

`compute_hash()` 可保留为私有方法，添加注释标记为 legacy：

```rust
/// 计算单个 MessageViewModel 的语义 hash（legacy，应使用 vm.content_hash()）
#[allow(dead_code)]
fn compute_hash(vm: &MessageViewModel) -> u64 {
    let mut hasher = DefaultHasher::new();
    vm.hash(&mut hasher);
    hasher.finish()
}
```

如果编译器报 dead_code warning，可以删除或加 `#[cfg(test)]`。

- [ ] **Step 3: 新增测试验证 rebuild 使用预计算 hash**

在 `render_thread_test.rs` 中添加测试：

```rust
#[tokio::test]
async fn test_rebuild_uses_content_hash() {
    // 验证 rebuild 使用 content_hash 而非重新计算
    let (tx, cache, _notify) = spawn_render_thread(80);

    let user = MessageViewModel::user("Test".to_string());
    let hash = user.content_hash();
    assert_ne!(hash, 0, "content_hash 应在构造时计算");

    tx.send(RenderEvent::Rebuild(vec![user])).unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.version > 0, "version should increment");
    assert!(!c.lines.is_empty());
}

#[tokio::test]
async fn test_rebuild_identical_hash_skips_render() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    let user1 = MessageViewModel::user("Same".to_string());
    let user2 = MessageViewModel::user("Same".to_string());
    // 验证相同内容产生相同 content_hash
    assert_eq!(user1.content_hash(), user2.content_hash());

    tx.send(RenderEvent::Rebuild(vec![user1])).unwrap();
    wait_render().await;
    let v1 = cache.read().version;
    let lines_v1 = cache.read().lines.len();

    // 第二次 Rebuild：相同内容，prefix_stable_len 应为 1
    tx.send(RenderEvent::Rebuild(vec![user2])).unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.version > v1, "version should still increment");
    assert_eq!(c.lines.len(), lines_v1, "lines should be identical");
}
```

- [ ] **Step 4: 运行 render_thread_test 验证**

```bash
cargo test -p peri-tui --lib render_thread -- --nocapture
```

Expected: 所有测试通过。

---

### Task 7: 处理 render_one 中的 dirty 修改

**Files:**
- Modify: `peri-tui/src/ui/render_thread.rs`

**注意：** `render_one()` 会修改 `AssistantBubble` 的 `blocks`（`ensure_rendered_incremental`）和 `UserBubble` 的 `rendered`（`parse_markdown`）。这些修改不参与 hash（`rendered` 和 `dirty`/`rendered_prefix_len` 不在 `Hash` impl 中），因此 **不需要** recompute_hash。

但需要验证 `render_one` 的调用时机：它只在 prefix_stable_len 之后的消息上调用，前缀区的 VM 不会被修改。所以这是安全的。

- [ ] **Step 1: 验证 render_one 不会影响 hash 正确性**

添加注释说明：

```rust
/// 渲染单条消息为 lines（含前后空行分隔）
///
/// 注意：此函数修改的 rendered/dirty/rendered_prefix_len 等字段不参与 Hash 计算，
/// 因此不会使 content_hash 失效。
fn render_one(...) -> Vec<Line<'static>> {
```

---

### Task 8: 修改测试文件中的直接构造点

**Files:**
- Modify: `peri-tui/src/ui/message_view/message_view_test.rs` — `make_done_subagent()`, `make_running_subagent()`, `test_aggregate_batch_groups_already_aggregated_skip` 中的 `SubAgentGroup` 构造
- Modify: `peri-tui/src/ui/message_render_test.rs` — 3 处 `ToolBlock` 构造 + 1 处 `ToolCallGroup` + 1 处 `SubAgentGroup`
- Modify: `peri-tui/src/ui/headless_test.rs` — 1 处 `ToolBlock` 构造（行 1084）
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs` — 1 处 `SubAgentGroup` 构造（行 1291）

- [ ] **Step 1: 修改 message_view_test.rs**

`make_done_subagent()` 和 `make_running_subagent()` 辅助函数中的 `SubAgentGroup` 构造：

```rust
fn make_done_subagent(agent_id: &str, task: &str) -> MessageViewModel {
    let mut vm = MessageViewModel::SubAgentGroup {
        agent_id: agent_id.to_string(),
        task_preview: task.to_string(),
        total_steps: 3,
        recent_messages: Vec::new(),
        is_running: false,
        collapsed: false,
        final_result: Some("done".to_string()),
        is_error: false,
        is_background: false,
        bg_hash: Some("test01".to_string()),
        batch_agents: Vec::new(),
        instance_id: None,
        content_hash: 0,
    };
    vm.recompute_hash();
    vm
}
```

`make_running_subagent()` 同理。

`test_aggregate_batch_groups_already_aggregated_skip` 中也有一个 `SubAgentGroup` 直接构造（行 288），加 `content_hash: 0` + `recompute_hash()`。

- [ ] **Step 2: 修改 message_render_test.rs**

3 处 `ToolBlock`、1 处 `ToolCallGroup`、1 处 `SubAgentGroup` 构造，每处添加 `content_hash: 0`。由于这些测试只检查渲染输出而不检查 hash，也可以简化为只添加 `content_hash: 0` 而不调用 `recompute_hash()`（测试不依赖 hash 正确性）。但为一致性，建议统一调用 `recompute_hash()`。

- [ ] **Step 3: 修改 headless_test.rs**

行 1084 的 `ToolBlock` 构造添加 `content_hash: 0`。

- [ ] **Step 4: 修改 message_pipeline_test.rs**

行 1291 的 `SubAgentGroup` 构造（`test_merge_frozen_subagents_empty_is_noop`）添加 `content_hash: 0` + `recompute_hash()`。

**注意：** 此处构造后紧跟 `new_vms.clone()`，而 `PartialEq` 不包含 `content_hash`，所以 `assert_eq!` 不受影响。但如果 `content_hash` 不一致可能影响 future 比较。务必调用 `recompute_hash()` 确保一致性。

- [ ] **Step 5: 全面编译验证**

```bash
cargo build -p peri-tui 2>&1 | tail -20
```

Expected: 编译通过。

---

### Task 9: 全量测试验证

- [ ] **Step 1: 运行 peri-tui 全量测试**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -40
```

Expected: 所有测试通过。

- [ ] **Step 2: 运行 clippy 检查**

```bash
cargo clippy -p peri-tui 2>&1 | grep -E "warning|error" | head -20
```

Expected: 无新增 warning/error。

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/ui/message_view/mod.rs \
        peri-tui/src/ui/message_view/message_view_test.rs \
        peri-tui/src/ui/render_thread.rs \
        peri-tui/src/ui/render_thread_test.rs \
        peri-tui/src/ui/message_view/aggregate.rs \
        peri-tui/src/app/message_pipeline/transform.rs \
        peri-tui/src/app/message_pipeline/reconcile.rs \
        peri-tui/src/app/message_pipeline/mod.rs \
        peri-tui/src/app/agent_ops/lifecycle.rs \
        peri-tui/src/app/agent_events_bg.rs
git commit -m "perf(render): inline content_hash into MessageViewModel to skip rebuild recalculation

Adds content_hash: u64 field to every MessageViewModel variant. Hash is
computed once at construction and updated on mutation (append_chunk,
is_streaming change, is_running change, etc.). RenderTask::rebuild() now
reads vm.content_hash() directly instead of calling compute_hash() per
message, eliminating O(n * content_size) hash computation for prefix
stable region.

Expected: 60-80% hash computation time reduction in 200+ message sessions.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## 自检

**1. Spec 覆盖：** 覆盖 issue 中提出的所有改造点：`content_hash` 字段、构造时计算、变更时更新、rebuild 读取。

**2. Placeholder 扫描：** 无 TBD/TODO，所有步骤有确切代码和命令。

**3. Hash 一致性：** `content_hash()` 的计算复用现有 `Hash` impl（不含 `rendered`/`color`），与旧 `compute_hash()` 输出完全一致。新增测试 `test_content_hash_matches_compute_hash` 验证。

**4. 变更点覆盖：** 所有直接修改 VM 内容字段的点（`append_chunk`、`is_streaming = false`、`is_running = false`、`content = output`、`collapsed`）都安排了 `recompute_hash()` 调用。

**5. 非变更点安全性：** `render_one()` 修改的 `rendered`/`dirty`/`rendered_prefix_len` 不参与 `Hash`，无需 recompute。`message_render.rs` 中的 `state.collapsed` 是 widget 状态不回写 VM，无需处理。
