> 归档于 2026-06-24，原路径 spec/issues/2026-06-13-shell-command-success-stderr-red.md

# Shell 命令（!）成功执行时 stderr 输出显示红色，误导用户

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-13

## 问题描述

用户通过 `!` 执行 shell 命令，命令成功（exit 0）但有 stderr 输出时，stderr 行显示为红色（`theme::ERROR`），与失败状态视觉上无法区分，用户误以为命令失败。

## 根因

`render_shell_command`（`message_render.rs:352-357`）中，stderr 行无条件使用 `theme::ERROR` 红色，不考虑 exit code：

```rust
let default_style = if *is_error {
    Style::default().fg(theme::ERROR)  // stderr 永远红色
} else {
    Style::default().fg(theme::MUTED)
};
```

许多常用命令成功时也写 stderr（如 `git status`、`cargo build` 的进度信息、`npm` 的 warn 信息），导致成功命令的输出看起来像失败。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 输入 `!git status` 或任何成功但有 stderr 输出的命令
  2. 观察输出行颜色
- **环境**：所有 OS

## 涉及文件

- `peri-tui/src/ui/message_render.rs`（第 352-357 行）—— `render_shell_command` 中 stderr 样式逻辑

## 建议修复

当 exit code == 0 时，stderr 行不用 ERROR 红色，改用 WARNING（黄/橙）或 DIM，区分"成功的警告输出"和"真正的失败"：

```rust
let default_style = if *is_error {
    if exit_code == Some(0) {
        Style::default().fg(theme::WARNING)  // 成功但有 stderr → 警告色
    } else {
        Style::default().fg(theme::ERROR)    // 失败 → 红色
    }
} else {
    Style::default().fg(theme::MUTED)
};
```

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-13 | — | Open | agent | 创建 |
| 2026-06-13 | Open | Fixed | agent | 修复：exit code == 0 时 stderr 用 MUTED 色（与 stdout 一致），仅失败时用 ERROR 红色。commit: 14fa178a (hotfix/shell-stderr-color) |
