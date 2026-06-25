> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-streaming-resize-cpu-spike.md

# 流式加载中拖动改变 TUI 窗口宽度，CPU 瞬间暴涨

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-14

## 问题描述

Agent 流式输出期间（loading 状态），拖动 macOS 终端窗口边缘连续改变横向宽度时，CPU 瞬间飙升至异常高位，界面严重卡顿。仅在加载期间发生，空闲状态下 resize 影响不大。

## 症状详情

- **触发条件**：Agent 正在流式输出（`loading=true`），此时拖动终端窗口边缘连续改变宽度
- **表现**：CPU 使用率瞬间暴涨，渲染线程被大量 Resize 事件淹没
- **严重程度**：影响流式输出体验，在历史消息较多时尤其严重

## 根因分析

问题发生在 **渲染线程 Resize 事件未做节流/去抖** + **流式期间 Resize 与 Rebuild 事件叠加** 两个因素的交互：

### 事件链路

```
拖动 resize  →  crossterm Event::Resize × N/sec
                    ↓
event.rs:264   仅清除 text_selection，返回 Action::Redraw
                    ↓
每帧 render  →  render_messages() (main_ui.rs:462)
    cache.width ≠ text_area_width → send RenderEvent::Resize(new_width)
                    ↓ (× 每帧一次，直到 cache.width 更新)
渲染线程       收到 Resize → message_hashes.clear() → 全量 rebuild(last_messages)
                    ↓
               render_one() × 所有消息 → Markdown 解析 + Paragraph::line_count 换行计算
               build_wrap_map() → 对每条 Line 再做一遍 line_count 计算
```

### 关键代码路径

1. **render_messages.rs:462-467**：每帧比较 `cache.width` 与 `text_area.width`，不等时发送 `RenderEvent::Resize`。渲染线程处理前，连续多帧都会发送 Resize 事件，造成队列积压
2. **render_thread.rs:376-384**：`Resize` 处理器 `self.message_hashes.clear()` 强制全量重建，`prefix_stable_len` 退化为 0，所有消息重新走 `render_one()`（Markdown 解析 + 换行计算）
3. **render_thread.rs:119-143**：`build_wrap_map()` 对每条 Line 调用 `Paragraph::line_count(width)`——此调用在有大量消息时消耗显著
4. **流式叠加**：流式期间的 `check_throttle()`（100ms 节流）会额外生成 `Rebuild` 事件，与积压的 Resize 事件在渲染线程队列中交替处理，进一步放大计算量

### 为什么加载期间特别严重

- 空闲时：消息列表稳定，resize 事件虽重复但 `last_messages` 不变，hash clear 后确实需要重建，但消息数量通常较少
- 加载时：流式 chunk 不断到达 → `last_messages` 不断更新 → 每次 Resize 重建的都是**最新**的消息列表 → 消息列表逐渐增长 → 计算量递增
- 额外叠加：流式本身的 100ms RebuildAll 与 resize 引发的 Rebuild 互相竞争，render thread 持续饱和

## 涉及文件

- `peri-tui/src/ui/main_ui.rs:462-467` —— 每帧发送 Resize 事件，无去抖/节流
- `peri-tui/src/ui/render_thread.rs:376-384` —— Resize 事件处理，hash 全量清除
- `peri-tui/src/ui/render_thread.rs:119-155, 258-345` —— `build_wrap_map()` 和 `rebuild()`，全量重建逻辑

## 修复

### 1. 发送端去抖（`main_ui.rs`）

新增 `MessageState::last_resize_width` 字段，记录上次已发送的 resize 宽度。仅当 text_area 宽度与上次发送的不同时才发送新的 `RenderEvent::Resize`。拖动 resize 期间宽度可能在 2-3 个值间来回切换，之前每帧都发送（60fps 时为 60 次/秒），现在仅在宽度切换时发送（约 2-3 次/整个拖动过程）。

### 2. 接收端合并（`render_thread.rs`）

`Resize` 事件处理中新增 drain coalescing：收到一个 Resize 后，用 `try_recv()` 将所有积压的 Resize 事件合并为最后一个宽度，仅执行一次全量重建。与发送端去抖组合后，render thread 队列中几乎不存在积压的 Resize 事件（仅约 2-3 个/拖动过程）。

### 变更文件

- `peri-tui/src/app/message_state.rs` —— 新增 `last_resize_width` 字段
- `peri-tui/src/ui/main_ui.rs` —— resize 发送去抖
- `peri-tui/src/ui/render_thread.rs` —— drain coalescing
