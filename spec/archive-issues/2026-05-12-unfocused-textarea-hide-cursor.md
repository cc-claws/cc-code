> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-unfocused-textarea-hide-cursor.md

# 输入框光标应随聚焦状态隐藏

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-12
**修复日期**：2026-05-12

## 问题描述

在以下两种情况下，输入框的光标应该隐藏：
1. **多 session 分屏**：非聚焦 session 的输入框光标仍在闪烁
2. **应用失焦**：切换到其他终端窗口时，TUI 的输入框光标仍在闪烁

这造成视觉混淆——用户无法一眼看出当前哪个窗口/session 是活跃的。

## 症状详情

### 多 session 分屏

| 状态 | 边框颜色 | ❯ 前缀颜色 | 输入框光标 |
|------|---------|------------|-----------|
| 聚焦 | `theme::ACCENT`（紫色） | `theme::TEXT`（白色）+ BOLD | 显示（反色闪烁） |
| 非聚焦 | `theme::BORDER_DIM`（灰色） | `theme::MUTED`（灰色） | **仍显示（反色闪烁）** ← 问题 |

### 应用失焦

| 状态 | 输入框光标 |
|------|-----------|
| 应用聚焦 | 显示（反色闪烁） |
| 应用失焦 | **仍显示（反色闪烁）** ← 问题 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. **分屏模式**：启动 TUI 并创建至少 2 个 session，观察非聚焦 session 的输入框
  2. **应用失焦**：启动 TUI，使用 `Cmd+Tab` 切换到其他终端窗口，观察 TUI 的输入框

## 期望行为

- 非聚焦 session 的输入框光标应该隐藏
- 应用失焦时，所有输入框光标应该隐藏

## 相关代码

- `peri-tui/src/main.rs:130-135` — 终端初始化（需要添加 `EnableFocusChange`）
- `peri-tui/src/event.rs:215-225` — 焦点事件处理（新增 `FocusGained` / `FocusLost` 分支）
- `peri-tui/src/app/mod.rs:114-120` — App 结构体（新增 `focused: bool` 字段）
- `peri-tui/src/ui/main_ui.rs:260-273` — 输入框渲染（根据 `is_active` 和 `app.focused` 决定光标样式）

## 技术背景

### tui-textarea 光标控制

`tui-textarea` 提供的光标控制 API：
- `set_cursor_style(Style)` — 设置光标样式
- 默认 `cursor_style` 为 `Style::default().add_modifier(Modifier::REVERSED)`（反色）
- 移除 `REVERSED` 修饰符后，光标与普通文本无异，视觉上等同于隐藏

### 终端焦点事件

crossterm 的焦点事件**不是默认启用的**，需要手动发送 `EnableFocusChange` 命令（ANSI escape sequence `[?1004h`）。

启用后，终端会发送以下事件：
- `Event::FocusGained` — 应用获得焦点
- `Event::FocusLost` — 应用失去焦点

## 修复方案

### 1. 终端初始化时启用焦点事件（`main.rs`）

```rust
use ratatui::crossterm::event::{EnableFocusChange, DisableFocusChange};

execute!(
    stdout,
    EnterAlternateScreen,
    EnableMouseCapture,
    EnableBracketedPaste,
    EnableFocusChange  // ← 新增
)?;
```

### 2. App 结构体添加焦点状态（`app/mod.rs`）

```rust
pub struct App {
    pub session_mgr: SessionManager,
    pub services: ServiceRegistry,
    pub global_panels: panel_manager::PanelManager,
    pub focused: bool,  // ← 新增
}
```

### 3. 事件处理中监听焦点变化（`event.rs`）

```rust
async fn handle_event(app: &mut App, ev: Event) -> Result<Option<Action>> {
    match ev {
        Event::FocusGained => {
            app.focused = true;
            return Ok(Some(Action::Redraw));
        }
        Event::FocusLost => {
            app.focused = false;
            return Ok(Some(Action::Redraw));
        }
        // ... 其他事件处理
    }
}
```

### 4. 渲染时根据焦点状态隐藏光标（`main_ui.rs`）

```rust
// 应用失焦 或 session 未激活 → 隐藏光标
let should_hide_cursor = !app.focused || !is_active;
if should_hide_cursor {
    let mut ta = textarea_ref.clone();
    ta.set_cursor_style(Style::default().fg(theme::DIM));  // 移除 REVERSED
    f.render_widget(&ta, chunks[5]);
} else {
    f.render_widget(textarea_ref, chunks[5]);
}
```
