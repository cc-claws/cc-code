# Issue #145: agent Bash 运行提示与生命周期切换时的终端清屏闪烁

**状态**: Fixed (已归档)
**创建日期**: 2026-09-18  
**严重程度**: P2  
**平台**: 全平台（Windows ConPTY 下尤为明显）  
**GitHub Issue**: [#145](https://github.com/cc-claws/cc-code/issues/145)  

## 问题描述

用户在执行耗时命令（如 `sleep 10` 等前台 Bash 工具）时，界面在执行超过 2 秒出现提示：
```text
  ⎿ Running… (Xs)
    (ctrl+b to run in background)
```
或命令执行完成、后台化切换时，TUI 会极快地黑屏/白屏闪烁一下，无法平滑过渡。

## 根因分析

### 1. 物理清屏 `terminal.clear()` 被显式触发（直接元凶）

- 在 `peri-tui/src/app/agent_shell_executor.rs:33` 中定义了前台注册防抖延迟：
  ```rust
  const FOREGROUND_REGISTRATION_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
  ```
- 当前台命令执行满 2 秒时，异步任务向主循环发送 `AgentShellRegistration`。
- 主循环调用 `peri-tui/src/app/shell_command.rs:554-558` 中的 `register_agent_shell`：
  ```rust
  self.set_agent_bash_tool_started_at(&slot.command, slot.started_instant);
  self.session_mgr.current_mut().agent_shells.push(slot);
  self.session_mgr
      .current_mut()
      .ui
      .request_terminal_clear_redraw(); // 触发全屏清屏标记
  self.render_rebuild();
  ```
- 类似地，`background_agent_foreground`（L635）和 `poll_agent_shells`（L695）在生命周期切换或退出时也均调用了 `request_terminal_clear_redraw()`。
- 该标记在下一帧由 `peri-tui/src/main.rs:1093` 的 `draw_app` 消费：
  ```rust
  if app.session_mgr.current_mut().ui.take_terminal_clear_redraw() {
      terminal.clear()?; // 向 stdout 输出 \x1b[2J 并重置双缓冲区
  }
  ```
- `terminal.clear()` 物理抹黑终端并清空 Ratatui double buffer，导致整屏在几毫秒内闪烁。

### 2. 全局 Hash 缓存被高频清空（次级震荡）

- 在 `peri-tui/src/ui/render_thread.rs:496-501` 中：
  ```rust
  if self.running_bash_needs_control_b_hint_rebuild() {
      self.message_hashes.clear();
      let messages = self.last_messages.clone();
      self.rebuild_safe(messages);
      return true;
  }
  ```
- 为更新 `Running… (Xs)` 的秒数，每 200ms tick 粗暴清空了 `message_hashes`，导致 `prefix_stable_len` 恒为 0，使整个会话中所有历史消息每秒被全量重新解析渲染 5 次，产生持续的性能压力与终端抖动。

### 3. 对照验证：为什么短 Bash（如 ≤1s）完全不抖动？

代码在 `peri-tui/src/app/agent_shell_executor.rs:288-298` 中设计了前台防抖：
```rust
tokio::spawn(async move {
    tokio::select! {
        _ = tokio::time::sleep(FOREGROUND_REGISTRATION_DELAY) => { // 2 秒到期
            if !exit_signal_for_registration.is_exited() {
                let _ = registration_tx.send(registration);
            }
        }
        _ = exit_signal_for_registration.wait() => {} // 命令短于 2 秒直接退出
    }
});
```
- **对于短命令（如 1 秒）**：进程在 2 秒前已执行完毕并触发 `exit_signal`，`select!` 命中 `wait()` 分支，注册直接被丢弃；
- **清屏从未被触发**：`register_agent_shell` 从未被调用，`request_terminal_clear_redraw()` 永远不会执行，`terminal.clear()?` 为 0 调用；
- **提示行从未生成**：`started_at` 始终为 `None`，ToolBlock 始终保持 1 行 Header，`running_bash_needs_control_b_hint_rebuild` 始终为 false，不会清空 hash；
- **对照结论**：只有超过 2 秒的长命令才会进入该注册链路并触发 `terminal.clear()` 物理清屏，形成 100% 对齐的因果反证。

## 修复方案

1. **移除物理清屏标记调用**：
   - 移除 `register_agent_shell`、`background_agent_foreground`、`poll_agent_shells` 中调用的 `request_terminal_clear_redraw()`；
   - 依赖 Ratatui 原生单元格级 Diff Double-Buffering 进行平滑重绘（`message_area` 与 `status_bar` 原本每帧已有各自局部 `Clear` widget 保护）。
2. **优化秒数刷新与缓存复用**：
   - 优化 `render_thread.rs` 中的计时刷新机制，移除 `self.message_hashes.clear()`；
   - 仅针对正在运行且超过 2 秒的 Bash ToolBlock，在秒数变化时做增量更新，历史前缀消息 100% 复用缓存。
3. **更新回归测试**：
   - 更新 `shell_command_test.rs` 中断言 `force_terminal_clear_redraw` 的测试用例。
