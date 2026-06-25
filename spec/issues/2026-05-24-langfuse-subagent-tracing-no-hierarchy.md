# Langfuse 遥测中 SubAgent 缺少层级追踪，内部操作错误挂在主 Agent 下

**状态**：Open
**优先级**：中
**创建日期**：2026-05-24

## 问题描述

在 Langfuse 遥测中，SubAgent（普通/Fork/Background 三种类型均受影响）被记录为普通的工具调用（Tool observation），SubAgent 内部的 LLM 调用和工具调用直接挂到主 agent-run observation 下面，与主 Agent 自己的 LLM/工具调用混在一起。Langfuse UI 中无法区分哪些操作是主 Agent 做的、哪些是 SubAgent 做的，导致 SubAgent 的耗时和 Token 成本分析无法独立进行。

## 症状详情

### 当前 Langfuse Trace 结构（异常）

```
Trace
└── agent-run (主 Agent observation)
    ├── ChatAnthropic (LLM Generation)
    ├── Tools (tools batch span)
    │   ├── Agent (Tool observation) ← 只有工具记录，没有 SubAgent 层级
    │   └── Read (Tool observation)
    ├── ChatAnthropic (LLM Generation) ← SubAgent 内部的 LLM 调用，错误地挂到主 Agent 下
    ├── Grep (Tool observation) ← SubAgent 内部的工具调用，缺少 SubAgent 父节点
    └── ...
```

### 期望的 Langfuse Trace 结构

```
Trace
└── agent-run (主 Agent observation)
    ├── ChatAnthropic (LLM Generation)
    ├── Tools (tools batch span)
    │   ├── Agent (Tool observation) ← 保留工具记录
    │   │   └── subagent:code-reviewer (SubAgent Agent observation) ← 新增，作为 Tool 的子节点
    │   │       ├── ChatAnthropic (LLM Generation) ← SubAgent 内部 LLM 调用
    │   │       ├── Tools (subagent tools batch span)
    │   │       │   ├── Read (Tool)
    │   │       │   └── Grep (Tool)
    │   │       └── ...
    │   └── Read (Tool observation)
    └── ...
```

### 关键现象

| 维度 | 表现 |
|------|------|
| SubAgent 类型 | 普通（subagent_type）/ Fork / Background 三种均受影响 |
| SubAgent 可见性 | 作为普通 Tool observation 可见，但没有 SubAgent 层级包装 |
| SubAgent 内部 LLM 调用 | 可见，但 parent 指向主 agent-run，与主 Agent 混在一起 |
| SubAgent 内部工具调用 | 可见，但同样挂到主 Agent 的工具批次下 |
| 缺失内容 | SubAgent 层级的 Agent observation（type=Agent, name=subagent:xxx） |

## 复现条件

- **复现频率**：必现（只要启用了 Langfuse 遥测且有 SubAgent 调用）
- **触发步骤**：
  1. 启用 Langfuse 遥测
  2. 进行一次包含 SubAgent 调用的对话（如让 Agent 调用 code-reviewer 审查代码）
  3. 在 Langfuse UI 中查看对应 trace
- **环境**：所有使用 Langfuse 遥测 + SubAgent 功能的场景

## 涉及文件

- `peri-acp/src/langfuse/tracer.rs` —— `on_subagent_start`/`on_subagent_end` 方法已定义但未调用（死代码），缺少 Tool-SubAgent 父子关联逻辑
- `peri-acp/src/session/executor.rs` —— 事件泵中 `SubagentStarted`/`SubagentStopped` 事件被 `_ => {}` 丢弃，未转发到 tracer
- `peri-agent/src/agent/events.rs` —— `SubagentStarted`/`SubagentStopped` 事件缺少 `tool_call_id` 等关联字段，无法建立 Tool-SubAgent 父子关系
- `peri-middlewares/src/subagent/tool/define.rs` —— `SubAgentTool.invoke()` 内发射 `SubagentStarted`/`SubagentStopped` 事件，但不携带父 Tool 的关联信息
- `peri-middlewares/src/subagent/tool/mod.rs` —— `SourceAgentIdHandler` 仅对 ToolStart/ToolEnd/TextChunk 注入 `source_agent_id`，SubagentStarted/Stopped 直接透传
