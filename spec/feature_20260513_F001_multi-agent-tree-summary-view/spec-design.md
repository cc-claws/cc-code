# Feature: 20260513_F001 - multi-agent-tree-summary-view

## 需求背景

当主 agent 并行 dispatching 多个 SubAgent 时（如 `dispatching-parallel-agents` skill 触发），每个 SubAgent 渲染为独立的 `SubAgentGroup` 块，展开显示内部工具调用和消息，占满大量屏幕空间。用户期望类似 Claude Code 的紧凑树形汇总视图——多个 agent 合并为一个可折叠的树，默认折叠，每个 agent 只占一行摘要。

当前问题：

- 每个 SubAgent 独立展开，内部消息全部显示，垂直空间占用过多
- 无「同批次 agent 汇总」的概念
- 无树形连接线（`├─`/`└─`）的渲染能力

## 目标

- 同一 ReAct 步骤中并行发起的多个 SubAgent，全部完成后合并为一个树形汇总视图
- 默认折叠，每行显示：任务描述 + 工具调用数 + 完成状态
- 用户可展开查看各 agent 的完整详情
- 不同批次、被用户消息隔开的 SubAgent 保持独立，不合并
- 历史恢复路径同样支持批次聚合

## 方案设计

### 数据模型

在现有 `SubAgentGroup` 变体中新增 `batch_agents` 字段，复用该变体而非创建新 VM 类型。

**新增结构体：**

```rust
/// 批次中单个 agent 的摘要信息
struct AgentSummary {
    agent_id: String,
    task_preview: String,       // 截断到 50 字符
    tool_count: usize,          // total_steps
    is_error: bool,
    final_result: Option<String>, // 仅第一行
}
```

**SubAgentGroup 变体扩展：**

```rust
SubAgentGroup {
    // ... 现有字段全部保留 ...
    batch_agents: Vec<AgentSummary>, // 新增：空 = 单 agent，非空 = 批次汇总
}
```

语义约定：

- `batch_agents.is_empty()` → 单 agent，渲染逻辑与现有一致（零改动）
- `batch_agents` 非空 → 批次汇总模式，`task_preview` 和 `recent_messages` 等现有字段不用于摘要行（由 batch_agents 驱动）

### Pipeline 批次检测

**新增 Pipeline 状态：**

```rust
struct BatchInfo {
    started: usize,
    completed: usize,
}

// MessagePipeline 新增
active_batch: Option<BatchInfo>,
```

**检测规则：**

1. `SubAgentStart`：如果 `active_batch` 为 None → 创建 `BatchInfo { started: 1, completed: 0 }`；否则 `started += 1`
2. `SubAgentEnd`：`active_batch.completed += 1`。当 `completed == started && started > 1` 时，标记此批次为「多 agent 批次」
3. `StepDone` 或非 Agent 的 `ToolStart` 到达时：重置 `active_batch = None`

**判定依据**：同一 ReAct 步骤中的并行 Agent 工具调用——多个 `SubAgentStart` 在同一批次连续到达，`SubAgentEnd` 陆续到达，当所有 agent 完成且数量 > 1 时触发聚合。

### 聚合机制

在 `merge_frozen_subagents()` 之后，新增 `aggregate_batch_groups()` 步骤：

1. 遍历 `tail_vms`，找到连续的、属于同一批次的 SubAgentGroup
2. 将 N 个 SubAgentGroup 合并为 1 个：
   - `batch_agents` 从各 VM 的 `agent_id` / `task_preview` / `total_steps` / `is_error` / `final_result` 提取
   - 保留第一个 VM 的位置，删除其余 N-1 个
   - `collapsed` 默认 `true`
3. 合并后的 VM 通过 `RebuildAll` 替换尾部

**流式行为：**

- 执行中：多个 SubAgentGroup 独立显示（各自 `is_running: true`），保持当前 UX
- 全部完成后：reconcile 触发聚合，一次性替换为树形汇总视图

### 树形渲染

**折叠状态（默认）：**

```
⏺ 3 agents finished
   ├─ 创建用户文档首页 · 2 tool uses · Done
   ├─ 创建大模型配置文档 · 8 tool uses · Done
   └─ 创建Agent管理文档 · Done
```

- Header 行：`⏺`（蓝色）+ `N agents finished`（白色）
- 每 agent 行：`├─`/`└─` 树形连接线（`theme::DIM`）+ task_preview 截断 50 字符 + `· N tool uses` + 状态（Done 绿色 / Failed 红色）
- 有错误 agent 时 header 改为 `N agents finished, K failed`

**展开状态：**

展开后每个 agent 显示完整详情（header + final_result），与当前单个 agent 展开效果一致，用 2 空格缩进 + `theme::SUB_AGENT_BG` 背景色。

**渲染实现：**

在 `render_view_model()` 的 `SubAgentGroup` 分支中，检查 `batch_agents.is_empty()`：

- 空 → 现有渲染逻辑不变
- 非空 → 新增树形渲染分支，根据 `collapsed` 字段决定折叠/展开

**交互：** Enter 键切换 `collapsed` 状态（复用现有 SubAgentGroup 的折叠/展开机制）。

### 历史恢复路径

在 `messages_to_view_models()` 中处理 BaseMessage 历史时，增加批次检测：

1. 解析 Ai 消息中的 ToolUse 块，统计其中 Agent 工具的数量
2. 同一 Ai 消息包含 > 1 个 Agent ToolUse → 标记为同批次
3. 收集完所有对应 ToolResult 后，调用相同的 `aggregate_batch_groups()` 聚合

检测逻辑：同一 Ai 消息中的多个 Agent ToolUse 必定属于同一 ReAct 步骤（一次 LLM 响应的工具调用），因此直接按 Ai 消息粒度判定批次。

### 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/ui/message_view.rs` | SubAgentGroup 新增 `batch_agents` 字段 + `AgentSummary` 结构体 |
| `peri-tui/src/ui/message_render.rs` | SubAgentGroup 树形渲染分支 |
| `peri-tui/src/app/message_pipeline.rs` | BatchInfo 状态、批次检测、`aggregate_batch_groups()` 聚合 |
| `peri-tui/src/ui/message_view.rs`（`aggregate_tool_groups` 附近） | 新增 `aggregate_batch_groups()` 函数 |

## 实现要点

1. **批次边界判定**：利用 SubAgentStart/SubAgentEnd 的到达顺序判断同批次。并行 dispatching 时多个 SubAgentStart 连续到达，全部结束后才触发聚合
2. **聚合原子性**：`aggregate_batch_groups()` 在 reconcile 期间执行，此时 tail_vms 可变，一次性完成合并和删除
3. **单 agent 兼容**：`batch_agents` 为空时所有逻辑短路，现有行为零影响
4. **PartialEq 适配**：`MessageViewModel::PartialEq` 实现需比较 `batch_agents` 字段
5. **树形连接线**：最后一个 agent 用 `└─`，其余用 `├─`，复用 `theme::DIM` 颜色

## 约束一致性

- **消息管线统一**：聚合逻辑在 Pipeline 的 reconcile 阶段执行，符合 MessagePipeline 作为唯一入口的架构约束
- **Widget 独立 crate**：渲染逻辑在 `message_render.rs`（TUI 层），不涉及 peri-widgets
- **事件驱动 TUI**：无新增事件类型，复用现有 SubAgentStart/SubAgentEnd/RebuildAll
- **RebuildAll 尾部替换**：聚合通过 RebuildAll 触发，只替换尾部，保留前缀
- 无架构偏离，无新增约束

## 验收标准

- [ ] 并行 dispatching 3+ 个 SubAgent 时，全部完成后显示为树形汇总视图
- [ ] 树形视图默认折叠，每行显示 agent 名称 + 工具数 + 状态
- [ ] 按 Enter 键展开显示各 agent 完整详情
- [ ] 单个 SubAgent 调用显示效果不变
- [ ] 不同批次的 SubAgent 不合并
- [ ] 历史恢复路径正确聚合同批次 agent
- [ ] `batch_agents` 为空时所有现有测试不受影响
