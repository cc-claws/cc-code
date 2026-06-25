> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-ctrlc-blocked-cannot-exit.md

# Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 修复方案

在 wizard 拦截块开头检测 Ctrl+C，启动双重确认退出流程（与正常主界面 Ctrl+C 行为一致）。第一次 Ctrl+C 设置 `quit_pending_since`，2 秒内再次按下则退出。

## 问题描述

Setup Wizard 的事件拦截块在 `event.rs:320-367` 捕获所有 Key 事件。`handle_setup_wizard_key` 不识别 Ctrl+C，返回 `None` 后事件循环无条件返回 `Redraw`。原本在第 540 行的全局 Ctrl+C 处理器永远无法到达。用户在 Wizard 中完全无法退出程序。

## 症状详情

| 现象 | 详情 |
|------|------|
| Ctrl+C 无效 | 按 Ctrl+C 无任何反应，不中断也不退出 |
| 无其他退出方式 | Language 步的 Esc 可退出（from_command=false 时），但 Form/Edit 步 Esc 只能回退到上一步 |
| 阻塞级体验 | 用户可能被迫 kill -9 进程 |

## 根因

`peri-tui/src/event.rs:320-367`

```rust
if app.global_ui.setup_wizard.is_some() {
    // ... handle_setup_wizard_key ...
    return Ok(Some(Action::Redraw));  // ← 无条件 Redraw，跳过后续所有处理器
}
// Ctrl+C handler at line 540~ never reached
```

## 期望

Setup Wizard 拦截块应在调用 `handle_setup_wizard_key` 之前先检查 Ctrl+C（或其他退出组合键），允许用户在任何步骤退出或中断。

或者 `handle_step_*` 函数中增加对 Ctrl+C 的显式处理，返回 Skip 或 Quit 动作。

## 涉及文件

- `peri-tui/src/event.rs` —— line 320-368 (Wizard 事件拦截)
- `peri-tui/src/app/setup_wizard.rs` —— `handle_step_*` 各函数
