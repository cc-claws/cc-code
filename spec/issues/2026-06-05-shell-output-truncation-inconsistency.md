---
id: 2026-06-05-shell-output-truncation-inconsistency
title: Shell 输出详细模式截断不一致（40 行硬限制 vs ToolResult 无限制）
status: open
priority: medium
created: 2026-06-05
github_issue: #170
github_url: https://github.com/cc-claws/cc-code/issues/170
---

## 问题

Shell 输出（Bash/Grep/Glob 等工具的 stdout+stderr）在详细模式（Ctrl+O）下仍有 40 行硬限制，而 ToolResult 在详细模式下使用 `usize::MAX` 无限制。用户切换到详细模式后，期望看到完整输出，但实际上仍被截断。

## 代码分析

### 问题位置

`peri-tui/src/ui/message_render.rs`

### 截断逻辑对比

| 内容类型 | 默认模式 | 详细模式（Ctrl+O） | 代码位置 |
|---------|---------|------------------|---------|
| **Shell 输出** | 6 行 | 40 行（仍有硬限制） | 第 324-325 行 |
| **ToolResult** | 20 行 | `usize::MAX`（无限制） | 第 618 行 |
| **Reasoning** | 折叠 | 完整显示 | 第 484-485 行 |

### 关键代码片段

**Shell 输出截断（第 324-325 行）- 有问题：**
```rust
let max_lines = if detail_mode {
    SHELL_OUTPUT_DETAIL_LINES  // 40 行，仍有硬限制
} else {
    SHELL_OUTPUT_COLLAPSED_LINES // 6 行
};
```

**ToolResult 截断（第 618 行）- 正确实现：**
```rust
// 详细模式显示完整内容，否则截断
let max_lines = if detail_mode { usize::MAX } else { 20 };
```

**Reasoning 显示（第 484-485 行）- 正确实现：**
```rust
// detail_mode 显示完整 reasoning，否则只显示 tail_lines
if detail_mode {
    // 完整显示
}
```

## 影响

1. 用户体验不一致：同样是详细模式，ToolResult 显示完整，Shell 输出却被截断
2. 调试困难：长输出的 Bash 命令（如 `git log`、`cargo build`）在详细模式下仍看不到完整结果
3. 用户困惑：Ctrl+O 提示 "for details"，但详细模式下仍显示 "output truncated at 40 lines"

## 复现步骤

1. 执行一个产生超过 40 行输出的 Bash 命令（如 `git log --oneline -50`）
2. 默认模式显示 6 行 + "... X more lines hidden, Ctrl+O for details"
3. 按 Ctrl+O 切换到详细模式
4. 期望：显示完整 50 行输出
5. 实际：显示 40 行 + "... output truncated at 40 lines (10 more lines hidden)"

## 修复建议

修改 `peri-tui/src/ui/message_render.rs` 第 324-325 行：

```rust
// 修改前
let max_lines = if detail_mode {
    SHELL_OUTPUT_DETAIL_LINES  // 40
} else {
    SHELL_OUTPUT_COLLAPSED_LINES // 6
};

// 修改后
let max_lines = if detail_mode {
    usize::MAX  // 详细模式显示完整内容
} else {
    SHELL_OUTPUT_COLLAPSED_LINES // 6
};
```

同时可以删除不再使用的常量：
```rust
// const SHELL_OUTPUT_DETAIL_LINES: usize = 40;  // 删除此行
```

## 验证方式

1. 执行超过 40 行输出的命令（如 `git log --oneline -50`）
2. 默认模式：显示 6 行 + 截断提示
3. 按 Ctrl+O 切换详细模式：显示完整 50 行输出，无截断提示
4. 再次按 Ctrl+O 切回默认模式：显示 6 行 + 截断提示

## 相关文件

- `peri-tui/src/ui/message_render.rs` - 截断逻辑实现
- `peri-tui/src/ui/render_thread.rs` - detail_mode 状态管理
- `peri-tui/src/app/thread_ops.rs` - toggle_detail_mode() 方法
- `peri-tui/src/event/keyboard/shortcuts.rs` - Ctrl+O 快捷键绑定

## 关联 Issue

- `spec/issues/2026-06-01-tool-output-truncation-bypass.md` - 工具输出截断机制被绕过
