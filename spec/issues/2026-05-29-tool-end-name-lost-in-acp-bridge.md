# ToolEnd 事件经 ACP bridge 后工具名丢失，显示为空字符串

**状态**：Open
**优先级**：中
**创建日期**：2026-05-29

## 问题描述

ACP 事件映射重构（bb388ca）中，`ExecutorEvent::ToolEnd` 被映射为 `SessionUpdate::ToolCallUpdate`。但 `ToolCallUpdate` 的 ACP schema 只有 `toolCallId`、`status`、`rawOutput` 字段，不携带工具名。TUI 侧 `handle_session_update_peri` 在 `tool_call_update` 分支中硬编码 `name: String::new()`，导致所有通过 session/update 路径到达的 ToolEnd 事件丢失工具名。

## 症状详情

| 场景 | 期望行为 | 实际行为 |
|------|----------|----------|
| 工具调用完成 | ToolBlock 显示工具名（如 "Bash"、"Read"） | 工具名为空字符串 |
| AskUserQuestion 结果 | 显示 `? → {output}` 特殊格式 | 回退为通用 ToolBlock 显示 |
| 错误工具 | 显示 `{工具名}: ✗ {error}` | 显示 `: ✗ {error}`（名称为空） |

## 复现条件

- **复现频率**：必现（所有工具调用）
- **触发步骤**：
  1. 使用 bb388ca 及之后的版本启动 TUI
  2. 发送任何触发工具调用的 prompt
  3. 观察工具调用完成后的 ToolBlock：工具名为空
- **环境**：所有 provider

## 涉及文件

- `peri-acp/src/event/mapper.rs:125-143` — `ToolEnd` 映射为 `ToolCallUpdate`，schema 无 `name` 字段
- `peri-tui/src/app/agent_ops/acp_bridge.rs:168-174` — bridge 中 `name: String::new()` 硬编码为空
- `peri-tui/src/app/agent.rs` — `map_executor_event` 中 ToolEnd 仍正确携带 name（类别③路径），但类别①路径丢失
