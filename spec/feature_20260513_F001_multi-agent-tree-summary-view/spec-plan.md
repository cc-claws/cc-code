# 实施计划: 20260513_F001 - multi-agent-tree-summary-view

## 依赖关系

```
Task 1 (AgentSummary 数据模型)
  |
  +---> Task 2 (Pipeline 批次检测)
  |       |
  |       +---> Task 3 (aggregate_batch_groups 聚合)
  |               |
  |               +---> Task 4 (树形渲染)
  |
  +---> Task 5 (历史恢复路径)
  |       |
  |       +---> Task 3 (复用 aggregate_batch_groups)
  |
Task 6 (测试) -- 依赖 Task 1-5 全部完成
```

Task 1 是基础。Task 2 和 Task 5 可以并行开发，但都依赖 Task 1。Task 3 依赖 Task 2。Task 4 依赖 Task 1 和 Task 3。Task 6 覆盖全部。

---

## Task 1: SubAgentGroup 新增 batch_agents 字段与 AgentSummary 结构体

**文件**: `peri-tui/src/ui/message_view.rs`

**改动:**

1. 新增 `AgentSummary` 结构体（放在 `ToolEntry` 附近，模块顶层）：

```rust
/// 批次中单个 agent 的摘要信息
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSummary {
    pub agent_id: String,
    pub task_preview: String,       // 截断到 50 字符
    pub tool_count: usize,          // total_steps
    pub is_error: bool,
    pub final_result: Option<String>, // 仅第一行
}
```

2. `SubAgentGroup` 变体新增字段 `batch_agents: Vec<AgentSummary>`

3. `PartialEq` impl 新增 `batch_agents` 参与比较（在 `SubAgentGroup` match arm 中追加）

4. `Hash` impl 新增 `batch_agents` 参与 hash

5. 所有构造 `SubAgentGroup` 的位置补上新字段 `batch_agents: Vec::new()`：
   - `subagent_group()` 构造函数
   - `from_base_message_with_cwd` 中 Agent 工具恢复路径
   - `message_pipeline.rs` 中所有 `SubAgentGroup` 构造点（约 6 处）：
     - `tool_end_internal` 中前台 agent 冻结路径
     - `tool_end_internal` 中后台 agent 路径
     - `build_tail_vms` 中从 `subagent_stack` 构建的路径
     - `drain_subagent_stack` 中异常残留路径

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 2: Pipeline 批次检测状态

**文件**: `peri-tui/src/app/message_pipeline.rs`

**改动:**

1. 新增 `BatchInfo` 结构体（放在 `SubAgentState` 附近）：

```rust
struct BatchInfo {
    started: usize,
    completed: usize,
    stack_depth: usize,
}
```

2. `MessagePipeline` 新增字段 `active_batch: Option<BatchInfo>`，`new()` 中初始化为 `None`

3. 修改 `SubAgentStart` 处理器：
   - `active_batch` 为 None → 创建 `BatchInfo { started: 1, completed: 0, stack_depth: subagent_stack.len() - 1 }`
   - 否则 → `started += 1`

4. 修改 `SubAgentEnd` 处理器：
   - `active_batch` 为 Some → `completed += 1`
   - `completed == started` 时标记批次就绪

5. 批次重置时机：
   - `Done` / `Interrupted`：重置 `active_batch = None`
   - `ToolStart` 且 name != "Agent"：重置（非 Agent 工具打断批次连续性）
   - `StateSnapshot`：不重置

6. `clear()` 方法追加 `self.active_batch = None;`

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 3: aggregate_batch_groups() 聚合函数

**文件**: `peri-tui/src/ui/message_view.rs`（放在 `aggregate_tool_groups` 附近）

**改动:**

1. 新增函数 `aggregate_batch_groups(messages: &mut Vec<MessageViewModel>)`：

算法：
1. 扫描 messages，找到连续的、`batch_agents.is_empty()` 的、`!is_running` 的 SubAgentGroup 区间
2. 区间长度 <= 1 → 跳过
3. 从每个 VM 提取 `AgentSummary`（agent_id / task_preview 截断 50 字符 / total_steps / is_error / final_result）
4. 将 N 个 VM 合并为 1 个：保留第一个 VM 的位置，设置 `batch_agents`，`collapsed = true`
5. 删除区间中第 2..N 个 VM

2. `build_tail_vms()` 末尾（`aggregate_tool_groups` 之后）调用 `aggregate_batch_groups(&mut tail_vms)`

3. `messages_to_view_models()` 末尾同样追加调用

**流式期间**：`is_running: true` 的 VM 不参与聚合，保持独立显示。全部完成后 reconcile 触发，已完成的连续 SubAgentGroup 被合并为树形视图。

**验证**: `cargo build -p peri-tui` 编译通过

---

## Task 4: 树形渲染实现

**文件**: `peri-tui/src/ui/message_render.rs`

**改动:**

1. 在 `render_view_model` 的 `SubAgentGroup` 分支中，现有逻辑之前插入 `batch_agents` 检查：非空时调用 `render_batch_summary()`

2. 新增 `render_batch_summary()` 函数：

**折叠态：**
- Header 行：`⏺`（`theme::SAGE`）+ `N agents finished`（`theme::TEXT`）
  - 有错误时：`N agents finished, K failed`
  - 全部失败时：`N agents failed`
- 每个 agent 行（index 判断 `├─`/`└─`）：
  - 前缀：`   ` + `├─`/`└─`（`theme::DIM`）
  - task_preview（已截断）
  - `· N tool uses`（tool_count > 0 时）
  - 状态：`Done`（`theme::SAGE`）/ `Failed`（`theme::ERROR`）

**展开态：**
- Header 行同上
- 每个 agent 显示 task_preview + final_result（第一行，80 字符截断）
- 2 空格缩进 + `theme::SUB_AGENT_BG` 背景色
- `├─`/`└─` 连接线分隔

3. 折叠/展开切换复用现有 Enter 键机制，无需修改事件处理

**注意**: 字符串截断用 `chars().take(N).collect()`（CJK 安全）

**验证**: 手动运行 TUI，触发 3 个并行 SubAgent，确认树形汇总显示正确

---

## Task 5: 历史恢复路径批次聚合

**文件**: `peri-tui/src/app/message_pipeline.rs`

**改动:**

历史恢复路径已在 Task 3 中通过在 `messages_to_view_models()` 末尾调用 `aggregate_batch_groups()` 覆盖。同一 Ai 消息中的多个 Agent ToolUse 在 VM 列表中天然连续，`aggregate_batch_groups()` 的连续区间检测自动生效。

**验证点:**
- 同一 Ai 消息中 3 个 Agent ToolUse → 恢复后显示为树形汇总
- 不同 Ai 消息中的 Agent ToolUse → 各自独立
- 单个 Agent ToolUse → 不聚合

**不需要改动**: `from_base_message_with_cwd` 的 Agent 工具恢复逻辑无需修改

---

## Task 6: 单元测试

### A. message_view.rs 测试

| 测试名 | 场景 |
|--------|------|
| `test_aggregate_batch_groups_single_agent_noop` | 单个 SubAgentGroup 不聚合 |
| `test_aggregate_batch_groups_consecutive_agents` | 3 个连续已完成 SubAgentGroup 合并为 1 个 |
| `test_aggregate_batch_groups_running_agent_skip` | 中间有 is_running=true 时不合并 |
| `test_aggregate_batch_groups_mixed_batch` | 3 完成 + 1 running + 2 完成 = 两个聚合区间 |
| `test_aggregate_batch_groups_already_aggregated_skip` | batch_agents 非空不二次聚合 |
| `test_agent_summary_truncation` | task_preview 超过 50 字符正确截断 |
| `test_batch_group_default_collapsed` | 合并后 collapsed=true |

### B. message_pipeline.rs 测试

| 测试名 | 场景 |
|--------|------|
| `test_batch_info_single_agent_no_batch` | 单个 SubAgentStart+End 不触发批次 |
| `test_batch_info_multi_agent_triggers` | 3 个连续 SubAgentStart+End 触发批次 |
| `test_batch_info_reset_on_done` | Done 事件重置 active_batch |
| `test_batch_info_reset_on_non_agent_tool` | 非 Agent 工具 ToolStart 重置 |
| `test_batch_info_different_batches_no_merge` | 不同批次不合并 |
| `test_build_tail_vms_aggregate_after_all_done` | 全部完成后 build_tail_vms 触发聚合 |

### C. message_render.rs 测试

| 测试名 | 场景 |
|--------|------|
| `test_render_batch_summary_collapsed` | 折叠态：header + N 行 agent 摘要 |
| `test_render_batch_summary_expanded` | 展开态：header + N 个 agent 详情 |
| `test_render_batch_summary_with_error` | 有错误时 header 显示 failed 计数 |
| `test_render_batch_summary_tree_connectors` | `├─`/`└─` 连接线正确 |
| `test_render_single_agent_unchanged` | batch_agents 为空时走现有渲染路径 |

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 批次边界判定不准 | 利用 `stack_depth` + `subagent_stack` 交叉验证；单 agent 时 batch_agents 为空零影响 |
| 聚合时机与 merge_frozen_subagents 冲突 | aggregate_batch_groups 在 merge_frozen_subagents 之后执行 |
| 历史恢复中连续 SubAgentGroup 可能跨批次 | 同一 Ai 消息的 Agent ToolUse 天然连续，跨 Ai 消息不连续 |
| PartialEq/Hash 遗漏 batch_agents | 编译器检测字段遗漏 |
| 树形连接线窄终端错位 | 固定 3 字符前缀，不受终端宽度影响 |

## 无新 crate 依赖

所有改动使用现有代码结构。AgentSummary 使用基本类型，树形连接线使用固定 Unicode 字符，聚合逻辑复用 Vec 操作。

---

### 关键实施文件

- `peri-tui/src/ui/message_view.rs` — SubAgentGroup 数据模型、AgentSummary、aggregate_batch_groups()、PartialEq/Hash
- `peri-tui/src/app/message_pipeline.rs` — BatchInfo、批次检测、build_tail_vms 聚合、messages_to_view_models 历史恢复
- `peri-tui/src/ui/message_render.rs` — render_batch_summary() 树形渲染、SubAgentGroup batch_agents 分支
- `peri-tui/src/ui/theme.rs` — 颜色常量参考
- `peri-tui/src/app/events.rs` — AgentEvent 定义确认
