> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-multi-agent-tree-summary-view.md
# 多 SubAgent 并发/连续调用的视图过于冗长，需要树形汇总视图

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-13

## 问题描述

当主 agent 并行 dispatching 或顺序调用多个 SubAgent 时，每个 SubAgent 都渲染为独立的 `SubAgentGroup` 块（展开显示内部工具调用和消息），占满大量屏幕空间。用户期望类似 Claude Code 的紧凑树形汇总视图——多个 agent 合并为一个可折叠的树，默认折叠，每个 agent 只占一行摘要。

### 期望效果（参照 Claude Code）

```
⏺ 9 agents finished (ctrl+o to expand)
   ├─ 创建用户文档首页 · 2 tool uses · 0 tokens
   │  ⎿  Done
   ├─ 创建大模型配置文档 · 8 tool uses · 0 tokens
   │  ⎿  Done
   ├─ 创建Agent管理文档 · 5 tool uses · 0 tokens
   │  ⎿  Done
   └─ 创建故障排查文档
```

### 当前实际效果

每个 SubAgent 渲染为独立的展开块：

```
❯ Agent(code-reviewer)
  Review the implementation...
  ❯ Agent(type)
    task_preview...
    [展开的内部工具调用消息 1]
    [展开的内部工具调用消息 2]
    [展开的内部工具调用消息 3]
  ⎿ final_result 摘要...

❯ Agent(explorer)
  Explore the codebase...
  [展开的内部工具调用消息 1]
  [展开的内部工具调用消息 2]
  ⎿ final_result 摘要...
```

## 症状详情

| 维度 | 描述 |
|------|------|
| 触发场景 | 并行 dispatching 多个 agent（如 `dispatching-parallel-agents`）或顺序调用多个 sub-agent |
| 核心问题 | 每个 SubAgent 独立展开，内部消息全部显示，占用过多垂直空间 |
| 期望行为 | 同批次 agent 合并为树形汇总，默认折叠，每行显示名称/工具数/token数/状态 |
| 非目标 | 独立调用的 agent（不同批次、中间有用户消息隔开）保持独立块，不合并 |

### 合并条件

- **同批次合并**：同一个工具调用批次（如 `dispatching-parallel-agents` 一次发起的多个 agent）合并为树形汇总
- **独立 agent 保持独立**：不同批次、被用户消息隔开的 SubAgent 不合并

### 每行摘要信息

| 信息 | 来源 |
|------|------|
| 任务描述 | `task_preview` |
| 工具调用数 | `total_steps` 或 `tool_calls_count` |
| Token 数 | 需新增字段（当前 `SubAgentGroup` 未携带 token 统计） |
| 完成状态 | `is_running` / `is_error` → `Done` / `Running...` / `Failed` |

## 现状数据

### 当前 SubAgentGroup 数据结构

- `peri-tui/src/ui/message_view.rs:249-268` — `SubAgentGroup` 变体定义
- `peri-tui/src/ui/message_view.rs:844-862` — `subagent_group()` 构造函数

### 当前渲染逻辑

- `peri-tui/src/ui/message_render.rs:295-441` — `SubAgentGroup` 渲染（展开/折叠两种状态）
- 展开时渲染 header + task_preview + 内部消息（滑动窗口最多 4 条）+ final_result
- 折叠时渲染 header + task_preview（2 行）

### 缺失能力

1. **无批次聚合**：当前每个 SubAgent 独立渲染，没有「同批次 agent 汇总」的概念
2. **无 token 统计**：`SubAgentGroup` 未携带 token 消耗数据，需要从 `SubAgentEnd` 事件中获取
3. **无树形布局**：当前没有树形连接线（`├─`/`└─`）的渲染能力

## 涉及文件

- `peri-tui/src/ui/message_view.rs`（249-268 行）— `SubAgentGroup` 数据结构，需新增 token 统计字段
- `peri-tui/src/ui/message_render.rs`（295-441 行）— SubAgent 渲染逻辑，需新增树形汇总渲染
- `peri-tui/src/app/message_pipeline.rs` — Pipeline 层需识别同批次 agent 并聚合为 `AgentSummaryGroup`
- `peri-agent/src/agent/executor/` 或中间件层 — `SubAgentEnd` 事件需携带 token 统计
- `peri-tui/src/app/events.rs` — 可能需要新增批次标记字段

## 期望改进方向

1. 新增 `AgentSummaryGroup` VM 变体，将同批次 SubAgent 聚合为一个可折叠树
2. 默认折叠状态，每行显示：`任务描述 · N tool uses · M tokens` + 状态标签
3. 展开后显示每个 agent 的完整 `SubAgentGroup` 内容（当前展开效果）
4. 树形连接线（`├─`/`└─`）在折叠状态渲染
