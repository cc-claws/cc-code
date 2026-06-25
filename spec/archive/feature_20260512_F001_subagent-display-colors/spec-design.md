# Feature: 20260512_F001 - subagent-display-colors

## 需求背景

当前 SubAgent 统一使用绿色（`SAGE`）显示 `● {agent_id}` 格式。后台 agent 完成后以独立 ToolBlock `bg:{agent_name}` 显示，与前台的 SubAgentGroup 视觉不统一。用户需要一眼区分：

- 前台 agent（始终绿色）
- 后台 agent 运行中（黄色）
- 后台 agent 完成（绿色）
- 错误（红色）

同时，工具调用的显示格式需要从 `● {agent_id}` 改为 `Agent(type)` 风格，`Agent` 带状态颜色，`(type)` 和 `#bg_hash` 灰色。

## 目标

- SubAgentGroup 显示格式改为 `Agent(type)`，其中 `Agent` 带状态颜色，`(type)` 灰色
- 后台 agent 运行中显示黄色 `Agent(type) #hash`，完成后变绿色
- 后台 agent 完成时统一为 SubAgentGroup 样式，不再使用 ToolBlock `bg:xxx`
- 错误状态保持红色不变

## 方案设计

### 1. 数据模型扩展

**SubAgentGroup（`MessageViewModel`）新增两个字段：**

```rust
SubAgentGroup {
    // 现有字段不变
    agent_id: String,
    task_preview: String,
    total_steps: usize,
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    collapsed: bool,
    final_result: Option<String>,
    is_error: bool,
    // 新增
    is_background: bool,         // 是否为后台 agent
    bg_hash: Option<String>,     // 后台任务的短 ID（task_id 前 8 位）
}
```

**SubAgentState（pipeline 内部状态）同步新增：**

```rust
struct SubAgentState {
    // 现有字段
    agent_id: String,
    task_preview: String,
    total_steps: usize,
    recent_messages: Vec<MessageViewModel>,
    is_running: bool,
    finalized_vm: Option<MessageViewModel>,
    // 新增
    is_background: bool,
    bg_hash: Option<String>,
}
```

### 2. 事件流变更

当前后台 agent 的事件流：

```
SubAgentStart { is_background: true }
  → pipeline 创建 SubAgentGroup { is_running: true }
SubAgentEnd { result: "Background task bg-xxx started...", is_error: false }
  → pipeline 冻结 SubAgentGroup { is_running: false, final_result: Some("Background task...") }
BackgroundTaskCompleted { task_id, agent_name, output, ... }
  → agent_events_bg.rs 创建 ToolBlock "bg:{agent_name}"
```

问题：SubAgentEnd 过早地将后台 agent 标记为完成（`is_running=false`），且 BackgroundTaskCompleted 又创建了一个不相关的 ToolBlock。

**新的后台 agent 事件流：**

```
SubAgentStart { is_background: true }
  → SubAgentGroup { is_running: true, is_background: true, bg_hash: None }
  → 渲染: Agent(type) 黄色，无 hash

SubAgentEnd { result: "Background task bg-xxx started...", is_error: false }
  → 保持 is_running=true（不冻结）
  → 从 result 解析 task_id，取前 8 位存入 bg_hash
  → 渲染: Agent(type) #a1b2c3d4 黄色

BackgroundTaskCompleted { task_id, agent_name, output, ... }
  → 在 view_messages 中找到匹配的 SubAgentGroup，更新:
    - is_running = false
    - final_result = Some(output)
    - is_error = !success
  → 渲染: Agent(type) #a1b2c3d4 绿色
```

前台 agent 流程不变。

### 3. Pipeline 变更

#### 3.1 SubAgentStart 处理器

`message_pipeline.rs` 中 `handle_event` 的 `SubAgentStart` 分支：

- 将 `is_background` 传入 `SubAgentState`（当前被 `_` 忽略）
- `SubAgentState` 初始化时 `is_background = is_background`, `bg_hash = None`

#### 3.2 SubAgentEnd 处理器（`tool_end_internal`）

核心变更点。当 `sub.is_background == true` 时：

1. **不调用 finalize_vm**：保持 `is_running=true`
2. **解析 bg_hash**：从 result 字符串 `"Background task bg-{uuid} started..."` 中提取 task_id
   - 使用 `strip_prefix("Background task bg-")` + 取前 8 位字符
   - 解析失败时 `bg_hash = None`（优雅降级）
3. **不推入 frozen_subagent_vms**：后台 agent 需要保持活跃直到 BackgroundTaskCompleted 到达

当 `sub.is_background == false` 时，行为不变。

#### 3.3 done() / drain_subagent_stack

当前代码已有对 `is_running=true` 的跳过逻辑：

```rust
// 仍在运行（is_running=true）的不推入——background agent 仍在执行
```

无需变更。后台 agent 的 SubAgentGroup 在之前的 rebuild 中已存在于 `view_messages`，`is_running=true` 保持可见。

### 4. 渲染格式变更

#### 4.1 颜色映射

```rust
let agent_color = if is_error {
    theme::ERROR          // 红色
} else if is_running && is_background {
    theme::WARNING        // 黄色（后台运行中）
} else {
    theme::SAGE           // 绿色（完成 / 前台）
};
```

#### 4.2 显示格式

当前格式：

```
● code-review              ← BOLD + agent_color
  task preview text...      ← MUTED
```

新格式：

```
Agent(code-review) #a1b2c3d4    ← "Agent" BOLD + agent_color, "(code-review)" MUTED, "#hash" MUTED
  task preview text...           ← MUTED
```

渲染代码（`message_render.rs` SubAgentGroup 分支）：

```rust
let mut header_spans = vec![
    Span::styled("Agent".to_string(),
        Style::default().fg(agent_color).add_modifier(Modifier::BOLD)),
    Span::styled(format!("({})", agent_id),
        Style::default().fg(theme::MUTED)),
];
if let Some(ref hash) = bg_hash {
    header_spans.push(Span::styled(
        format!(" #{}", hash),
        Style::default().fg(theme::MUTED),
    ));
}
lines.push(Line::from(header_spans));
```

折叠和展开状态的 header 行统一使用此格式。

### 5. 后台完成处理变更

#### 5.1 handle_background_task_completed（agent_events_bg.rs）

**移除**：创建 ToolBlock `bg:{agent_name}` 的逻辑。

**新增**：

1. 遍历 `view_messages`，找到第一个满足条件的 SubAgentGroup：
   - `is_background == true`
   - `is_running == true`
   - `agent_id == agent_name`（BackgroundTaskCompleted 的 agent_name 字段）
2. 如果找到：
   - 克隆该 VM，更新字段：`is_running = false`, `final_result = Some(output)`, `is_error = !success`
   - 替换 view_messages 中的原 VM
   - 触发 `request_rebuild()`
3. 如果没找到（边缘情况，如历史恢复后）：
   - 回退到创建 ToolBlock（兼容现有行为）

匹配策略使用「第一个满足条件」而非精确 task_id 匹配，因为 BackgroundTaskCompleted 携带 `agent_name` 而非 `agent_id`（两者值相同），且多个同名后台 agent 按完成顺序依次匹配，符合直觉。

#### 5.2 agent_state_messages 通知

保留现有逻辑：将后台完成通知推入 `agent_state_messages` 供下一轮 LLM 上下文使用。这部分不变。

#### 5.3 continuation 流程

保留现有的 `agent_done_pending_bg` + `pending_bg_continuation` 逻辑。唯一变更：completion 通知中的 display 文本不再单独创建 ToolBlock，而是通过 SubAgentGroup 的 final_result 展示。

### 6. 持久化与恢复

`from_base_message_with_cwd`（`message_view.rs`）中 Agent 工具的恢复路径：

当前逻辑从 `input["subagent_type"]` 读取 agent_id，构建 SubAgentGroup。新增：

- **is_background 检测**：从 result 字符串检测 `"Background task"` 前缀
  - 如果匹配 → `is_background = true`
  - 否则 → `is_background = false`
- **bg_hash 解析**：同 pipeline 中的解析逻辑，从 result 提取 task_id 前 8 位
- **is_running**：恢复时一律 `false`（后台任务的实际结果不在持久化消息中）
- **final_result**：保持原 result 内容

### 7. Hash / PartialEq / 构造函数更新

- `Hash` impl：新增 `is_background` 和 `bg_hash` 参与 hash
- `PartialEq` impl：新增 `is_background` 和 `bg_hash` 参与比较
- `subagent_group()` 构造函数：新增 `is_background` 参数（默认 `false`）
- `tool_end_internal` / `drain_subagent_stack` 等处构造 SubAgentGroup 的代码同步更新

## 实现要点

1. **后台 agent SubAgentEnd 不冻结**：最关键变更。`tool_end_internal` 需检查 `sub.is_background`，后台路径跳过 `finalize_vm`，改为解析 bg_hash 并保持 `is_running=true`。

2. **task_id 解析**：从 `"Background task bg-{uuid} started..."` 中提取。使用 `strip_prefix("Background task bg-")` + `split(' ')` + 取前 8 字符，不引入正则依赖。提取为独立 helper 函数 `parse_bg_hash(result: &str) -> Option<String>`，pipeline 和 `from_base_message_with_cwd` 复用。

3. **view_messages 直接操作**：BackgroundTaskCompleted 不经过 MessagePipeline（它是独立事件通道），直接在 `agent_events_bg.rs` 中操作 `view_messages`。这与当前 `AddMessage` 操作模式一致（同属 pipeline 外的 VM 操作），通过 `request_rebuild()` 触发渲染更新。

4. **多同名后台 agent 匹配**：使用「第一个 `is_running=true` 的匹配」策略。后台任务按完成顺序依次匹配 view_messages 中的 SubAgentGroup，符合 FIFO 语义。

5. **折叠/展开状态保留**：BackgroundTaskCompleted 更新 SubAgentGroup 时，保留 `collapsed` 字段不变（克隆 → 更新 → 替换，不重置折叠状态）。

## 约束一致性

- **符合 Widget 独立 crate 约束**：渲染变更在 `peri-tui` 内部，不涉及 `peri-widgets`
- **符合消息管线统一约束**：流式更新仍通过 `PipelineAction::None` + `request_rebuild()` 路径；后台完成直接操作 `view_messages` 是对管道的例外，但在现有架构中已有先例（`AddMessage` 操作）
- **符合配色系统约束**：使用现有 theme 常量（`WARNING`/`SAGE`/`ERROR`/`MUTED`），不引入新颜色
- **符合编码规范**：字符串截断使用字符级操作（`.chars().take(8)`）
- **无架构偏离**：不改变事件定义（`SubAgentStart`/`SubAgentEnd` 签名不变），不新增中间件

## 验收标准

- [ ] 前台 SubAgent 显示为 `Agent(type)` 格式，Agent 绿色 BOLD，`(type)` 灰色
- [ ] 后台 SubAgent 运行中显示 `Agent(type)` 黄色 BOLD（SubAgentStart 后无 hash，SubAgentEnd 后显示 `#hash`）
- [ ] 后台 SubAgent 完成后显示 `Agent(type) #hash` 绿色 BOLD
- [ ] 错误状态显示 `Agent(type)` 红色
- [ ] 后台 agent 完成时不再出现 `bg:xxx` ToolBlock
- [ ] 多个同名后台 agent 可正确按 FIFO 匹配更新
- [ ] 历史恢复路径正确推断 `is_background` 和 `bg_hash`
- [ ] 现有测试通过（message_pipeline_test / headless_test）
- [ ] 状态栏 `[BG: N]` 指示器行为不变
