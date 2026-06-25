> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-middleware-heavy-files.md

# Middleware 层大文件：subagent/tool.rs + plugin/installer.rs + plugin/marketplace.rs

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`c7d9082`（subagent/tool）+ `49092a0`（plugin/installer）+ `8376aa1`（plugin/marketplace）

## 问题描述

`peri-middlewares` crate 下 3 个文件超过 700 行，混合了工具定义、序列化、API 调用等多种职责。

## 现状数据

| 文件 | 行数 | 大小 | 主要问题 |
|------|------|------|---------|
| `peri-middlewares/src/subagent/tool.rs` | 1091 | 44KB | 8 个 pub 函数，工具定义 + 参数解析 + 执行逻辑混合 |
| `peri-middlewares/src/plugin/installer.rs` | 756 | 26KB | 下载 + 解压 + 验证混合 |
| `peri-middlewares/src/plugin/marketplace.rs` | 728 | 25KB | API 请求 + 响应解析 + 缓存逻辑混合 |

## 期望改进方向

- `subagent/tool.rs` → `subagent/tool/define.rs`（Agent 工具定义 + 参数 schema） + `subagent/tool/invoke.rs`（invoke 执行逻辑） + `subagent/tool/schema.rs`（JSON Schema 构建）
- `plugin/installer.rs` → `plugin/installer/download.rs`（下载逻辑） + `plugin/installer/extract.rs`（解压 + 验证）
- `plugin/marketplace.rs` → `plugin/marketplace/api.rs`（HTTP 请求） + `plugin/marketplace/types.rs`（响应类型定义）

## 涉及文件

- `peri-middlewares/src/subagent/tool.rs`（1091 行，44KB）
- `peri-middlewares/src/plugin/installer.rs`（756 行，26KB）
- `peri-middlewares/src/plugin/marketplace.rs`（728 行，25KB）
