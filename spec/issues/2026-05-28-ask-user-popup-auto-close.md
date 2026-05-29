# AskUser 弹窗短暂显示后自动关闭

**状态**：Open
**优先级**：高
**创建日期**：2026-05-28
**类型**：Bug

## 问题描述

Agent 调用 AskUserQuestion 工具后，AskUser 问答弹窗仅短暂显示（< 1 秒）即自动消失，用户来不及操作。弹窗消失后底部状态栏显示 agent 执行已完成（loading 停止）。用户期望弹窗保持在屏幕上等待用户选择/输入后才关闭。

## 症状详情

| 现象 | 期望 | 实际 |
|------|------|------|
| 弹窗持续时间 | 一直显示直到用户确认 | 极短时间内自动消失 |
| 弹窗关闭后状态 | loading 持续，等待用户回答 | loading 停止，agent 似乎已完成 |
| 用户输入的答案 | 用户选择后提交给 LLM | 弹窗已关闭无法操作 |

### 状态流转

弹窗显示由 `AgentComm.interaction_prompt` 控制：

1. **弹窗创建**：`handle_acp_elicitation()` 设置 `interaction_prompt = Some(Questions(...))`
2. **弹窗清除**：`cleanup_agent_state()` 设置 `interaction_prompt = None`
3. `cleanup_agent_state()` 的调用路径：
   - `handle_done()` → `cleanup_agent_state(None)`
   - `handle_error()` → `cleanup_agent_state(Some(...))`
   - `poll_agent()` 通道断开处理器 → `cleanup_agent_state(None)`
   - `poll_agent()` cancel 超时（5 秒）→ `cleanup_agent_state(None)`

## 疑似根因分析

### 根因 1：cancel 超时残留（可能性高）

`submit_message()` 在新 prompt 启动时**未清理 `cancel_sent_at`**。

**时序**：
1. 用户上一个 prompt 按 Ctrl+C 中断 → `cancel_sent_at = Some(now)`
2. 5 秒超时安全网触发（若 Interrupted 事件未及时到达）→ `cleanup_agent_state()` → 清除 `cancel_sent_at`
3. 但如果用户在 5 秒内提交了新消息：`submit_message()` → `loading = true`，但 `cancel_sent_at` 仍为旧值
4. 随后 `poll_agent()` 检查 `cancel_sent_at`，距原始 Ctrl+C 超过 5 秒 → 触发超时 → `cleanup_agent_state()` → 清空弹窗

`submit_message()` 目前重置的状态：
```
✓ subagent_depth = 0
✓ agent_replied = false
✓ reconcile_already_done = false
✗ cancel_sent_at — 未清理！
```

### 根因 2：AgentDone 在弹窗期间到达（可能性中）

正常流程中 agent 在 elicitation 处阻塞，不应发送 `AgentDone`。但以下边缘情况可能导致 `AgentDone` 在被用户回答前到达：

- `AcpTransportBroker::handle_questions()` 的 `send_request` 若返回错误，直接返回空答案（`empty_answers`），agent 继续执行
- 若 broker 的 `send_request` 因某种原因快速失败，`AgentDone` 会在弹窗创建后的同一帧或下一帧到达

### 根因 3：`agent_rx` 通道断开（可能性低）

`poll_agent()` 在 ACP 路径下先尝试 `acp_notification_rx`，若为空则 fallback 到 `agent_rx`。若 `agent_rx` 被错误地设置为某旧通道且已断开，断开处理器会调用 `cleanup_agent_state()`。

但在当前 ACP 架构下，`agent_rx` 通常为 `None`，不应触发此路径。

## 涉及文件

- `peri-tui/src/app/agent_submit.rs:73` — `submit_message()` 设置 `loading=true` 但未清理 `cancel_sent_at`
- `peri-tui/src/app/agent_ops/lifecycle.rs:25-28` — `cleanup_agent_state()` 清空 `interaction_prompt`
- `peri-tui/src/app/agent_ops/polling.rs:12-29` — cancel 超时检查，无条件调用 `cleanup_agent_state()`
- `peri-tui/src/app/agent_ops_interaction.rs:55-156` — `handle_acp_elicitation()` 创建弹窗
- `peri-tui/src/app/agent_comm.rs:85` — `cancel_sent_at` 定义，会在提交新 prompt 时残留
- `peri-tui/src/app/agent_ops/acp_bridge.rs:38` — `AgentDone` 通知路由到 `handle_done()`

## 诊断结论（2026-05-28）

通过 `systematic-debugging` 流程完成静态分析 + 追踪代码变更历史（commit `0211c41` 引入 `cancel_sent_at` 安全网），**确认根因为疑似根因 1：`cancel_sent_at` 残留**。

### 确认的触发时序

1. 用户按下 Ctrl+C 中断正在运行的 agent → `interrupt()` 设置 `cancel_sent_at = Some(now)` 并发送 ACP cancel
2. ACP server 处理 cancel → `AgentEvent::Interrupted` 到达 → `handle_interrupted()` 清理 `cancel_sent_at = None`，**同时清空 `pending_messages`，改由 Interrupted 处理中的 `flush_pending_messages()` 启动新 prompt**
3. `handle_interrupted()` 内调用 `flush_pending_messages()` → 内部调用 `submit_message()` → 设置 `loading = true`，`task_start_time = Some(now2)`，**但未清 `cancel_sent_at`（仍为步骤 1 的时间戳）**
4. 新 prompt 启动，agent 执行。若 LLM 调用 AskUserQuestion → 弹窗显示
5. `poll_agent()` 每帧检查 `cancel_sent_at`：距步骤 1 超过 5 秒 → 超时触发 → `cleanup_agent_state()` → 清空 `interaction_prompt`（弹窗消失）

### 排除的根因

- **根因 2（AgentDone 在弹窗期间到达）**：经验证，`AgentDone` 仅在 agent 完成执行后发送，而 agent 被 elicitation 阻塞。`AgentDone` 不可能在弹窗显示期间到达。
- **根因 3（agent_rx 通道断开）**：ACP 路径下 `agent_rx` 始终为 `None`，不会触发断开处理器。

### 诊断埋点

在以下位置添加了 `tracing::warn!` / `tracing::info!` 诊断日志（保留用于后续验证）：

| 文件 | 位置 | 内容 |
|------|------|------|
| `agent_ops_interaction.rs` | 弹窗创建 | `"DIAG: AskUser popup CREATED"` |
| `agent_ops/acp_bridge.rs` | AgentDone 到达 | `"DIAG: ACP→TUI AgentDone received"` 含 `has_popup` |
| `agent_ops/lifecycle.rs` | `cleanup_agent_state()` 清空弹窗 | `"DIAG: cleanup_agent_state clearing interaction_prompt"` 含 `cancel_sent_elapsed` |
| `agent_ops/polling.rs` | cancel 超时触发 | `"cancel timeout: 5s elapsed"` 含 `elapsed_secs`/`has_popup` |

可通过 `RUST_LOG=peri_tui=info cargo run -p peri-tui` 运行时追踪事件顺序。

## 修复记录（2026-05-28）

**修复**：`agent_submit.rs` — 在 `submit_message()` 和 `flush_pending_messages()` 两个新任务启动点，`task_start_time` 设值前增加 `cancel_sent_at = None` 清理。

**diff**：
```rust
// 开始计时新任务——清理上一个 prompt 可能残留的 cancel 状态
self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .cancel_sent_at = None;
self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .task_start_time = Some(std::time::Instant::now());
```

**待验证**：用户实际测试确认 AskUser 弹窗不再自动关闭。
