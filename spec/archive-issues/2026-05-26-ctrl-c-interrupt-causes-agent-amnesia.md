> 归档于 2026-05-27，原路径 spec/issues/2026-05-26-ctrl-c-interrupt-causes-agent-amnesia.md

# Ctrl+C 中断后继续对话时 agent 丢失当前轮次上下文

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-26

## 问题描述

在同一 TUI session 内，用户按 Ctrl+C 中断 agent 输出后继续发消息，agent 能记住之前轮次的对话内容，但**不记得被中断那一轮**的上下文。UI 界面上历史消息正常显示（包括被中断的轮次），但 agent 回复时表现为「失忆」——仅当前被中断轮次的上下文丢失。

## 症状详情

| 维度 | 表现 |
|------|------|
| 失忆范围 | 仅被 Ctrl+C 中断的当前轮次，之前轮次正常 |
| UI 显示 | 界面上所有历史消息正常显示，包括被中断轮次的 UserBubble 和部分响应 |
| Agent 行为 | 继续对话时，agent 回复表明不记得被中断轮次中讨论的内容 |
| 复现频率 | 必现 |
| 中断时机 | 流式文本输出时、工具执行时均会出现 |
| 场景 | 同一 TUI 进程内继续对话（非重启 TUI） |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，与 agent 进行多轮对话
  2. 在最新一轮 agent 输出过程中，按 Ctrl+C 中断
  3. 在输入框中继续发送新消息
  4. 观察 agent 回复：agent 能记住步骤 1 之前轮次的上下文，但不记得被中断轮次（步骤 2）中讨论的内容
- **环境**：所有模型、所有 OS

## 涉及文件

- `peri-tui/src/app/agent_ops/lifecycle.rs` —— `handle_interrupted()` 中断处理逻辑，包含 `agent_state_messages.truncate(pre_len)` 回滚操作
- `peri-tui/src/app/agent_ops/acp_bridge.rs` —— ACP 通知桥接，将 `Interrupted` 事件转换为 TUI `AgentEvent`
- `peri-tui/src/app/agent_submit.rs` —— 新消息提交时 `agent_state_messages` 的构建逻辑

## 根因

`peri-tui/src/acp_server/prompt.rs` 中，当 `result.ok == false`（包括 Cancelled）时，无条件执行 `state.history.truncate(history_len)`，将 ACP server 端的消息历史回滚到本轮提交前的状态。

关键数据流断裂点：
1. TUI `handle_interrupted`（有工具调用路径）：保留 `agent_state_messages` 和 view_messages
2. ACP server `prompt.rs`：truncate `state.history` → 丢弃当前轮次全部消息
3. 下一轮 prompt：ACP server 传入 truncated history → agent 无当前轮次上下文 → 失忆

**安全保证**：deferred write 模式确保 `result.messages` 在 cancel 后始终合法——`dispatch_tools` 在写入 state 后才返回 `Err(Interrupted)`，不存在孤立 tool_use。

## 修复

- `peri-tui/src/acp_server/prompt.rs`：Cancelled 时检查 `result.messages.len() > history_len + 1`（agent 有进展），若有则保留历史并用 `strip_leaked_prepends` 剥离 `execute()` 错误路径泄漏的 system prepend 消息
- 新增 `strip_leaked_prepends` 辅助函数：通过原始历史首条消息 ID 定位，剥离 leaked system prepends
- 新增 4 个单元测试覆盖：有历史/空历史/ID 找不到/无 leaked 四种场景
