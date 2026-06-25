> 归档于 2026-05-16，原路径 spec/issues/2026-05-11-auto-compact-no-resubmit.md

# Auto Compact 后 Agent 未自动 Resubmit 继续执行

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-11
**修复 commit**：`505c034` fix(auto-compact): preserve user input across compact for reliable resubmit

## 问题描述

当上下文使用量超过阈值触发自动 full compact 后，`handle_compact_done` 应自动用原始用户输入 resubmit 重新启动 agent，但实际上 agent 停止执行，没有继续任务。

## 症状详情

- **触发条件**：agent 执行长任务，上下文使用量达到 `auto_compact_threshold`（默认 85%），触发 `ContextWarning` → `needs_auto_compact = true` → agent `Done` → `start_compact("auto")`
- **实际行为**：compact 完成后 agent 停止，不会自动 resubmit 继续执行
- **期望行为**：compact 完成后自动用 `last_user_input` 重新提交，agent 继续执行未完成的任务

## 复现条件

- **复现频率**：用户报告发生，具体复现频率待确认
- **触发步骤**：
  1. 向 agent 提交一个需要大量工具调用的任务（使上下文增长到 85%+）
  2. 等待 agent 完成当前轮次（Done 事件）
  3. 观察 auto-compact 是否触发
  4. compact 完成后观察 agent 是否自动 resubmit
- **环境**：auto_compact_enabled = true（默认）

## 根因分析

### Resubmit 逻辑

`handle_compact_done`（`peri-tui/src/app/agent_compact.rs:133-168`）中的 resubmit 逻辑：

```rust
const MAX_AUTO_COMPACT_RESUBMITS: u32 = 3;
if let Some(ref original_input) = self.session_mgr.sessions[...].agent.last_user_input {
    if self.session_mgr.sessions[...].agent.auto_compact_resubmit_count < MAX_AUTO_COMPACT_RESUBMITS {
        let input = original_input.clone();
        self.submit_message(input);
        self.session_mgr.sessions[...].agent.auto_compact_resubmit_count = new_count;
    } else {
        // 达到上限，显示提示
    }
}
```

### 可能的失败路径

**路径 1：`last_user_input` 为 `None`**

`last_user_input` 在 `submit_message`（`agent_submit.rs:134`）中设置。如果 compact 发生在 `submit_message` 未被调用的场景（如 session 恢复、后台任务 continuation），`last_user_input` 为 `None`，resubmit 被静默跳过——无日志、无提示。

**路径 2：`auto_compact_resubmit_count` 已达上限**

如果之前已经 resubmit 了 3 次（`MAX_AUTO_COMPACT_RESUBMITS = 3`），resubmit 被跳过并显示提示。但正常场景下不应这么快达到上限。

**路径 3：`submit_message` 内部失败**

`submit_message` 可能因为 channel 已关闭、agent task 启动失败等原因静默失败。需要检查 `submit_message` 的错误处理。

**路径 4：Compact 触发时机与后台任务冲突**

`Done` 事件处理器（`agent_ops.rs:348-357`）检查 `has_bg_tasks`：

```rust
let has_bg_tasks = self.session_mgr.sessions[...].background_task_count > 0;
if should_check_compact && needs_auto_compact && !has_bg_tasks {
    self.start_compact("auto".to_string());
}
```

如果 compact 在后台任务运行时被跳过（`needs_auto_compact` 保留），后续后台任务完成时由 `BackgroundTaskCompleted` 处理器触发。但此时 `last_user_input` 可能已经被清理或覆盖。

**路径 5：`submit_message` 重置 `auto_compact_resubmit_count`**

`submit_message`（`agent_submit.rs:137`）将 `auto_compact_resubmit_count` 重置为 0。`handle_compact_done` 在调用 `submit_message` 后立即将其设为 `new_count`（第 155-157 行）。但如果 `submit_message` 是异步的且 resubmit 的 agent 也触发了 compact，可能存在竞态。

### 最可能的根因

**路径 1（`last_user_input` 为 `None`）** 是最可能的根因。compact 流程中没有任何地方保证 `last_user_input` 有值，也没有在为 `None` 时给出提示。用户看到的现象就是 "compact 完成后 agent 停止了"，没有任何错误信息。

## 修复方案

### 方案 A：添加诊断日志和用户提示

在 `handle_compact_done` 的 resubmit 分支添加日志，当 `last_user_input` 为 `None` 时给出明确提示：

```rust
if let Some(ref original_input) = self.session_mgr.sessions[...].agent.last_user_input {
    // ... resubmit logic ...
} else {
    tracing::warn!("auto-compact: last_user_input is None, cannot resubmit");
    let vm = MessageViewModel::system(
        "上下文已压缩，但无法自动继续（原始输入丢失）。请重新输入任务。".to_string()
    );
    self.apply_pipeline_action(PipelineAction::AddMessage(vm));
}
```

### 方案 B：保证 `last_user_input` 在 compact 流程中始终可用

在 `start_compact` 被调用时，将 `last_user_input` 保存到独立字段（如 `pre_compact_user_input`），防止在 compact 异步执行期间被清理：

```rust
// agent_ops.rs, start_compact 调用前
self.session_mgr.sessions[...].agent.pre_compact_user_input =
    self.session_mgr.sessions[...].agent.last_user_input.clone();
```

在 `handle_compact_done` 中优先使用 `pre_compact_user_input`。

## 相关代码

- `peri-tui/src/app/agent_compact.rs:133-168` —— `handle_compact_done` resubmit 逻辑
- `peri-tui/src/app/agent_submit.rs:131-137` —— `last_user_input` 设置和计数器重置
- `peri-tui/src/app/agent_ops.rs:342-382` —— Done 事件中的 auto-compact 两级策略
- `peri-tui/src/app/agent_ops.rs:67-89` —— `ContextWarning` 设置 `needs_auto_compact`
- `peri-tui/src/app/agent_comm.rs:74` —— `last_user_input` 字段定义
- `peri-tui/src/app/agent_comm.rs:109` —— `last_user_input` 默认值 `None`
