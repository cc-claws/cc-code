> 归档于 2026-05-20，原路径 spec/issues/2026-05-12-compact-auto-continue-scenarios.md

# Compact 自动继续功能在不应触发的场景下仍然 resubmit

**状态**：Fixed（`3c0b2cd`）
**优先级**：中
**创建日期**：2026-05-12
**修复日期**：2026-05-12

## 问题描述

`handle_compact_done` 中的 auto-continue resubmit 逻辑不区分 compact 的触发来源，在所有场景下都会尝试用 `last_user_input` 重新提交任务。但以下两种场景不应触发自动继续：

1. **用户手动 `/compact`**：用户主动压缩上下文后，期望停下来查看压缩结果或输入新任务，而不是 agent 立即用上一次的输入自动重新执行
2. **Agent 正常完成任务后触发的 auto-compact**：agent Done 表示任务已完成，compact 后不应再用原始输入重新启动 agent（任务已结束，重新执行没有意义）

## 症状详情

### 当前行为

`handle_compact_done`（`agent_compact.rs:125-189`）中，只要 `resubmit_input` 有值且未达到 resubmit 上限，就无条件调用 `submit_message(original_input)` 继续执行。没有检查 compact 的触发来源。

### 期望行为

| 触发来源 | instructions 参数 | 是否应自动继续 |
|----------|------------------|---------------|
| Agent 执行中上下文超限（auto-compact） | `"auto"` | ✅ 是 |
| 后台任务完成后延迟 auto-compact | `"auto"` | ✅ 是 |
| 用户手动 `/compact` | 用户输入的指令 | ❌ 否 |
| Agent 正常完成任务后 auto-compact | `"auto"` | ❌ 否 |

## 复现条件

- **复现频率**：必现
- **触发步骤（手动 compact）**：
  1. 向 agent 提交一个任务并等待完成
  2. 输入 `/compact` 手动压缩上下文
  3. 观察：compact 完成后 agent 自动用上一次的任务输入重新开始执行
- **触发步骤（任务完成后 auto-compact）**：
  1. 向 agent 提交一个会产生大量上下文的任务
  2. 等待 agent 完成（Done 事件），同时上下文超限触发 auto-compact
  3. 观察：compact 完成后 agent 用已完成的任务输入重新开始执行

## 相关代码

- `peri-tui/src/app/agent_compact.rs:125-189` —— `handle_compact_done` resubmit 逻辑，当前不区分触发来源
- `peri-tui/src/app/thread_ops.rs:302` —— `start_compact(instructions)`，`instructions` 参数区分 `"auto"` 和用户输入
- `peri-tui/src/app/agent_ops.rs:395` —— Done 事件中 auto-compact 触发（`start_compact("auto")`）
- `peri-tui/src/app/agent_events_bg.rs:190` —— 后台任务完成后延迟 auto-compact 触发
- `peri-tui/src/command/compact.rs:25` —— `/compact` 命令触发（`start_compact(args)`）

## 修复方案

添加 `compact_should_resubmit: bool` flag 到 `AgentComm`（`agent_comm.rs:87`）：
- `start_compact()` 根据 `instructions == "auto"` 设置 flag（`thread_ops.rs:399`）
- Done handler 和 BG completion handler 调用 `start_compact("auto")` 后立即覆盖为 `false`
- `handle_compact_done` 读取 flag 后清除，仅当 `true` 时 resubmit
- `reset_agent_session` 重置 flag（`thread_ops.rs:121`）

## 回归测试

- `test_manual_compact_does_not_resubmit` — 手动 `/compact` 完成后不 resubmit
- `test_post_done_auto_compact_does_not_resubmit` — Done 后 auto-compact 完成后不 resubmit

## 关联 Issue

- `spec/issues/2026-05-11-auto-compact-no-resubmit.md`（状态：Fixed）—— 上一次修复了 resubmit 不工作的问题，本次是其后续：resubmit 工作了但在不该触发的场景下也触发了
