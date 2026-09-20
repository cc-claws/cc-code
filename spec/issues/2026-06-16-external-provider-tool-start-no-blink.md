# Agent 执行长时间 Bash 工具时 TUI 不显示黄色闪烁指示器

**状态**：Open
**优先级**：中
**创建日期**：2026-06-16
**GitHub Issue**：#174 (https://github.com/cc-claws/cc-code/issues/174)

## 问题描述

Agent 执行长时间 Bash 命令（如 `sleep 30`）时，TUI 的 ToolBlock 不显示黄色 `●` 闪烁指示器。用户无法直观感知工具正在执行。

**影响范围**：所有 Provider（包括 peri 内置 agent），必现。

## 根因分析

### 核心原因：渲染线程 hash-based diffing 导致 ToolBlock 只渲染一次，tick 值被冻结

完整的事件→渲染链路确实无断裂，但 **渲染线程的缓存机制** 导致闪烁动画只在首次渲染时生效一帧，之后 tick 值被冻结。

### 完整链路追踪

#### 1. ToolStart 触发首次重建（正确）

```
ToolStart 事件
  → handle_agent_event(ToolStart)  [agent_ops/mod.rs:147-200]
    → pipeline.handle_event() → 写入 pending_tools
    → request_rebuild()  [agent_render.rs:72-81]
      → build_rebuild_all(prefix_len)
      → apply_pipeline_action(RebuildAll)
        → 替换 view_messages（含 pending ToolBlock: content="", is_error=false）
        → render_rebuild() → 发送 RenderEvent::Rebuild 到渲染线程
        → cache.version += 1
```

#### 2. 渲染线程首次渲染（正确，但只此一次）

```
RenderEvent::Rebuild(messages)
  → rebuild()  [render_thread.rs:360-447]
    → hash diff: 新 ToolBlock hash ≠ 旧 hash → 需要重新渲染
    → render_one()  [render_thread.rs:236-278]
      → tick = STARTUP.elapsed().as_millis() / 200  ← 基于墙钟时间计算
      → render_view_model(vm, ..., tick)  [message_render.rs:417]
        → is_running = content.is_empty() && !is_error  → true
        → format_indicator(Running, tick)  [display.rs:10-21]
          → (tick/4).is_multiple_of(2) → ● 或 空格
      → 返回带指示器的 Line（● 或 空格，取决于此刻 tick 的奇偶）
    → 写入 message_lines[i] 缓存
    → cache.version += 1
```

#### 3. 主循环后续绘制（问题所在）

```
主循环 'event_loop:
  advance_tick()  → spinner tick 推进（与 ToolBlock 无关）
  poll_agent()    → 无新事件（工具执行中，无 ACP 通知）
  next_event()    → 阻塞 50ms，返回 None
  should_render   → loading=true → true
  draw_app()      → 读取渲染线程缓存 cache.lines
                     ↑ 缓存未变！message_lines 仍是首次渲染的结果
                     ↑ tick 值冻结在首次渲染那一刻
                     → ToolBlock 始终显示 ● 或始终显示空格（取决于首次渲染时的 tick 奇偶）
```

#### 4. 为什么不再次 rebuild？

工具执行期间无 ACP 事件 → `poll_agent()` 返回 false → 不触发 `request_rebuild()` → 渲染线程不收到 `RenderEvent::Rebuild` → `rebuild()` 不被调用 → `render_one()` 不被调用 → tick 不更新 → 指示器冻结。

即使主循环因为 `loading=true` 而持续每 33ms 绘制一次，也只是重复读取同一份缓存行。

### 关键代码位置

| 文件 | 行号 | 关键逻辑 |
|------|------|----------|
| `render_thread.rs` | 243-246 | `render_one()` 中 tick 计算：`STARTUP.elapsed().as_millis() / 200`（墙钟时间，但只在 rebuild 时计算一次） |
| `render_thread.rs` | 360-447 | `rebuild()` 中 hash diff：`content_hash` 未变 → 跳过渲染，复用缓存 |
| `render_thread.rs` | 393-407 | hash diff 循环：`i < prefix_stable_len` → continue（复用缓存） |
| `render_thread.rs` | 450-455 | 主循环：只在收到 `RenderEvent::Rebuild` 时调用 `rebuild()` |
| `agent_render.rs` | 72-81 | `request_rebuild()`：直接调用 `build_rebuild_all()`，不走 throttle |
| `main.rs` | 790-814 | `None` 分支：`loading=true` → 持续绘制，但读的是缓存 |
| `display.rs` | 10-21 | `format_indicator(Running, tick)`：闪烁逻辑本身正确 |

### 对比：Spinner 为什么能正常动画？

Spinner 的 tick 由 `main.rs:745` 的 `advance_tick()` 驱动，直接写入 `spinner_state.tick()`。`message_area.rs:43-67` 在每次 `draw_app()` 时**直接读取** `spinner_state.tick()` 构建 spinner 行——不经过渲染线程缓存。所以 spinner 的 tick 每帧都更新。

而 ToolBlock 的 tick 由 `render_thread.rs:246` 的 `STARTUP.elapsed()` 计算，结果**写入缓存的 Line**。后续绘制直接读缓存，tick 不更新。

## 修复方向

### 方案 A：主循环直接重绘 running ToolBlock（推荐）

在 `draw_app()` 中，对 `is_running` 状态的 ToolBlock，用当前墙钟时间重新计算 tick 并替换指示器 Span，不依赖渲染线程缓存。

**优点**：改动最小，不破坏渲染线程的 hash diff 优化
**缺点**：需要在 draw 层面识别 running 状态

### 方案 B：主循环定期强制 rebuild

在主循环的 `None` 分支中，当 `loading=true` 且存在 pending tools 时，定期（如每 200ms）调用 `request_rebuild()` 强制渲染线程重新渲染。

**优点**：复用现有链路
**缺点**：破坏 hash diff 优化，每 200ms 全量重渲染所有消息

### 方案 C：渲染线程内置 tick 定时器

渲染线程在 `run()` 循环中添加 tick 定时器（如每 200ms），检测到有 running ToolBlock 时自动重新渲染。

**优点**：最干净的架构
**缺点**：渲染线程需要理解"running"语义，侵入性较大

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 peri TUI
  2. 发送需要执行长时间 Bash 命令的请求（如 "sleep 30 秒后打印时间"）
  3. 观察 ToolBlock 区域是否显示黄色 `●` 闪烁
- **预期**：Bash 执行期间 ToolBlock 显示黄色 `●` 闪烁 + 工具名青色加粗
- **实际**：指示器冻结在首次渲染的状态（始终 ● 或始终空格），无闪烁动画

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-16 | — | Open | agent | 创建：所有 Provider 下 agent 执行长时间 Bash 时 TUI 不显示黄色闪烁 |
| 2026-06-16 | Open | Open | agent | 根因确认：渲染线程 hash-based diffing 导致 ToolBlock 只渲染一次，tick 值被冻结在首次渲染时。主循环持续绘制但读的是缓存行，tick 不更新。核心矛盾：blink 动画需要每帧更新 tick，但渲染缓存只在 rebuild 时更新 |
