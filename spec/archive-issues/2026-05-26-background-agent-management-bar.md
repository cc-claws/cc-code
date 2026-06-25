> 归档于 2026-05-27，原路径 spec/issues/2026-05-26-background-agent-management-bar.md

# 后台 SubAgent 统一管理栏

**状态**：Fixed
**优先级**：中
**类型**：Feature
**创建日期**：2026-05-26

## 问题描述

当前通过 Agent 工具以 `run_in_background: true` 启动的后台 SubAgent 只在状态栏显示一个计数器，用户无法看到有哪些后台 agent 在运行、每个 agent 在做什么，也无法主动切换查看某个 agent 的输出。需要新增一个统一的后台 agent 管理 UI 区域。

## 期望行为

在状态栏下方新增一个后台 agent 列表栏，支持：

- 显示所有正在运行的后台 SubAgent（每行一个）+ "main" 主会话入口
- 用独立快捷键从输入框跳转到此区域
- 上下键选择、Enter 进入该 agent 的聚焦视图
- 聚焦视图下消息区域只显示该 SubAgent 的输出，输入框边框变色并标注 agent 名称
- SubAgent 结束后自动切回 main

## 症状详情

### 现状

| 维度 | 现状 | 差距 |
|------|------|------|
| 数据追踪 | `background_task_count: usize` 仅一个计数器 | 无法知道有哪些 agent、各自状态 |
| 启动事件 | `handle_subagent_start` 只 `+1` counter | 丢失 agent_id、instance_id、task_preview |
| 完成事件 | `handle_background_task_completed` 只 `-1` counter | 完成后无额外处理 |
| UI 展示 | 状态栏显示「N 个后台任务」计数 | 无 agent 列表、无切换入口 |
| 切换能力 | 无 | 无法单独查看某个后台 agent 的输出 |

### 涉及文件

- `peri-tui/src/app/chat_session.rs` — `background_task_count: usize`（需扩展为列表）
- `peri-tui/src/app/agent_ops/subagent.rs:75` — `handle_subagent_start`（仅 counter +1）
- `peri-tui/src/app/agent_events_bg.rs:62` — `handle_background_task_completed`（仅 counter -1）
- `peri-tui/src/ui/main_ui/status_bar.rs:170` — 状态栏渲染后台任务计数
- `peri-tui/src/ui/main_ui/mod.rs` — 底部布局约束定义
- `peri-tui/src/event/keyboard.rs` — 快捷键注册
- `peri-tui/src/app/message_pipeline.rs` — 消息流可能需过滤支持

## 设计要点

### 1. 数据模型

将 `background_task_count: usize` 替换为：

```rust
struct RunningBgAgent {
    agent_id: String,       // SubAgent 的 agent_id (如 "code-reviewer")
    instance_id: String,    // 此次调用的唯一实例 ID
    task_preview: String,   // 用户传入的任务摘要
    status: BgAgentStatus,  // Running / Completing
    color: Color,           // 分配给此 agent 的标识色
}
background_agents: Vec<RunningBgAgent>,
```

`main` 始终排在列表第一位（不是 `RunningBgAgent`，是固定项）。

### 2. UI 布局

修改 `render_session_column()` 中的底部约束，在 status_bar 下方新增 `background_agent_bar`（高度固定 1+N 行，N = min(agent_count, max_visible)）。

```
[sticky_header]
[messages]
[attachment bar]
[panel_height]
[queued_height]
[input_height]
[status_bar]
[background_agent_bar]  ← 新增
```

### 3. 键盘交互

- 新增快捷键（建议 `Ctrl+J` 或 `Ctrl+B`）从输入框跳转到此区域
- 区域内：↑↓ 移动选中、Enter 进入聚焦视图、Esc 退出聚焦回到 main
- 焦点在此区域时，输入框不可编辑（视觉反馈：输入框变暗）

### 4. 聚焦视图

进入某个后台 agent 的聚焦视图后：

- **消息区域**：只显示该 agent（由 `instance_id` 匹配）的 SubAgentGroup/ToolCallGroup 消息
- **输入框边框**：颜色变为该 agent 的标识色，上边框右侧显示 `[agent_name]`
- **自动退出**：收到该 agent 的 `BackgroundTaskCompleted` 事件时自动切回 main 视图
- 所有后台 agent 完成后（列表为空），bar 自动隐藏

### 5. 事件流

```
SubAgentStart(bg=true) → 追加 RunningBgAgent 到列表 → 渲染 bar
BackgroundTaskCompleted  → 标记对应 agent 状态为 Completing → (如当前聚焦此 agent) 切回 main → 移除此 agent → (如列表为空) 隐藏 bar
```
