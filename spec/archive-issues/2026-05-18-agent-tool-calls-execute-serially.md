> 归档于 2026-05-18，原路径 spec/issues/2026-05-18-agent-tool-calls-execute-serially.md

# 多 Agent 工具调用串行执行而非并发

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-18
**类型**：Bug

## 问题描述

当 LLM 在同一轮中发起多个 `Agent` 工具调用时，预期它们应并发执行（SubAgent 之间互不依赖），但实际表现是串行执行——后一个 Agent 必须等前一个完全结束才开始。

## 症状详情

| 现象 | 详情 |
|------|------|
| 触发条件 | LLM 在同一轮发出 ≥2 个 `Agent` 工具调用 |
| 实际行为 | Agent 工具一个接一个执行，第二个在第一个完成后才开始 |
| 期望行为 | 多个 Agent 工具应并发启动和运行 |

示例场景：父 Agent 同时调用 `code-reviewer` 和 `explore` 两个 SubAgent，观察到的执行顺序是 code-reviewer 完全跑完后 explore 才开始。

## 复现条件

- **复现频率**：必现（只要有多个 Agent 工具调用就串行）
- **触发步骤**：
  1. 父 Agent 在同一轮发出多个 Agent 工具调用
  2. 观察各 SubAgent 的启动和完成时间
- **环境**：任何模型/配置

## 涉及文件

- `peri-agent/src/agent/executor/tool_dispatch.rs`（L204-268）—— 工具执行调度逻辑，对 Agent 工具硬编码了串行 for 循环，未根据 `child_handler_factory` 是否存在判断是否可并发
- `peri-middlewares/src/subagent/tool/define.rs`（L173-181）—— `with_child_handler_factory` 已提供每子 Agent 独立 event handler 的能力，意图就是消除锁竞争以支持并发，但调度层未感知此配置

## 解决方案

**提交**：`6de639b` — fix: restore concurrent Agent tool execution with per-child event handlers

**根本原因**：提交 `c00335f` 为防止并发 SubAgent 死锁引入了串行执行。在三个死锁根因被独立修复后（LLM 流式取消支持 `tokio::select!`、4096 事件通道缓冲、`source_agent_id` 精确路由），串行限制不再必要。

**具体变更**：

1. **tool_dispatch.rs** — 移除 Agent 工具专门的串行执行路径，阶段二统一使用 `futures::future::join_all` 并发执行所有 ready_calls
2. **TUI agent.rs** — 恢复 `child_handler_factory`，从 `child_event_tx` 构建工厂函数，传递给 `subagent.with_child_handler_factory()`
3. **SubAgentTool::invoke** — 优先通过 `child_handler_factory(agent_id)` 创建 per-child event handler，避免共享 Langfuse Mutex 的锁竞争
