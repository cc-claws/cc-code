# Ctrl+C 偶发直接退出（Windows Terminal）

**状态**: Open
**创建日期**: 2026-06-27
**严重程度**: P1
**平台**: Windows (ConPTY / Windows Terminal)

## 问题描述

在 Windows Terminal 下使用 cc-code 时，按一次 Ctrl+C 偶发直接退出程序，而非预期的中断 Agent 或进入 quit-pending 状态。

## 根因分析

### 核心问题：单次 Ctrl+C 产生两个 KeyDown 事件

`peri-tui/src/main.rs:449-498` 中注册了 `ctrl_handler`，在收到 `CTRL_C_EVENT` 时向 STD_INPUT 注入 KeyDown + KeyUp 事件对。

但在 Windows Terminal (ConPTY) 下，原始的 KEY_EVENT 可能残留在输入缓冲区中，导致 crossterm 读到**两个** KeyDown 事件：

```
时序：
1. crossterm 读到原生 KeyDown(Ctrl+C)
   → handle_ctrl_c → loading=false → 设置 quit_pending_since = now
2. crossterm 读到 ctrl_handler 注入的 KeyDown(Ctrl+C)
   → handle_ctrl_c → quit_pending_since 存在且 < 2s → Action::Quit → 退出
```

两个事件间隔约 0-1ms，等同于单次按键。

### 为什么是偶发

- 取决于 Windows Terminal 版本和 ConPTY 行为（是否同时保留 KEY_EVENT）
- 取决于 `poll(50ms)` 时序——两个事件是否在同一 poll 周期
- Agent 运行中（`loading=true`）时第一个 Ctrl+C 走 interrupt 路径不设置 `quit_pending_since`，不会触发退出

## 修复方案

在 `handle_ctrl_c` 的 quit-pending 判断前加 100ms 最小间隔防抖：

```rust
if let Some(since) = app.global_ui.quit_pending_since {
    if since.elapsed() < Duration::from_millis(100) {
        return None; // 重复事件，忽略
    }
    if since.elapsed() < Duration::from_secs(2) {
        return Some(Action::Quit);
    }
    // ...
}
```

100ms 覆盖重复事件（0-1ms），不影响正常双击退出（人类间隔 200ms+）。

## 涉及文件

- `peri-tui/src/event/keyboard/normal_keys.rs:431-450` — `handle_ctrl_c` 函数
- `peri-tui/src/main.rs:449-498` — Windows `ctrl_handler` 注入逻辑
