> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-mod-rs-cohesion.md

# Mod.rs 子模块内聚度问题：command/mod.rs + sync/mod.rs + panels/mod.rs + mcp/mod.rs

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`75d30d5`（command/ 重组为核心/面板/会话三组）

## 问题描述

4 个 `mod.rs` 文件子模块数量超过 10 个，表明模块职责边界不够清晰。`app/mod.rs`（48 子模块）已有独立 issue（2026-05-14）追踪，本 issue 覆盖其余 4 个。

## 现状数据

| 文件 | 子模块数 | 阈值 | 问题 |
|------|---------|------|------|
| `peri-tui/src/command/mod.rs` | 25 | 15 | 命令过多时可考虑按功能分组目录（core/panel/system） |
| `peri-tui/src/sync/mod.rs` | 13 | 15 | 接近阈值，sync 模块自身 789 行 |
| `peri-tui/src/ui/main_ui/panels/mod.rs` | 11 | 15 | 接近阈值，但 panels 目录结构已清晰 |
| `peri-middlewares/src/mcp/mod.rs` | 12 | 15 | 接近阈值，MCP 子模块边界合理 |

`command/mod.rs` 最严重——25 个命令子模块平铺，可分组为：
- `command/core/` — 基础命令（help, clear, exit, history, doctor）
- `command/panel/` — 面板命令（config, model, plugin, mcp, hooks, cron, agents, memory, login）
- `command/session/` — 会话命令（split, rename, compact, context_cmd, cost, lang, effort, loop_cmd, setup）

## 期望改进方向

优先处理 `command/mod.rs` 重组。其余 3 个暂未突破严重阈值，可等需求驱动时再调整。

## 涉及文件

- `peri-tui/src/command/mod.rs`（25 子模块）
- `peri-tui/src/sync/mod.rs`（13 子模块）
- `peri-tui/src/ui/main_ui/panels/mod.rs`（11 子模块）
- `peri-middlewares/src/mcp/mod.rs`（12 子模块）
