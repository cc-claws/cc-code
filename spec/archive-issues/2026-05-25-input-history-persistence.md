> 归档于 2026-05-25，原路径 spec/issues/2026-05-25-input-history-persistence.md

# 输入历史跨会话持久化

**状态**：已完成
**优先级**：低
**创建日期**：2026-05-25
**修复日期**：2026-05-25

## 问题描述

当前上下方向键切换历史输入已实现（`history_ops.rs`，上限 200 条），但历史仅存储在内存中，session 结束或 TUI 重启后丢失。用户希望 Enter 提交过的所有输入（无论对错）都能跨会话持久化保存，重新打开 TUI 后仍可通过上下键回溯。

## 当前行为

- 上下方向键浏览历史输入**已实现**：光标在 textarea 首行/末行时触发 `history_up()`/`history_down()`
- 历史存储在 `SessionUiState.input_history: Vec<String>`（内存态）
- 上限 200 条，去重（与最近一条相同则不记录）
- 按 Enter 即记录，不区分正确/错误
- session 结束、`/clear`、TUI 重启后历史丢失

## 期望行为

1. **持久化**：Enter 提交的输入自动保存到磁盘，TUI 重启后仍可上下键回溯
2. **全局共享**：所有项目共用同一份输入历史
3. **所有输入都保留**：不区分正确/错误，Enter 了就保存

## 症状详情

| 维度 | 当前 | 期望 |
|------|------|------|
| 存储位置 | 内存（`Vec<String>`） | 磁盘持久化（`~/.peri/input-history.json`） |
| 生命周期 | 随 session 销毁 | 跨 TUI 重启保留 |
| 隔离 | 按 session 实例 | 全局共享（用户偏好） |
| 上限 | 200 条 | 1000 条 |

## 涉及文件

- `peri-tui/src/app/history_persistence.rs`（新）—— 持久化模块
- `peri-tui/src/app/history_ops.rs`—— 浏览 + 写入持久化
- `peri-tui/src/app/ui_state.rs`—— 初始化时加载持久化数据
- `peri-tui/src/app/chat_session.rs`—— 传递 cwd（保留参数供未来扩展）
- `peri-tui/src/app/panel_ops.rs`—— 测试代码同步

## 实现方案

**设计决策**：JSON 文件存储，路径 `~/.peri/input-history.json`。全局共享——输入历史是用户个人偏好，非项目设置。

**路径变更**（commit `28aaa64`）：从 `{cwd}/.peri/history.json` 迁至 `~/.peri/input-history.json`，移除 cwd 参数依赖。

**实现**：

| # | 文件 | 改动 |
|---|------|------|
| 1 | `history_persistence.rs`（新） | `load_input_history()` / `save_input_history(history)`，原子写入 |
| 2 | `ui_state.rs` + `chat_session.rs` | `UiState::new()` 构造时加载全局历史 |
| 3 | `history_ops.rs` | `push_input_history()` 后保存 |
| 4 | `history_ops.rs` | 上限 200 → 1000 |

**行为**：
- Session 启动时加载 `~/.peri/input-history.json`
- 每次 Enter 提交后保存完整历史列表
- 文件不存在或 JSON 损坏 → 静默回退到空列表
- 全局共享，所有项目用同一份历史

## Commits

| SHA | 描述 |
|-----|------|
| `053eb9f` | feat(tui): persist input history to disk, keyed by cwd |
| `28aaa64` | refactor(tui): move input history from {cwd}/.peri/ to ~/.peri/ |
