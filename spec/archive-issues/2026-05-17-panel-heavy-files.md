> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-panel-heavy-files.md

# Panel 文件过度肥大：mcp_panel.rs + login_panel.rs + setup_wizard.rs

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`49fdead`（setup_wizard）+ `1d56060`（mcp_panel）+ `51af6f6`（login_panel）

## 问题描述

`peri-tui/src/app/` 下有 3 个 panel 文件超过 700 行，将面板状态管理、App 扩展操作、UI 渲染逻辑混在同一文件中，与 CLAUDE.md 倡导的"面板组件逻辑独立"原则矛盾。

## 现状数据

| 文件 | 行数 | 大小 | 主要问题 |
|------|------|------|---------|
| `peri-tui/src/app/mcp_panel.rs` | 1083 | 40KB | MCP 面板数据操作 + UI 渲染混合，17 个 pub 声明 |
| `peri-tui/src/app/setup_wizard.rs` | 1016 | 34KB | Setup 向导步骤状态机 + UI 渲染混合 |
| `peri-tui/src/app/login_panel.rs` | 763 | 30KB | 登录表单状态机 + App 操作混合 |

所有三个文件均 PanelComponent trait 实现与 App 扩展操作（`*_panel_*` 方法）混合在同一文件中。

## 期望改进方向

按三层拆分模式统一处理：

- `mcp_panel.rs` → `mcp_panel/state.rs` + `mcp_panel/ops.rs` + `mcp_panel/ui.rs`
- `setup_wizard.rs` → `setup_wizard/state.rs` + `setup_wizard/ui.rs`
- `login_panel.rs` → `login_panel/state.rs` + `login_panel/ui.rs`

拆分后将 `pub(crate)` 类型降为模块内部可见，减少 API 面。

## 涉及文件

- `peri-tui/src/app/mcp_panel.rs`（1083 行，40KB）
- `peri-tui/src/app/setup_wizard.rs`（1016 行，34KB）
- `peri-tui/src/app/login_panel.rs`（763 行，30KB）
