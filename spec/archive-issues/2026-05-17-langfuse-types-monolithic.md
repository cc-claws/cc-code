> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-langfuse-types-monolithic.md

# langfuse-client/src/types.rs 所有类型定义集中（1008 行）

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`4775c57`

## 问题描述

`langfuse-client/src/types.rs` 达到 1008 行（39KB），将整个 crate 的类型定义集中在一个文件中，包含了 trace、span、generation、event、score 等所有领域的结构体和序列化逻辑。

## 现状数据

| 指标 | 值 |
|------|-----|
| 行数 | 1008 |
| 大小 | 39KB |
| 内容 | 所有 API 请求/响应类型 + IngestionEvent 枚举 + 序列化逻辑 |

文件结构是纯类型定义（无复杂逻辑），因此严重程度较低。但集中的类型文件不利于新贡献者理解领域模型。

## 期望改进方向

按领域拆分：

```
langfuse-client/src/types/
├── mod.rs       ← 公共导出 + 类型别名
├── event.rs     ← IngestionEvent + 相关类型
├── trace.rs     ← Trace + TraceBody
├── span.rs      ← Span + SpanBody
├── generation.rs ← Generation + GenerationBody
├── score.rs     ← Score + ScoreBody
└── common.rs    ← 公共枚举（ObservationLevel 等）
```

此改动需同步更新 `src/lib.rs` 的 `pub use` 导出以及所有 `use langfuse_client::types::*` 引用。

## 涉及文件

- `langfuse-client/src/types.rs`（1008 行，39KB）
- `langfuse-client/src/lib.rs`（导出声明需更新）
