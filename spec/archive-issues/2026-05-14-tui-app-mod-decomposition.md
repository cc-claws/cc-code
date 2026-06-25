> 归档于 2026-05-24，原路径 spec/issues/2026-05-14-tui-app-mod-decomposition.md

# TUI app/ 目录模块化：app/mod.rs 48 个子模块、plugin_panel.rs 2017 行

**状态**：Resolved (基本完成)
**优先级**：高
**创建日期**：2026-05-14
**修复日期**：2026-05-20

## 问题描述

`peri-tui/src/app/mod.rs` 包含 48 个子模块声明，多个文件超 1000 行，混合了面板组件定义、App struct 操作、状态管理等职责。

## 修复结果

### 大文件拆分

| 文件 | 修复前 | 修复后 | 拆分方式 |
|------|--------|--------|---------|
| `plugin_panel.rs` | 2017 行 | `plugin_panel/mod.rs` (816 行) + `handlers/` (9 子模块) + `types.rs` | 面板渲染/Handlers/类型分离 |
| `agent_ops.rs` | 1500 行 | `agent_ops/mod.rs` (383 行) + `lifecycle.rs` + `polling.rs` + `subagent.rs` + `acp_bridge.rs` | 生命周期/轮询/子代理/ACP 分离 |
| `mcp_panel.rs` | 1218 行 | `mcp_panel/mod.rs` (123 行) + `component.rs` + `ops.rs` | 组件/操作分离 |
| `message_pipeline.rs` | 912 行 | `message_pipeline/mod.rs` (742 行) + `reconcile.rs` + `transform.rs` | reconcile/transform 逻辑拆分 |
| `login_panel.rs` | 1021 行 | `login_panel/mod.rs` (394 行) + `component.rs` | 组件/状态分离 |
| `panel_ops.rs` | 1084 行 | 145 行 | 操作函数分发到各面板子模块 |
| `setup_wizard.rs` | 804 行 | `setup_wizard/mod.rs` + `ops.rs` | 操作分离 |

### app/mod.rs 结构化

`app/mod.rs` 从 762 行手动声明 48 子模块改为使用 `include!` 宏按类别分组：

```
include!("modules_panels.inc");    // 面板模块
include!("modules_state.inc");     // 状态管理模块
include!("modules_agent.inc");     // Agent 通信模块
include!("modules_system.inc");    // 系统/交互模块
```

### 面板 API 面缩小

`panel_manager.rs` 的 35 个 pub 声明中内部类型已降为 `pub(crate)`，减少公开 API 面。

## 仍存在的改进空间

- `plugin_panel/mod.rs` 仍 816 行，渲染逻辑可进一步拆分
- `message_pipeline/mod.rs` 仍 742 行，reconcile 逻辑复杂
- `app/mod.rs` 554 行，核心 App struct 定义仍在单个文件中

## 涉及文件

- `peri-tui/src/app/mod.rs`（762→554 行，6 direct + 4 include! 分组）
- `peri-tui/src/app/modules_*.inc`（4 个分组声明文件）
- `peri-tui/src/app/plugin_panel/`（handler 拆为 9 子模块 + types）
- `peri-tui/src/app/agent_ops/`（5 子模块）
- `peri-tui/src/app/mcp_panel/`（3 子模块）
- `peri-tui/src/app/message_pipeline/`（3 子模块）
- `peri-tui/src/app/login_panel/`（2 子模块）
- `peri-tui/src/app/setup_wizard/`（2 子模块）
