# Langfuse 可观测性覆盖度报告

> 生成日期：2026-05-25 | 代码库：perihelion

---

## 1. 架构概述

perihelion 的 Langfuse 集成采用**事件驱动**架构，核心层无 Langfuse 依赖。

```
peri-agent (emit AgentEvent)  ──mpsc──→  peri-acp/executor  ──→  LangfuseTracer  ──→  Batcher  ──→  OTLP HTTP
         (zero langfuse deps)                 (match 5 event types)          (per-turn)         (max_events=50, flush=10s)
```

| 组件 | 文件 | 职责 |
|------|------|------|
| `langfuse-client` | `langfuse-client/` | 自研 Langfuse V4 客户端：HTTP + Basic Auth、OTLP 序列化、批量缓冲、退避重试 |
| `LangfuseTracer` | `peri-acp/src/langfuse/tracer.rs` | 每轮 Agent 执行新建，事件→Langfuse Observation 映射 |
| `LangfuseSession` | `peri-acp/src/langfuse/session.rs` | 进程级共享状态：Client + Batcher（Arc 跨线程复用） |
| `executor.rs` | `peri-acp/src/session/executor.rs:186-227` | 事件泵：match 5 种 ExecutorEvent → tracer 方法 |

---

## 2. 记录的关键信息

### 2.1 Observation 层级结构

```
Trace (trace_id, 隐式由 OTLP 创建)
  │  session_id: 会话 thread_id
  │
  └─ Observation(type=AGENT, name="agent-run")
     │  input: 用户 prompt
     │  output: 累积的最终回答
     │  version: CARGO_PKG_VERSION
     │
     ├─ Observation(type=GENERATION, name="ChatOpenAI"|"ChatAnthropic")
     │  │  parent: agent-run
     │  │  input:  {messages: [...], tools: [...]}  ← 含 System prompt（修复后）
     │  │  output: LLM 原始响应文本
     │  │  model:  "gpt-4o" / "claude-sonnet-4-20250514" 等
     │  │  usage_details: {input, output, total} + {cache_creation, cache_read}
     │  │  cost_details: null  ← 始终为空
     │  │
     │  ├─ Span(type=SPAN, name="Tools")
     │  │  │  parent: agent-run
     │  │  │
     │  │  ├─ Observation(type=TOOL, name="Read")
     │  │  │  │  input:  {"file_path": "..."}
     │  │  │  │  output: 工具执行结果
     │  │  │  │  level: ERROR（工具失败时）
     │  │  │
     │  │  └─ Observation(type=TOOL, name="Glob")
     │  │     │  parent: Tools batch span
     │  │
     │  └─ ... (更多轮次的 Generation + Tools)
     │
     └─ (子Agent 分支)
        └─ Observation(type=AGENT, name="subagent:code-reviewer")
           │  parent: 无显式 parent_observation_id（依赖 batcher 顺序维持层级）
           │  input:  task prompt 前 200 字符
           │  output: 子 Agent 结果
           │
           ├─ Observation(type=GENERATION, ...)
           └─ Span(type=SPAN, name="Tools") ...
```

### 2.2 已监控事件（5/16）

| AgentEvent 变体 | 触发时机 | Langfuse 操作 |
|-----------------|----------|--------------|
| `LlmCallStart` | `do_llm_step()` 调用 LLM 前 | 缓存 messages + tools，flush 上一批 Tools span |
| `LlmCallEnd` | LLM 返回结果后 | 发送 `GenerationCreate`（含 model、usage_details） |
| `ToolStart` | 工具调用开始 | 创建 `PendingTool` 缓冲，必要时新建 Tools batch span |
| `ToolEnd` | 工具执行完毕 | 发送 `ObservationCreate(type=TOOL)`，更新 batch span 结束时间 |
| `TextChunk` | 流式文本到达 | 累积到 `final_answer`，用于 `on_trace_end()` 的 output |

### 2.3 记录的数据字段

| 字段 | 来源 | 状态 |
|------|------|------|
| `trace_id` | UUID v7 | ✅ |
| `session_id` | 会话 thread_id | ✅ |
| `version` | `CARGO_PKG_VERSION` | ✅ |
| `model`（Generation） | LLM 适配器的 `model()` | ✅ |
| `usage_details` | LLM 响应中的 `TokenUsage` | ✅ 含 cache_creation/cache_read |
| `input`（Generation） | `state.messages()` 快照 | ✅ |
| `output`（Generation） | LLM 返回的完整响应文本 | ✅ |
| `input`（Tool） | 工具调用参数 JSON | ✅ |
| `output`（Tool） | 工具执行结果字符串 | ✅ |
| `input`（Agent） | 用户输入文本 | ✅ |
| `output`（Agent） | 累积 `TextChunk` 或错误消息 | ✅ |
| `cost_details` | — | ❌ 始终为 None |
| `model_parameters` | — | ❌ 始终为 None |
| `tags`（Trace） | — | ❌ 始终为 None |
| `metadata` | — | ❌ 始终为 None |
| `environment` | — | ❌ 始终为 None |
| `user_id` | — | ❌ 始终为 None |
| `release` | — | ❌ 始终为 None |

---

## 3. 智能体覆盖度评估

### 3.1 已覆盖的核心路径

| 维度 | 覆盖状态 | 说明 |
|------|----------|------|
| LLM 调用（每次 step） | ✅ **完整** | 输入（含 System prompt + 消息历史 + tools）、输出、模型、token 用量 |
| 工具调用（每次） | ✅ **完整** | 按批次分组（Tools batch span），包含输入/输出/错误标记 |
| 子 Agent 调用 | ⚠️ **部分** | 记录了子 Agent 的 Observation，但 `parent_observation_id` 缺失 |
| 流式文本输出 | ✅ **完整** | 累积为 agent-run 的 output |
| Agent 级 input/output | ✅ **完整** | agent-run 包围整个执行轮次 |

### 3.2 未覆盖的 Agent 生命周期阶段

| 事件 | 影响 | 优先级 |
|------|------|--------|
| **Compact 压缩流程** (`CompactStarted/Completed/Error`) | 无法追踪压缩触发时机、摘要质量、压缩后消息数量 | 🔴 高 |
| **LLM 重试** (`LlmRetrying`) | 无法区分首次失败和重试成功，影响稳定性分析 | 🔴 高 |
| **上下文窗口警告** (`ContextWarning`) | 无法关联 compact 触发与窗口压力事件 | 🟡 中 |
| **后台 Agent 任务** (`BackgroundTaskCompleted`) | 后台子 Agent 执行完全不可见 | 🟡 中 |
| **Todo 状态变化** (`TodoUpdate`) | 缺少任务规划执行的轨迹 | 🟢 低 |
| **LSP 诊断** (`LspDiagnostics`) | 缺少代码质量反馈 | 🟢 低 |
| **StepDone** | 缺少轮次边界的可观测性 | 🟢 低 |
| **StateSnapshot / MessageAdded** | 内部状态快照，对 Langfuse 无观测价值 | — |

### 3.3 缺失的观测能力

| 能力 | 说明 |
|------|------|
| **成本追踪** | `cost_details` 始终为 None，无法追踪实际金钱成本 |
| **评分/评估** | 无 Score 事件，无人工反馈或自动 LLM-as-Judge 评分 |
| **模型参数** | `model_parameters`（temperature/top_p 等）未记录 |
| **环境标识** | `environment`/`user_id`/`release` 未发送，跨环境辨识困难 |
| **自定义标签** | 无 tags 机制，无法按功能维度过滤 |
| **Prompt 管理** | 未使用 Langfuse Prompt Management（prompt_name/prompt_version） |
| **Evaluation 管线** | 无自动化评估数据集或评测执行轨迹 |

---

## 4. 关键发现

### 4.1 强项

1. **解耦设计优秀**：`peri-agent` 完全不感知 Langfuse，通过通用 `AgentEvent` 事件流松耦合集成
2. **工具批次追踪**：Tools batch span 将并发工具调用合理分组，Langfuse UI 层级清晰
3. **SubAgent 栈管理**：支持嵌套子 Agent 的正确层级入队顺序
4. **OTLP 协议**：使用 OTLP 端点而非原生 API，支持自定义 ObservationType

### 4.2 问题

1. **SubAgent 无显式 parent**：`end_subagent()` 创建 Observation 时 `parent_observation_id: None`，在 Langfuse UI 中子 Agent 与主 Agent 平级显示
2. **Compact 完全盲区**：Compact 是 Agent 性能的关键操作，但其生命周期事件被 `_ => {}` 丢弃
3. **成本信息缺失**：所有 `cost_details` 为 None，无法进行成本分析
4. **重试不可见**：LLM 重试期间的发���在 Langfuse 中无记录

---

## 5. 建议改进优先级

| 优先级 | 改进项 | 工作量 |
|--------|--------|--------|
| P0 | **追踪 Compact 事件**：将 Compact 作为 agent-run 的子 Observation 或 Event 记录 | 小 |
| P0 | **追踪 LlmRetrying**：发送 EventCreate 或设置 Generation 的 metadata | 小 |
| P1 | **填充 cost_details**：根据 model 和 usage 计算成本（Anthropic/OpenAI 定价表） | 中 |
| P1 | **SubAgent parent 关联**：设置 `parent_observation_id` 指向主 agent-run | 小 |
| P2 | **追踪 ContextWarning**：关联 Compact 触发决策 | 小 |
| P2 | **填充 metadata/tags**：添加模型别名、权限模式等上下文 | 小 |
| P3 | **追踪 BackgroundTask**：为后台任务创建独立 trace | 中 |
| P3 | **评分机制**：接入人工反馈或 LLM-as-Judge 评估 | 大 |
