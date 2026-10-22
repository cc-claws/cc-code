# Langfuse 领域

## 领域综述

Langfuse 领域负责 Agent 执行的全链路可观测性，将每次 LLM 调用、工具调用、对话 Session 结构化上报到 Langfuse 监控平台。Langfuse 上报逻辑现位于 `cc-acp`（`cc-acp/src/langfuse/`，依赖在 cc-acp 的 Cargo.toml），cc-tui 仅在启动时构造 LangfuseConfig 并传入；不侵入 `cc-agent` 核心框架。

核心职责：
- Trace 管理：每次用户发送消息对应一个 Trace，多轮消息共享同一 Session
- Generation 追踪：LLM 调用记录（名称、模型、input/output、token 用量）
- Tool 观测：工具调用记录（ObservationType::Tool，嵌套在 "Tools" 批次 Span 下）
- Agent Observation：每次 Agent 执行创建一个 type=Agent、name="agent-run" 的根 Observation 包裹整个循环
- Session 生命周期：LangfuseSession 与 SQLite Thread 一一对应，切换/新建 Thread 时重置

## 核心流程

### 观测层级结构

```
Langfuse Session (session_id = thread_id)
  └── Trace (trace_id)
        └── Observation(type=Agent, name="agent-run")   ← 根 Observation 本身即 agent-run
              ├── Observation(type=Generation, name="llm-step-{n}")
              ├── Span(name="Tools"，工具批次)
              │     ├── Observation(type=Tool, name=tool_name)
              │     └── Observation(type=Tool, ...)
              └── Observation(type=Generation, name="llm-step-{n+1}")
```

### Session 生命周期管理

```
App 启动: 创建进程级共享 LangfuseSession（LangfuseSession::new(config)，无 session_id 参数）

submit_message():
  LangfuseTracer::new(session.clone(), thread_id)  ← 每轮新 trace_id，session_id 由 Tracer 逐轮传入

new_thread() / open_thread():
  session 不重建，仅下一轮 Tracer 携带新的 thread_id
```

### 事件→观测映射

| AgentEvent | Langfuse 对象 | 携带数据 |
|-----------|-------------|---------|
| submit_message() | Trace + Agent Observation 开始 | input、session_id |
| LlmCallStart | Generation 创建 | input messages |
| LlmCallEnd | Generation 更新 | model、output、usage |
| ToolStart | Tool Observation 创建 | name、input |
| ToolEnd | Tool Observation 更新 | output、is_error |
| Done | Trace + Agent Observation 结束 | final_answer |

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 客户端库 | langfuse-client（workspace 内 crate，Langfuse V4 客户端，替代 langfuse-ergonomic） |
| Batcher | 自定义 Batcher 实现，异步批量上报，BackpressurePolicy::DropNew 避免 OOM |
| 重试机制 | 指数退避重试，最大 3 次，网络错误自动恢复 |
| 线程安全 | `Arc<parking_lot::Mutex<LangfuseTracer>>`，FnEventHandler 闭包与主线程共享 |
| Session 级别 | LangfuseSession 持有 Arc<LangfuseClient> + Arc<Batcher>，跨多轮复用 |
| Observation 类型 | Agent/Tool/Generation via IngestionEvent 枚举，经 OTLP 端点 `/api/public/otel/v1/traces` 上报 |
| 命名约定 | Generation 名称：ChatOpenAI / ChatAnthropic（与 LangChain 约定一致） |
| 配置方式 | 环境变量 LANGFUSE_PUBLIC_KEY/SECRET_KEY/BASE_URL，未配置时静默跳过 |
| Hook 扩展 | AgentEvent 新增 LlmCallStart/LlmCallEnd 两个变体（向后兼容，调用方加 `_ => {}` 即可）|

## Feature 附录

### feature_20260324_F001_rust-langfuse-client
**摘要:** Langfuse 客户端早期探索（无设计文档）
**关键决策:** — （早期探索，无正式设计）
**归档:** [链接](../../archive/feature_20260324_F001_rust-langfuse-client/)
**归档日期:** 2026-03-27

### feature_20260324_F001_langfuse-tui-monitoring
**摘要:** TUI 层接入 Langfuse 全链路追踪
**关键决策:**
- 侵入最小化：cc-agent 仅新增 LlmCallStart/LlmCallEnd 两个 AgentEvent 变体
- Batcher 生命周期：进程级共享（随 LangfuseSession 创建，非每轮新建）
- 工具调用按 tool_call_id 以 HashMap 关联 ToolStart/End（非 FIFO）
- 依赖隔离：langfuse 相关依赖在 cc-acp（上报逻辑已迁至 cc-acp/src/langfuse/）
**归档:** [链接](../../archive/feature_20260324_F001_langfuse-tui-monitoring/)
**归档日期:** 2026-03-27

### feature_20260325_F001_tui-langfuse-session
**摘要:** Thread 级 LangfuseSession 使多轮消息归属同一 Session
**关键决策:**
- LangfuseSession 持有共享 client/batcher，生命周期提升到 Thread 级
- LangfuseTracer 每轮新建（独立 trace_id），共享 session
- 打开历史 Thread 时重置 session，下次发消息用历史 thread_id 创建新 Session
**归档:** [链接](../../archive/feature_20260325_F001_tui-langfuse-session/)
**归档日期:** 2026-03-27

### feature_20260325_F001_langfuse-nested-subagent-trace
**摘要:** Langfuse 嵌套子 Agent 追踪迭代探索（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F001_langfuse-nested-subagent-trace/)
**归档日期:** 2026-03-27

### feature_20260325_F001_langfuse-subagent-nesting
**摘要:** Langfuse 子 Agent 嵌套追踪迭代探索（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F001_langfuse-subagent-nesting/)
**归档日期:** 2026-03-27

### feature_20260325_F003_langfuse-observation-types
**摘要:** 规范化 Langfuse 观测层级与类型命名
**关键决策:**
- Agent Observation 包裹整个 ReAct 循环，所有 Generation/Tool 挂在其下
- 工具调用从 span-create 切换到 observation-create（ObservationType::Tool）
- Generation 名称改为 ChatOpenAI / ChatAnthropic（provider_name 从 LlmProvider::display_name 取）
- parent_observation_id 统一指向 agent_span_id
**归档:** [链接](../../archive/feature_20260325_F003_langfuse-observation-types/)
**归档日期:** 2026-03-27

### feature_20260325_F004_subagent-langfuse-nesting
**摘要:** 子 Agent Langfuse 嵌套追踪最终迭代（无设计文档）
**关键决策:** — （迭代探索版本）
**归档:** [链接](../../archive/feature_20260325_F004_subagent-langfuse-nesting/)
**归档日期:** 2026-03-27

### feature_20260330_F004_langfuse-client
**摘要:** workspace 内 langfuse-client crate 替代 langfuse-ergonomic
**关键决策:**
- 新建 workspace crate langfuse-client，实现 Langfuse V4 API 客户端
- 替代 langfuse-ergonomic 0.6.3 外部依赖，完全自主可控
- Batcher 异步批量上报 + 指数退避重试（最大 3 次）
- 保持与现有 LangfuseTracer/LangfuseSession 接口兼容
- TUI 层依赖从 langfuse-ergonomic 切换到 langfuse-client
**归档:** [链接](../../archive/feature_20260330_F004_langfuse-client/)
**归档日期:** 2026-04-27

## Issue 经验附录

### issue_2026-05-17-langfuse-types-monolithic

**摘要:** langfuse-client/src/types.rs 所有类型定义集中在一个文件（1008 行），按领域拆分为 event/trace/span/generation/score/common
**状态:** Fixed
**归档日期:** 2026-05-18
**涉及文件:** langfuse-client/src/types.rs, langfuse-client/src/lib.rs
**说明:** 纯代码组织优化，无领域认知提炼。

### issue_2026-05-23-langfuse-missing-system-prompt-after-compact

**摘要:** Compact 后 Langfuse 遥测丢失系统提示词（实际 LLM 调用也缺失）
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** Compact后系统提示词, System消息前缀, do_full_compact, re_inject
**问题本质:** do_full_compact() 用 `*state.messages_mut() = new_messages` 整体替换，丢弃了头部的 System 消息（含系统提示词、CLAUDE.md、skills 摘要）
**通用模式:** 整体替换消息数组前必须提取并保留 System 前缀（`take_while(|m| m.is_system())`），compact 只替换 User/Assistant/Tool 部分
**架构影响:** BaseModelReactLLM.system 从未设置，系统提示词完全通过 state.messages() 的 System 消息传递
**涉及文件:** cc-acp/src/langfuse/tracer.rs:206-292, cc-agent/src/agent/executor/llm_step.rs:22-27, cc-agent/src/agent/executor/mod.rs:240-241, cc-middlewares/src/compact_middleware.rs:228-248
**CLAUDE.md 链接:** false

### issue_2026-05-23-langfuse-agent-run-root-missing

**摘要:** Langfuse agent-run 根节点缺失（native ingestion 迁移后回归）
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** Langfuse OTLP, ObservationType, native ingestion, skip_serializing_if
**问题本质:** Native ingestion API 严格校验 ObservationType（只接受 GENERATION/SPAN/EVENT），Agent/Tool 被拒绝；同时 ObservationUpdate 的 null 字段清空已有数据
**通用模式:** 外部 API 端点迁移时必须验证所有自定义枚举值的兼容性；Option 字段必须添加 skip_serializing_if 防止序列化为 null 清空已有数据
**技术决策:** OTLP 端点（宽松校验）优于 native ingestion（严格校验），配合 x-langfuse-ingestion-version header 确保实时可见
**涉及文件:** langfuse-client/src/batcher.rs, langfuse-client/src/client.rs, langfuse-client/src/types/mod.rs, cc-acp/src/langfuse/tracer.rs
**CLAUDE.md 链接:** false

---

## 相关 Feature
- → [tui.md#feature_20260324_F001_langfuse-tui-monitoring](./tui.md#feature_20260324_F001_langfuse-tui-monitoring) — TUI 集成点在 app/agent.rs
- → [agent.md#feature_20260326_F009_relay-message-id-propagation](./agent.md#feature_20260326_F009_relay-message-id-propagation) — message_id 字段由 F006 引入，Langfuse 观测依赖此 ID
