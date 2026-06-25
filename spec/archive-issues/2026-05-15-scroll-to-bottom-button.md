> 归档于 2026-05-16，原路径 spec/issues/2026-05-15-scroll-to-bottom-button.md

# 消息列表面板缺少「滚动到底」快捷按钮

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-15
**修复日期**：2026-05-15

## 问题描述

当用户在消息列表中向上滚动查看历史消息后（`scroll_follow = false`），没有任何可视化的 UI 元素提示用户可以快速回到底部。用户必须手动向下滚动或提交新消息才能回到最新内容。应在消息列表右侧添加一个下箭头按钮，当存在未显示的最底部内容时显示，点击后直接滚动到底。

## 症状详情

| 场景 | 当前行为 | 期望行为 |
|------|----------|----------|
| 用户向上滚动查看历史 | 无提示，需手动滚回 | ✅ 右侧显示 ▲ 置顶按钮 |
| 用户向下滚动查看尾部 | 无提示，需手动滚回 | ✅ 右侧显示 ▼ 置底按钮 |
| 用户点击 ▲ 按钮 | N/A | ✅ 立即滚动到顶部 |
| 用户点击 ▼ 按钮 | N/A | ✅ 立即滚动到底部 |
| 用户已在顶部（`offset == 0`） | N/A | ✅ 不显示 ▲ 按钮 |
| 用户已在底部（`scroll_follow == true`） | N/A | ✅ 不显示 ▼ 按钮 |

## 期望行为

在消息列表渲染区域的右侧（与现有滚动条共用列），显示箭头按钮：

- **▲ 置顶按钮**：当 `offset > 0` 时，在滚动条列顶部（`inner.y`）显示，点击后 `scroll_offset = 0`
- **▼ 置底按钮**：当 `scroll_follow == false`（即用户已滚离底部）时，在滚动条列底部（`inner.bottom().saturating_sub(1)`）显示，点击后 `scroll_follow = true`，`scroll_offset = u16::MAX`

## 修复方案

**Commit**: `86fa2ae`

### 修改文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/app/thread_ops.rs` | 新增 `scroll_to_bottom()`（+8 行）和 `scroll_to_top()`（+10 行） |
| `peri-tui/src/ui/main_ui.rs` | 在 `render_messages()` 中渲染 ▲/▼ 按钮（+19 行），灰色 `theme::MUTED` |
| `peri-tui/src/event.rs` | 在 `MouseEventKind::Down(Left)` 中拦截按钮区域点击（+22/-5 行） |

### 按钮显示逻辑

- ▲：`offset > 0`，位于消息区域右上角（`inner.y`），1 行高
- ▼：`offset < max_scroll`（等价 `!scroll_follow`），位于消息区域右下角，1 行高

### 点击判定

- 右栏 2 列 × 顶部 2 行 → `scroll_to_top()`
- 右栏 2 列 × 底部 2 行 → `scroll_to_bottom()`
- 击穿时继续走文本选区逻辑

## 相关代码

- `peri-tui/src/ui/main_ui.rs:476-533` —— 消息列表滚动状态计算（`scroll_offset`、`scroll_follow`、`max_scroll`）
- `peri-tui/src/ui/main_ui.rs:655-678` —— 消息列表渲染（sticky header、Paragraph scroll、Scrollbar）
- `peri-tui/src/app/ui_state.rs:9-10` —— `scroll_offset` 和 `scroll_follow` 字段定义
- `peri-tui/src/event.rs:1048-1078` —— 鼠标滚轮事件处理
