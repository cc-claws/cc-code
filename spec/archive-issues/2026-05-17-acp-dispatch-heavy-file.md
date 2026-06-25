> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-acp-dispatch-heavy-file.md

# ACP dispatch.rs 请求分发逻辑过度集中（1044 行）

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`74ae7b9`

## 问题描述

`peri-tui/src/acp/dispatch.rs` 达到 1044 行（40KB），14 个 pub 函数。该文件负责 ACP 协议的所有请求分发逻辑——initialize、session/new、session/cancel、session/load、prompt、RequestPermission 等全部 handler 都集中在一个文件中。

## 现状数据

| 指标 | 值 |
|------|-----|
| 行数 | 1044 |
| 大小 | 40KB |
| pub 函数 | 14 |
| 主要职责 | ACP 请求路由 + 所有 handler 实现 + 响应序列化 |

各 handler 之间逻辑独立，但共享一些公共工具函数（如事件映射、session 查询）。

## 期望改进方向

按 handler 分组拆分为子目录：

```
acp/dispatch/
├── mod.rs          ← 路由入口 + 公共工具
├── initialize.rs   ← initialize handler
├── session.rs      ← session/new, session/load, session/cancel handlers
├── prompt.rs       ← prompt handler（核心，~300 行）
├── permission.rs   ← RequestPermission handler
└── helpers.rs      ← 公共序列化/反序列化工具
```

## 涉及文件

- `peri-tui/src/acp/dispatch.rs`（1044 行，40KB）
