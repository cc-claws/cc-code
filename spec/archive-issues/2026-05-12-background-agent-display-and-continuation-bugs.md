> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-background-agent-display-and-continuation-bugs.md

# Background Agent 三个 Bug：显示消失、subagent_type 限制、continuation 不触发

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-12

## 根因分析

### 根因 1：fork+background 走错路径

`invoke()` 中 fork 检测优先于 background 检测（`tool.rs:645-649`），当 LLM 同时设置 `fork:true` 和 `run_in_background:true` 时：

1. `invoke_fork`（同步阻塞）被调用，完全忽略 `run_in_background`
2. `map_executor_event` 仍从 input 读取 `run_in_background=true` → 发送 `SubAgentStart { is_background: true }`
3. `background_task_count` 被 +1 但永远不会被递减（fork 是同步的，不产生 BackgroundTaskCompleted）
4. Done 时 `background_task_count > 0` → `agent_done_pending_bg = true` → continuation 永远不触发

### 根因 2：frozen_subagent_vms 跨轮次膨胀导致错位

`done()` 从 `subagent_stack.drain` 中推入 `finalized_vm`，但 `tool_end_internal`（SubAgentEnd 时）已经推过一次。`frozen_subagent_vms` 是全局累积列表，只增不减。

跨多轮后 `merge_frozen_subagents` 按位置匹配错位——用旧轮次的 frozen vm 覆盖新轮次的 SubAgentGroup，导致显示内容被覆盖。

日志证据：

```
round1 done: frozen_count=1→2（重复推入）
round2 done: frozen_count=3→4（累积膨胀）
continuation reconcile: reconcile_subagent_count=0, frozen_count=4 → 错位替换
```

### 根因 3：pending_bg_continuation 竞态

`poll_agent` 中 `take()` 消费 continuation 后检查 `loading`，若 `loading=true`（auto-compact 期间），continuation 被永久丢失。

## 修复内容

### Fix 1: `tool.rs` — background 优先于 fork，新增 `invoke_background_fork`

- `invoke()` 中将 `run_in_background` 检测移到 fork 检测之前
- `invoke_background()` 新增 `is_fork` 参数
- 新增 `invoke_background_fork()` 方法：结合 fork 语义（继承父消息+系统提示词+工具集）与后台执行

### Fix 2: `message_pipeline.rs` — 修复 frozen_subagent_vms 重复推入

- `done()` 和 `interrupt()` 不再从 `subagent_stack.drain` 推入已有 `finalized_vm` 的条目
- 提取 `drain_subagent_stack()` 方法：只处理未 finalize 的异常残留

### Fix 3: `agent_ops.rs` — 修复 pending_bg_continuation 竞态

```rust
// Before: take() 在 loading=true 时丢失 continuation
if let Some(c) = pending_bg_continuation.take() {
    if !loading { submit_message(c); }
}

// After: 只在 loading=false 时才 take()
if !loading {
    if let Some(c) = pending_bg_continuation.take() {
        submit_message(c);
    }
}
```

## 涉及文件

| 文件 | 改动 |
|------|------|
| `peri-middlewares/src/subagent/tool.rs` | `invoke()` 分支重排、`invoke_background()` 新增 is_fork 参数、新增 `invoke_background_fork()` |
| `peri-tui/src/app/agent_ops.rs` | `poll_agent()` 中 `pending_bg_continuation` 竞态修复 |
| `peri-tui/src/app/message_pipeline.rs` | `done()`/`interrupt()` 不再重复推入 finalized_vm |
| `peri-tui/src/ui/headless_test.rs` | 新增 2 个诊断测试 |
