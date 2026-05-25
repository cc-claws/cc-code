# 输入历史跨会话持久化（按项目隔离）

**状态**：已完成（JSON 持久化）
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
2. **按项目隔离**：不同项目目录（cwd）有独立的输入历史，互不干扰
3. **所有输入都保留**：不区分正确/错误，Enter 了就保存

## 症状详情

| 维度 | 当前 | 期望 |
|------|------|------|
| 存储位置 | 内存（`Vec<String>`） | 磁盘持久化（SQLite/JSON 文件） |
| 生命周期 | 随 session 销毁 | 跨 TUI 重启保留 |
| 隔离 | 按 session 实例 | 按项目目录（cwd） |
| 上限 | 200 条 | 可考虑加大（如 1000 条） |

## 涉及文件

- `peri-tui/src/app/history_ops.rs`（历史浏览逻辑）—— 加载持久化历史
- `peri-tui/src/app/agent_submit.rs`（提交时调用 `push_input_history`）—— 写入持久化存储
- `peri-tui/src/app/ui_state.rs`（`input_history` / `history_index` 字段）—— 初始化时加载持久化数据
- `peri-tui/src/acp_server/requests.rs`（session/new 时加载历史）—— 按 cwd 加载
- `peri-tui/src/acp_server/mod.rs`（SessionState）—— 存储路径管理

## 实现方案（已修复）

**设计决策**：JSON 文件存储，路径 `{cwd}/.peri/history.json`。无需 SQLite——数据只是 `Vec<String>`。

**4 个任务**（commit `053eb9f`）：

| # | 文件 | 改动 |
|---|------|------|
| 1 | `history_persistence.rs`（新） | `load_input_history(cwd)` / `save_input_history(cwd, history)`，原子写入 |
| 2 | `ui_state.rs` + `chat_session.rs` | `UiState::new()` 接受 cwd，构造时加载磁盘历史 |
| 3 | `history_ops.rs` | `push_input_history()` 后调用 `save_input_history()` |
| 4 | `history_ops.rs` | 上限 `truncate(200)` → `truncate(1000)` |

**行为**：
- Session 启动时加载 `{cwd}/.peri/history.json`
- 每次 Enter 提交后保存完整历史列表（不去重、不区分正确/错误）
- 文件不存在或 JSON 损坏 → 静默回退到空列表
- 不同项目目录有独立的 `.peri/history.json`
