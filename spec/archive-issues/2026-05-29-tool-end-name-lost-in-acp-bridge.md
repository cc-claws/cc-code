> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-tool-end-name-lost-in-acp-bridge.md
# ToolEnd 事件经 ACP bridge 后工具名丢失，显示为空字符串

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-29
**修复日期**：2026-05-29

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

## 根因分析

`mapper.rs` 将 `ExecutorEvent::ToolEnd` 映射为 `ToolCallUpdate` 时，`ToolCallUpdateFields` 缺少 `.title(name)` 调用（对比 `transport_broker.rs:57` 的 HITL 路径正确使用了 `.title()`）。TUI 侧 `acp_bridge.rs` 解析 `tool_call_update` 时也硬编码 `name: String::new()` 未从 JSON 中读取 `title` 字段。

## 修复

1. `peri-acp/src/event/mapper.rs` — `ToolCallUpdateFields` 添加 `.title(name.clone())`，绑定 `name` 到 pattern
2. `peri-tui/src/app/agent_ops/acp_bridge.rs` — 从 JSON `title` 字段解析工具名，替代 `String::new()`
3. `peri-acp/src/event/mapper_test.rs` — 新增 `test_tool_end_carries_title` 回归测试

## 涉及文件

- `peri-acp/src/event/mapper.rs:125-148` — `ToolEnd` 映射，添加 `.title(name.clone())`
- `peri-tui/src/app/agent_ops/acp_bridge.rs:146-180` — bridge 解析 `title` 字段
- `peri-acp/src/event/mapper_test.rs` — 回归测试
