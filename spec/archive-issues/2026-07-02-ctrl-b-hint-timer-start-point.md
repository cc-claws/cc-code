# Ctrl+B 提示计时起点错误：应从 Bash 实际执行开始，而非 ToolStart 事件到达 TUI

**状态**: Fixed (已归档)
**创建日期**: 2026-07-02
**严重程度**: P3
**平台**: 全平台

## 问题描述

TUI 中 Bash 工具运行超过 2 秒后显示的 `"(Ctrl+B to run in background)"` 提示，其计时起点是 `ToolStart` 事件到达 TUI 并创建 `MessageViewModel::ToolBlock` 的时刻，而非 Bash 子进程实际 spawn 的时刻。

用户视角：看到的"已运行时间"比 Bash 实际执行时间偏长。

## 根因分析

### 当前计时起点

`started_at` 在两处被设置为 `Instant::now()`：

1. `peri-tui/src/app/message_pipeline/transform.rs:142` — `build_tool_start_vm()` 构建 ToolBlock VM 时
2. `peri-tui/src/ui/message_view/mod.rs:931` — `MessageViewModel::tool_block_pending()` 构造函数

两者都在收到 `ToolStart` 事件时记录时间。

### 实际执行时序

`peri-agent/src/agent/executor/tool_dispatch.rs` 中的三阶段流程：

```
阶段一 (L221-290): before_tool 中间件链 → emit ToolStart (L245)
阶段二 (L292-338): tool.invoke(input) ← Bash 实际在此 spawn
阶段三 (L342+):    收集结果
```

`ToolStart` 在阶段一 emit，Bash 在阶段二才实际执行。两者之间存在：
- `before_tool` 中间件链剩余处理时间（多工具并发时更明显）
- ACP transport 延迟
- TUI pipeline 处理延迟

### 渲染逻辑

`peri-tui/src/ui/message_render.rs:672-694`：

```rust
// 2 秒阈值提示
if !moved_to_background
    && exit_code.is_none()
    && started_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(2))
{
    // 显示已运行时间 + "(Ctrl+B to run in background)"
}
```

### 触发重建

`peri-tui/src/ui/render_thread.rs:471-489`：`running_bash_needs_control_b_hint_rebuild()` 检查 `started_at.elapsed() >= 2s`，满足时每 tick 重建 UI。

## 修复方案

将 `started_at` 的计时起点从 ToolStart 事件改为 Bash 实际 spawn 时刻。可选方案：

**方案 A**：在 `ShellHandle` 创建时通过新事件（如 `ShellSpawned { task_id, spawned_at }`）回传给 TUI，TUI 收到后更新 `started_at`。

**方案 B**：在 BashTool 的 `invoke` 入口记录时间，通过 `ToolResult` 或中间事件回传。

方案 A 更精确，但需要新增事件类型；方案 B 改动较小但仍有 transport 延迟。

## 影响范围

- `peri-tui/src/ui/message_render.rs` — 渲染逻辑
- `peri-tui/src/ui/message_view/mod.rs` — ToolBlock VM 构造
- `peri-tui/src/app/message_pipeline/transform.rs` — ToolStart VM 构建
- `peri-tui/src/ui/render_thread.rs` — 2 秒阈值触发重建
- `peri-agent/src/agent/executor/tool_dispatch.rs` — ToolStart emit 时机
