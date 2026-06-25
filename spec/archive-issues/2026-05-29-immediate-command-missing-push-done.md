> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-immediate-command-missing-push-done.md
# Immediate 命令（/compact、/clear）执行后 TUI 永久卡在 loading 状态

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-29
**修复日期**：2026-05-29
**修复 commit**：a24d811

## 问题描述

ACP 命令系统重构（bb388ca）将 `/compact`、`/clear` 从 TUI 层下沉至 `peri-acp` 的 `session/command/` 模块。Immediate 命令在 `execute_prompt` 中直接 `return PromptResult`，绕过了 agent event pump。但 event pump 结束时会调用 `sink.push_done()` 发送 `peri/agent_event_done` 通知——Immediate 命令跳过了这一步，导致 TUI 永远收不到 `AgentDone` 事件，界面永久卡在 loading 状态，用户无法继续操作。

此外，`/clear` 命令只返回空 messages 列表，不发送 `StateSnapshot` 事件来通知 TUI 清空本地状态。即使 `push_done` 问题修复后，TUI 的 `view_messages` 和 `origin_messages` 仍保留旧数据。

## 症状详情

### 症状 1：/compact 和 /clear 执行后界面冻结

| 命令 | TUI 期望行为 | 实际行为 |
|------|-------------|----------|
| `/compact` | 显示"正在压缩..."→ 替换消息 → 恢复可操作 | compact 完成后 TUI 卡在 loading，无法输入 |
| `/clear` | 清空所有消息 → 恢复可操作 | TUI 卡在 loading，无法输入 |

用户只能通过 Ctrl+C 强制退出或等待 cancel_sent_at 超时（5 秒）。

### 症状 2：/clear 不清空 TUI 本地视图

即使 `push_done` 修复后 `handle_done` 能触发，pipeline 的 reconcile 使用旧的 `origin_messages`，`view_messages` 不会被清空——旧消息残留。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 bb388ca 及之后的版本启动 TUI
  2. 发送任何 prompt 让 agent 完成
  3. 输入 `/compact` 或 `/clear`
  4. 观察：命令执行完毕但 TUI 仍显示 loading 状态，输入框不可用
- **环境**：所有 provider、所有权限模式

## 涉及文件

- `peri-acp/src/session/executor.rs:161-183` — Immediate 命令拦截路径，直接 `return PromptResult` 不调用 `push_done`
- `peri-acp/src/session/executor.rs:333` — 正常 agent 路径的 `sink.push_done()` 调用（Immediate 路径未走到这里）
- `peri-acp/src/session/command/clear.rs:31-36` — `ClearCommand::execute` 只返回空 messages，不发送 StateSnapshot
- `peri-acp/src/session/command/compact.rs` — `CompactCommand::execute` 手动调用 `push_event` 但不调用 `push_done`
- `peri-tui/src/app/agent_ops/lifecycle.rs:38` — `handle_done()` 负责清理 loading 状态和 pipeline

## 修复

1. **push_done 缺失**：在 executor 的 Immediate 命令路径末尾补上 `event_sink.push_done(&session_id)`
2. **compact_manual 标志**：在 `agent_compact.rs` 中设置 `compact_manual = true`，`handle_compact_completed` 依赖此标志正确结束 loading
3. **并发 prompt 竞争**：`session/prompt` 在 `tokio::spawn` 中执行，两个 prompt（如 hello + /compact）并发运行，compact 在 `state.history` 更新前就读取了空 history。加 per-session `tokio::sync::Mutex` 串行化同一 session 的 prompt 请求（`acp_server/mod.rs`）
4. **AvailableCommandsUpdate 字段名**：ACP schema 序列化为 `"availableCommands"` 但 bridge 用 `update.get("commands")` 解析。改为 `update.get("availableCommands").or_else(|| update.get("commands"))` 兼容两种格式
