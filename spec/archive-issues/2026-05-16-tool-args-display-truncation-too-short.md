> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-tool-args-display-truncation-too-short.md

# 工具调用参数显示截断过短

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-16
**关闭日期**：2026-05-16

## 问题描述

TUI 中工具调用的参数显示截断过短（文件系统工具 60 字符，Shell 60 字符，Widget 层再截到 40 字符），导致无法从摘要中快速识别操作的完整目标路径或命令内容。Shell 命令和文件系统工具路径应适当放宽截断阈值。

## 症状详情

当前显示效果（截断过短）：

```
⏺ Edit(peri-middlewares/src/tools/output_persi…)   ← 路径被截断
⏺ Shell(cd /Users/konghayao/code/ai/perihelion …)  ← 命令被截断
```

期望：文件系统工具完整显示路径，Shell 显示最多 400 字符的命令。

## 现状数据

`format_tool_args`（`peri-tui/src/app/tool_display.rs:42-62`）在各分支的截断阈值：

| 工具 | 参数 | 当前截断 | 期望 |
|------|------|---------|------|
| Read/Write/Edit | `file_path`→相对路径 | 60 字符 | 完整路径（不截断） |
| Glob | `pattern`→相对路径 | 60 字符 | 200 字符 |
| Grep | `pattern` | 60 字符 | 200 字符 |
| folder_operations | `operation` + `path` | 不截断 | 保持现状 |
| Bash | `command` | 60 字符 | 400 字符 |
| WebSearch/WebFetch | `query`/`url` | 60 字符 | 保持现状 |
| LSP | `operation` | 40 字符 | 保持现状 |
| ExecuteExtraTool | `tool_name` | 40 字符 | 保持现状 |
| SearchExtraTools | `query` | 40 字符 | 保持现状 |

`format_args_summary`（`peri-widgets/src/tool_call/display.rs:18`）Widget 层二次截断：3 处调用均硬编码 `max_width: 40`，需统一提高到 400。

## 期望改进方向

1. `format_tool_args` 中 Read/Write/Edit 不截断 `file_path`（`strip_cwd` 后直接返回）
2. `format_tool_args` 中 Glob/Grep 的 `pattern` 截断阈值 60 → 200
3. `format_tool_args` 中 Bash 的 `command` 截断阈值 60 → 400
4. `format_args_summary` 3 处调用 `max_width: 40` → `400`
5. 其他工具保持现状

## 涉及文件

- `peri-tui/src/app/tool_display.rs`（37 行）—— `format_tool_args` 各分支截断阈值
- `peri-tui/src/app/tool_display_test.rs`（38 行）—— 对应测试
- `peri-widgets/src/tool_call/mod.rs:96` —— `format_args_summary` 调用
- `peri-widgets/src/message_block/blocks.rs:87` —— `format_args_summary` 调用
- `peri-tui/src/ui/message_render.rs:379` —— `format_args_summary` 调用

## 解决方案（已实现）

修改 5 个文件，仅调整截断阈值，不改函数签名或调用链：

| 文件 | 变更 |
|------|------|
| `tool_display.rs` | `format_tool_args`：Bash 400, Glob/Grep 200, Read/Write/Edit 不截断；其他不变 |
| `tool_display_test.rs` | 追加 4 个测试：file_path 不截断、Bash 400、Glob 200、Grep 200 |
| `tool_call/mod.rs:96` | `format_args_summary(..., 40)` → `(..., 400)` |
| `message_block/blocks.rs:87` | 同上 |
| `message_render.rs:379` | 同上 |

**测试**：peri-widgets 89 passed；peri-tui 478 passed, 1 pre-existing failure（`test_model_panel_shows_effort`）。
