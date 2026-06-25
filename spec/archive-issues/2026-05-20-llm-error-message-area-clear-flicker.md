> 归档于 2026-05-20，原路径 spec/issues/2026-05-20-llm-error-message-area-clear-flicker.md

# LLM 返回 400 时消息区域闪烁清空回到空白页

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-20
**修复日期**：2026-05-20

## 问题描述

使用 DeepSeek 模型时，消息流式输出过程中 LLM 返回 400 错误，TUI 消息区域出现短暂闪烁后完全清空，回到初始空白页面状态。历史对话内容丢失，用户无法看到之前的消息。

## 根因分析

两个相互叠加的 bug：

### Bug 1（根因）：Compact 后 round_start_vm_idx=0 + LLM 失败 = 视图完全清空

`handle_compact_completed()` 将 `round_start_vm_idx` 重置为 0。如果 compact 后下一次 LLM 调用在 StateSnapshot 到达之前就失败（如 400 错误），`handle_done()` 触发 `request_rebuild()` 时 `prefix_len=0`，`build_tail_vms()` 因 `has_snapshot_this_round=false` 跳过 reconcile 返回空 tail，`view_messages.drain(0..)` 完全清空。

### Bug 2（加剧因素）：Executor 不发送 Error 事件

`executor.rs` 在 `agent.execute()` 返回 Err 时只 log 不通知前端。TUI 只收到 `Done`，`handle_done()` 总是调用 `request_rebuild()` 且用户看不到任何错误信息。

## 修复方案

### Bug 1 修复

Compact 架构从「外层 loop + resubmit」 改为「`CompactMiddleware` 作为 `before_model` 钩子在 ReAct 循环内原地处理」。摘要作为 `BaseMessage::human()` 存入 state，compact 后 ReAct 循环自然继续调用 LLM，不再有 "compact 后 + LLM 失败 + round_start_vm_idx=0" 的组合。

### Bug 2 修复

`executor.rs:264-268` — agent 执行失败时发送 `AgentExecutionFailed` 事件通知前端：

```rust
// peri-acp/src/session/executor.rs:264-268
if let Some(tx) = event_tx.lock().unwrap().as_ref() {
    let _ = tx.send(ExecutorEvent::AgentExecutionFailed {
        message: e.to_string(),
    });
}
```

### 修改文件

| 文件 | 变更 |
|------|------|
| `peri-acp/src/session/executor.rs:264-268` | 新增 `AgentExecutionFailed` 事件发送 |
| `peri-middlewares/src/compact_middleware.rs` | 新增 `CompactMiddleware`，替换旧的 compact 循环 |
| `peri-acp/src/session/compact_runner.rs` | 已删除 |
| `peri-tui/src/app/agent_compact.rs` | TUI 侧 compact 事件处理简化 |
| `peri-tui/src/app/message_pipeline/mod.rs` | reconcile 逻辑，compact 后正确恢复消息 |

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发时机 | 消息流式输出过程中，LLM API 返回错误 |
| 闪烁表现 | 消息区域短暂闪烁一下 |
| 清空表现 | 整个消息区域被清空，回到初始空白/欢迎页面状态 |
| 历史消息 | 之前对话内容在界面上消失 |

### 错误日志

```
▶ 2026-05-20T08:47:42.577Z  POST [anthropic]
   UPSTREAM: https://api.deepseek.com/anthropic/v1/messages
◀ 2026-05-20T08:47:42.693Z  [anthropic]  → 400  (114ms)
```
