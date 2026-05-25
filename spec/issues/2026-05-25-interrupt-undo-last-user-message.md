# Ctrl+C 中断后支持撤回并重发上一条用户消息

**状态**：已修复（多层修复，已实现并验证）
**优先级**：中
**创建日期**：2026-05-25
**修复日期**：2026-05-25

## 问题描述

用户发送消息后、Agent 执行过程中按 Ctrl+C 中断时，已发送的用户消息（可能包含输入错误）仍保留在对话历史中，无法撤回或修改。用户需要能够在中断后自动将上一条消息从历史中移除，并重新编辑后再次发送。

## 当前行为

1. 用户在输入框中输入消息并按 Enter 发送
2. Agent 开始执行（LLM 调用/工具调用）
3. 用户发现消息输错，按 Ctrl+C 中断 Agent 执行
4. 中断完成后，输错的消息仍留在聊天记录中，下一轮对话 Agent 仍能看到这条消息
5. 用户只能在新一轮中追加补充，无法撤回或修改之前的消息

## 期望行为

1. 用户发送消息后按 Ctrl+C 中断 Agent 执行
2. 中断完成后，**自动**将上一条用户消息从历史中移除
3. 移除的消息内容自动填入输入框，用户可直接编辑后重新发送
4. 重新发送后作为新消息进入对话历史

## 症状详情

| 维度 | 当前 | 期望 |
|------|------|------|
| Ctrl+C 中断后消息状态 | 保留在历史中 | 自动从历史中移除 |
| 消息内容 | 不可修改 | 自动填入输入框供编辑 |
| 重新发送 | 需重新手动输入全部内容 | 编辑后直接发送 |
| 对话连续性 | 错误消息成为上下文的一部分 | 干净重试，不含错误消息 |

## 涉及文件

- `peri-tui/src/app/agent_ops/lifecycle.rs`（Agent 中断处理逻辑）—— 中断时移除最后一条用户消息
- `peri-tui/src/app/agent_ops/acp_bridge.rs`（ACP 通知处理）—— 中断事件处理
- `peri-tui/src/app/agent_submit.rs`（消息提交）—— 撤回后重发逻辑
- `peri-tui/src/app/agent_compact.rs`（compact 相关）—— 可能涉及消息清理
- `peri-agent/src/agent/state.rs`（Agent 状态管理）—— 消息历史移除接口
- `peri-tui/src/app/message_pipeline.rs`（消息渲染管线）—— UI 侧消息移除 + 输入框回填

## 实现方案（4 层修复）

### 架构概览

中断撤回涉及 4 层独立修复：

```
Ctrl+C → cancel token
→ AgentError::Interrupted
→ AgentExecutionFailed { message: "Interrupted by user" }
→ [事件路由] → AgentEvent::Interrupted (not Error)
→ [ACP Server] → state.history.truncate(history_len)  // 回滚后端历史
→ [TUI] → handle_interrupted()
    ├─ 有工具调用？ → 只中断，保留历史，显示 "app-interrupt-done"
    └─ 无工具调用？ → 撤回用户消息，恢复文本框，显示 "app-interrupted-resumed"
```

### Layer 1: ACP Server — 历史回滚

**文件**：`peri-tui/src/acp_server/prompt.rs`

**问题**：`state.history = result.messages` 无条件执行，取消后错误消息仍留在 ACP 对话状态中。

**修复**（commit `e12fbea`）：
```rust
if result.ok {
    state.history = result.messages;
} else {
    state.history.truncate(history_len); // 回滚到提交前长度
}
```

### Layer 2: 事件路由 — 取消事件走正确的 TUI 处理器

**文件**：`peri-tui/src/app/agent.rs`

**问题**：`AgentExecutionFailed` 无论什么原因都映射为 `AgentEvent::Error`，导致 `handle_error()`（显示错误 VM）而不是 `handle_interrupted()`（撤回消息 + 恢复文本框）。`AgentEvent::Interrupted` 从未被生成，`handle_interrupted()` 是死代码。

**修复**（commit `8a77f15`）：
```rust
ExecutorEvent::AgentExecutionFailed { message } => {
    if message == "Interrupted by user" {
        AgentEvent::Interrupted  // 走正确的撤回路径
    } else {
        AgentEvent::Error(message)
    }
}
```

### Layer 3: 行为分叉 — 有工具调用时不撤回

**文件**：`peri-tui/src/app/agent_ops/lifecycle.rs`

**问题**：Agent 已执行工具调用后中断，如果不加区分地撤回所有消息，会丢失有价值的工具调用上下文。

**修复**（commit `d43934b`）：在 `handle_interrupted()` 中检测 `view_messages` 是否包含 `ToolCallGroup` 或 `ToolBlock`：

| 场景 | 行为 |
|------|------|
| 纯文本/思考中 Ctrl+C | 撤回用户消息 → 恢复文本框 → `app-interrupted-resumed` |
| 工具调用执行中 Ctrl+C | 只中断工具 → 保留对话历史 → `app-interrupt-done` |

### Layer 4: 可靠的 VM 定位 — rposition 替代 round_start_vm_idx

**文件**：`peri-tui/src/app/agent_ops/lifecycle.rs`、`peri-tui/src/app/mod.rs`

**问题**：`round_start_vm_idx` 是 submit 时记录的 VM 索引。Pipeline 的 `request_rebuild()`（由 StateSnapshot 事件触发）会调用 `RebuildAll { prefix_len: round_start_vm_idx, tail_vms: build_tail_vms() }`，重建尾部 VMs。重建后 `round_start_vm_idx - 1` 不再指向 UserBubble，导致用户消息残留。

**修复**（commit `f742246`）：用 `rposition` 直接在 `view_messages` 中搜索最后一个 `UserBubble`，免疫 Pipeline 重建导致的索引漂移。

### 涉及文件（实际）

| 文件 | 变更 |
|------|------|
| `peri-tui/src/acp_server/prompt.rs` | Layer 1：回滚 state.history |
| `peri-tui/src/app/agent.rs` | Layer 2：事件路由 |
| `peri-tui/src/app/agent_ops/lifecycle.rs` | Layer 3+4：行为分叉 + VM 定位 |
| `peri-tui/src/app/mod.rs` | Layer 4：强制清理路径同步修复 |
| `peri-tui/src/app/agent_test.rs` | 新增事件路由测试用例 |

### Commits

| SHA | 描述 |
|-----|------|
| `e12fbea` | feat(tui): interrupt undo — rollback history on cancel + always restore textarea |
| `8a77f15` | fix(tui): route cancel-originated AgentExecutionFailed to Interrupted instead of Error |
| `d43934b` | fix(tui): only undo user message on interrupt when no tool calls were made |
| `f742246` | fix(tui): locate UserBubble by rposition instead of stale round_start_vm_idx |
