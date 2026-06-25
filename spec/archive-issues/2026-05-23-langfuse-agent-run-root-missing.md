> 归档于 2026-05-24，原路径 spec/issues/2026-05-23-langfuse-agent-run-root-missing.md

# Langfuse agent-run 根节点缺失（native ingestion 迁移后回归）

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-23
**修复日期**：2026-05-23

## 问题描述

5/22 langfuse 改动（切换到 native ingestion API + session_id 迁移到 per-turn 级别）后，Langfuse UI 中每轮对话的 `agent-run` 根节点（type=Agent observation）必现缺失。子节点（LLM Generation、Tool 调用等）正常存在并挂载到 trace 下，但缺少最顶层的 agent-run 聚合节点，导致无法��到单轮对话的整体汇总视图。

## 症状详情

| 维度 | 表现 |
|------|------|
| 缺失内容 | agent-run Observation（type=Agent，根节点，parent_observation_id=None） |
| 子节点状态 | LLM Generation、Tool Observation 等子节点正常存在 |
| 复现频率 | 必现，每轮对话都缺失 |
| 回归时间 | 5/22 commit `ecd4488`（native ingestion + tool session_id + version）之后 |

## 根因分析

三个问题叠加导致：

### 1. Native ingestion API 不支持 Agent/Tool observation type

commit `ecd4488` 把 batcher 从 OTLP 端点 (`/api/public/otel/v1/traces`) 切换到 native ingestion 端点 (`/api/public/ingestion`)。OTLP 端点把 `ObservationType` 当字符串属性不校验，所以 `Agent`/`Tool` 能过。Native ingestion 端点做严格校验，只接受 `GENERATION`/`SPAN`/`EVENT`，所有 `ObservationCreate(type=Agent)` 和 `ObservationCreate(type=Tool)` 都��� 400 拒绝。

服务端返回的错误：`Invalid option: expected one of "GENERATION"|"SPAN"|"EVENT"`

### 2. ObservationUpdate 中 null 字段清空已有数据

`ObservationBody` 等 struct 的 `Option` 字段缺少 `#[serde(skip_serializing_if = "Option::is_none")]`，导致 `ObservationUpdate` 中 `..Default::default()` 的 `None` 字段被序列化为 `"startTime": null` 等。Native ingestion 把 `null` 解释为"清除该字段"，会清掉 `ObservationCreate` 时设的 `start_time`、`input`、`session_id`。

### 3. TraceCreate 冗余事件（切回 OTLP 后的残留）

为 native ingestion 添加的 `TraceCreate` 事件在 OTLP 路径下是多余的——OTLP 通过 span 的 `trace_id` 隐式创建 trace，不需要显式 `TraceCreate`。多余的 `TraceCreate` 会被 OTLP 转换为额外的 span，造成 Langfuse UI 中出现重复节点。

## 修复方案

1. **batcher 切回 OTLP 端点**：恢复 `client.ingest()`（`/api/public/otel/v1/traces`），保留 `x-langfuse-ingestion-version: 4` header 确保 v2 读取 API 实时可见。OTLP 端点不过校验 observation type，`Agent`/`Tool` 正常通过。
2. **添加 `skip_serializing_if`**：所有 Body struct（`TraceBody`/`ObservationBody`/`SpanBody`/`GenerationBody`/`EventBody`/`ScoreBody`）的 `Option` 字段添加 `#[serde(skip_serializing_if = "Option::is_none")]`，防止 Update 事件清空已有字段。
3. **移除冗余 TraceCreate**：`on_trace_start()` 中删除 `TraceCreate` 事件，OTLP 通过 `trace_id` 隐式创建 trace。
4. **ObservationType 恢复原始值**：agent-run 和 subagent 用 `ObservationType::Agent`，工具调用用 `ObservationType::Tool`。

## 涉及文件

- `langfuse-client/src/batcher.rs` — `do_flush` 切回 `client.ingest()`（OTLP）
- `langfuse-client/src/batcher_test.rs` — mock 路径同步改为 OTLP 端点
- `langfuse-client/src/client.rs` — `ingest_native` 添加 `x-langfuse-ingestion-version: 4` header + 响应体错误日志
- `langfuse-client/src/types/mod.rs` — 所有 Body struct 添加 `skip_serializing_if`
- `peri-acp/src/langfuse/tracer.rs` — 移除 `TraceCreate`，恢复 `ObservationType::Agent/Tool`，添加诊断日志

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启用 Langfuse 遥测
  2. 执行任意对话
  3. 在 Langfuse UI 查看对应 trace
- **环境**：5/22 langfuse 改动后的版本
