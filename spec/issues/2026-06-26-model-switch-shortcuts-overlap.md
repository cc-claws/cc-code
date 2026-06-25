# 模型切换快捷键冗余 + Ctrl+T 空槽位 bug

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-26
**修复 commit**：`53c428a`（分支 `feature#wismyzhizi2018#6月份#T00001_模型切换快捷键收敛`）

## 问题描述

TUI 模型切换相关有 4 套独立快捷键，功能高度重叠且存在边界 bug：

| 快捷键 | 行为 | 问题 |
|--------|------|------|
| `Ctrl+P` | 打开 CommandPalette 面板（Provider → Model → Effort 三阶段） | 功能完整但 status_bar 未提示 |
| `Ctrl+T` / `Alt+M` | 在硬编码 `["opus","sonnet","haiku"]` 间盲循环 | **未过滤空槽位**，provider 只配 1-2 个 alias 时会切到空模型名 |
| `Ctrl+Shift+T` / `Alt+Shift+M` | cycle provider | 与 `Ctrl+P` 第一步完全重叠 |

且 `status_bar.rs:542` 默认 hints 只显示 `Ctrl+T`，`Ctrl+P` / `Ctrl+Shift+T` 都未提示，用户难以发现命令面板入口。

## 当前行为

```text
用户配置: providers[0].models = { opus: "gpt-4o", sonnet: "", haiku: "" }
按 Ctrl+T:
  → active_alias "opus" → "sonnet" (空字符串)
  → LlmProvider::from_config 返回 model_name = ""
  → status_bar 显示空模型名，下一轮 LLM 请求 model 字段为空
```

```text
status_bar hints (默认):
  "/" 命令 │ "Shift+Enter" 换行 │ "Ctrl+T" 切换模型 │ "Ctrl+U/D" 滚动
  → 用户看不到 Ctrl+P 入口
```

## 期望行为

- `Ctrl+Shift+T` / `Alt+Shift+M` 路径整体移除——Provider 切换统一走 `Ctrl+P` 命令面板
- `Ctrl+T` 改为从当前激活 provider 的 `ProviderModels` 动态收集**非空** alias，仅在非空槽位间循环；若可用 alias ≤1 则静默忽略
- `status_bar` 默认 hints 增加 `Ctrl+P` 命令面板提示
- locale 文案同步：删除 `Ctrl+Shift+T 切换 Provider`，补 `Ctrl+P 命令面板`

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. `~/.cc-code/settings.json` 中某 provider 只配置 opus 槽位（sonnet/haiku 留空字符串）
  2. 启动 `cargo run -p peri-tui`
  3. 按 `Ctrl+T` → 观察到 status_bar 模型名变空

## 关联 commit

`53c428a refactor(tui-keyboard): 收敛模型切换快捷键到 Ctrl+P/Ctrl+T`

修改 7 个文件：
- `peri-tui/src/event/keyboard.rs`
- `peri-tui/src/event/keyboard/shortcuts.rs`
- `peri-tui/src/app/global_ui_state.rs`
- `peri-tui/src/ui/main_ui/status_bar.rs`
- `peri-tui/locales/zh-CN/main.ftl`
- `peri-tui/locales/en/main.ftl`
- `CLAUDE.md`
