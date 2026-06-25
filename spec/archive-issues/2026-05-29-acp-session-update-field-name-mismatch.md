> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-acp-session-update-field-name-mismatch.md
# ACP 大重构后所有流式事件静默丢失，TUI 只能通过 StateSnapshot 获取最终结果

**状态**：Fixed（749fa34）
**优先级**：高
**创建日期**：2026-05-29

## 问题描述

ACP 协议大重构（bb388ca）将 Category ① 事件（TextChunk、AiReasoning、ToolStart、ToolEnd、TodoUpdate）从 `peri/agent_event` 路由改为 `session/update` 路由。但 `handle_session_update_peri` 解析 JSON 时使用了错误的字段名 `"type"`，而 ACP SDK 的 `SessionUpdate` 枚举序列化后的 tag 字段为 `"sessionUpdate"`。这导致所有流式事件的类型判断始终返回空字符串，全部被静默丢弃。

## 症状详情

| 事件类型 | 期望行为 | 实际行为 |
|----------|----------|----------|
| 文本流式 | 逐字流式输出 | 循环结束时一次性出现 |
| 推理/thinking 流式 | 逐步显示思考过程 | 只显示字符数摘要，无流式内容 |
| 工具调用开始 | 即时显示工具卡片 | 延迟到 StateSnapshot 到达 |
| 工具调用结束 | 即时更新结果 | 延迟到 StateSnapshot 到达 |
| TodoUpdate | 即时更新列表 | 不更新 |

TUI 只能通过 Category ③ 的 `StateSnapshot`（每轮 ReAct 循环结束时发送）获取最终消息状态，导致所有内容表现为"突然一次性出现"而非流式渐进。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 bb388ca 及之后的版本启动 TUI
  2. 发送任何 prompt
  3. 观察响应：无流式输出，内容在 agent 完成一轮循环后一次性出现
- **环境**：所有 provider（Anthropic/OpenAI），所有权限模式

## 涉及文件

- `peri-tui/src/app/agent_ops/acp_bridge.rs`（80 行）—— `handle_session_update_peri` 中 `update.get("type")` 应为 `update.get("sessionUpdate")`
- `peri-acp/src/event/mapper.rs`（99-105 行）—— `AiReasoning` 映射为 `SessionUpdate::AgentThoughtChunk`，序列化后 tag 为 `"sessionUpdate"`
- `peri-acp/src/session/event_sink.rs`（54-77 行）—— `TransportEventSink::push_event` 将 `SessionUpdate` 序列化后通过 `session/update` 通知发送

## 根因

`acp_bridge.rs:80` 使用 `update.get("type")` 提取 SessionUpdate 类型，但 `SessionUpdate` 枚举的 serde 配置为 `#[serde(tag = "sessionUpdate", rename_all = "snake_case")]`，序列化后的 JSON 结构为 `{"sessionUpdate": "agent_thought_chunk", ...}`，不存在 `"type"` 字段。

## 修复

将 `acp_bridge.rs:80` 的 `update.get("type")` 改为 `update.get("sessionUpdate")`。提交 749fa34。
