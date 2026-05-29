# /compact 显示"未知命令或 Skill"——AvailableCommandsUpdate 通知 JSON 格式不匹配被静默丢弃

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-29
**修复 commit**：a24d811

## 问题描述

ACP 大重构（bb388ca）引入了两条 `session/update` 发送路径，但两者的 JSON 格式不一致：
- `TransportEventSink`（event_sink.rs）发送 `{"update": {...}, "sessionId": "..."}`
- `notify.rs` 的三个 send_* 函数发送 `SessionNotification` 序列化格式（`{"sessionId": "...", "sessionUpdate": "available_commands_update", ...}`，无 `"update"` 外层）

TUI 侧 `handle_session_update_peri`（acp_bridge.rs）统一用 `params.get("update")` 解析。来自 `notify.rs` 的通知没有 `"update"` 字段 → 被 warn 日志丢弃 → `agent_commands` HashSet 永远为空 → `/compact`、`/clear`、`/model` 等 ACP 命令被 TUI 判定为"未知命令或 Skill"。

## 症状详情

| 操作 | 期望 | 实际 |
|------|------|------|
| 输入 `/compact` | 执行压缩 | 显示"未知命令或 Skill: /compact" |
| 输入 `/clear` | 清空历史 | 显示"未知命令或 Skill: /clear" |
| 输入 `/model` | 切换模型 | 显示"未知命令或 Skill: /model" |

所有非本地 UICommand 的 ACP slash commands 均不可用。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 bb388ca 及之后的版本启动 TUI
  2. 输入任何 `/` 开头的 ACP 命令（如 `/compact`）
  3. 观察到"未知命令或 Skill"错误提示
- **环境**：所有 provider

## 涉及文件

- `peri-tui/src/acp_server/notify.rs:74-93` — `send_available_commands_update` 使用 `SessionNotification` 序列化（无 `"update"` 外层）
- `peri-tui/src/acp_server/notify.rs:48-71` — `send_config_option_update` 同上
- `peri-tui/src/acp_server/notify.rs:96-112` — `send_session_info_update` 同上
- `peri-tui/src/app/agent_ops/acp_bridge.rs:66-72` — `handle_session_update_peri` 期望 `params.get("update")` 存在
- `peri-acp/src/session/event_sink.rs:64-67` — `TransportEventSink` 使用 `{"update": ..., "sessionId": ...}` 格式（正确）

## 修复

将 `notify.rs` 三个 send_* 函数改为与 `TransportEventSink` 一致的 `{"update": ..., "sessionId": ...}` 格式，移除 `SessionNotification` 包装。同时修复 bridge 中 `update.get("commands")` → `update.get("availableCommands")` 字段名不匹配。
