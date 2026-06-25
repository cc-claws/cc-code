> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication.md

# SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16

## 问题描述

多轮对话中使用 SubAgent 后，后续轮次的 SubAgentGroup 会显示前一轮 SubAgent 的数据，导致前一轮的 SubAgent 内容同时出现在两个位置：作为**轮次 1 的批次卡片**保留在 view_messages 前缀中，以及作为**轮次 2 的「独立」SubAgentGroup**（因内容被错误覆盖而显示轮次 1 的数据）。用户看到同一个 SubAgent 内容在界面中出现两次——一次是原始批次，一次是后续轮次中被错误填充的内容。

## 症状详情

| 轮次 | SubAgent 调用 | 预期显示 | 实际显示 |
|------|--------------|---------|---------|
| 轮次 1 | 并发 2 个 SubAgent（sa1, sa2） | 1 个批次卡片 | 1 个批次卡片 ✅ |
| 轮次 2 | 并发 1 个 SubAgent（sa3） | 1 个独立卡片 | 1 个独立卡片 + 轮次 1 的批次卡片仍可见 ⚠️ |
| 轮次 2 的独立卡片 | — | 显示 sa3 的信息 | 显示 sa1/sa2 的信息（内容被覆盖）❌ |

实际上表现是：「轮次 1 的批次卡片」（显示 "2 agents"）+ 「轮次 2 的独立卡片」（但显示的是轮次 1 的 agent 名和 task_preview），看起来同一批 SubAgent 内容被显示了两次——一次是批次形式，一次是「单独」形式。

## 复现条件

- **复现频率**：必现（多轮后）
- **触发步骤**：
  1. 启动 TUI，发送一条包含 `/dispatching-parallel-agents` 的消息（触发并发 SubAgent）
  2. 等待 SubAgent 全部完成、父 agent Done
  3. 再发送另一条触发 SubAgent 的消息（单个 SubAgent 或并发）
  4. 观察第二轮 SubAgent 卡片的 agent_id 和 task_preview
- **环境**：任意模型，YOLO 模式或 HITL 模式均可

## 相关代码

- `peri-tui/src/app/message_pipeline.rs:152` —— `frozen_subagent_vms: Vec<MessageViewModel>` 仅在 `clear()` 中清空（行 580），`done()`（行 518-527）和 `begin_round()`（行 646-651）均未清空
- `peri-tui/src/app/message_pipeline.rs:455` —— `tool_end_internal` 中 `self.frozen_subagent_vms.push(vm)` 持续累积
- `peri-tui/src/app/message_pipeline.rs:757` —— `merge_frozen_subagents` 按位置从 frozen_vms[0] 开始取用，取到的是最旧的条目
- `peri-tui/src/app/message_pipeline.rs:37-52` —— `merge_frozen_subagents` 函数：`frozen_vms[frozen_idx]` 位置匹配，无 agent_id 校验

### 跨轮次数据流追踪

```
轮次 1:
  SubAgentEnd(sa1) → frozen_subagent_vms.push(frozen1)
  SubAgentEnd(sa2) → frozen_subagent_vms.push(frozen2)
  done()            → frozen_subagent_vms 未清空 → [frozen1, frozen2]
  build_tail_vms    → merge: reconciled[0]←frozen1, reconciled[1]←frozen2 ✅

轮次 2:
  SubAgentEnd(sa3) → frozen_subagent_vms.push(frozen3) → [frozen1, frozen2, frozen3]
  done()           → frozen_subagent_vms 未清空 → [frozen1, frozen2, frozen3]
  build_tail_vms   → reconciled 有 1 个 sa3 的 SubAgentGroup placeholder
                    → merge: reconciled[0]←frozen1  ❌ 位置匹配到了轮次 1 的数据！
                    → sa3 的卡片显示的是 sa1 的 agent_id/task_preview
```

同时，轮次 1 的批次卡片（由 `aggregate_batch_groups` 合并 frozen1+frozen2 产生）在 `apply_pipeline_action` 的 RebuildAll 中因 `prefix_len` 保护而留在 view_messages 前缀中。轮次 2 的独立卡片（内容已被 frozen1 污染）追加到尾部。界面中出现两份轮次 1 的 SubAgent 内容。

### 根因总结

1. **frozen_subagent_vms 跨轮次累积**：在 `done()` / `begin_round()` 中未清空，只有 `clear()`（全量重置）才清空
2. **merge_frozen_subagents 按位置匹配而非按 agent_id 匹配**：`frozen_vms[frozen_idx]` 从 0 开始递增，新旧数据混用
3. **RebuildAll 的 prefix_len 设计**：轮次 1 的批次留在前缀中不被替换，导致旧数据可见

## 期望行为

- 每轮 Done 后 `frozen_subagent_vms` 应清空，或 `merge_frozen_subagents` 应按 agent_id 匹配而非位置匹配
- 每轮 SubAgentGroup 应显示本轮的 agent_id/task_preview，不出现跨轮次数据污染
- 前一轮的批次卡片在当前轮次 RebuildAll 后不应再可见（或应正确更新为当前轮次的内容）

## 修复方案

在 `begin_round()` 中调用 `self.frozen_subagent_vms.clear()`，确保每轮开始时清空上一轮的冻结 VMs。

**修改文件**：
- `peri-tui/src/app/message_pipeline.rs:653` —— `begin_round()` 新增 `self.frozen_subagent_vms.clear()`
- `peri-tui/src/app/message_pipeline_test.rs` —— 新增 2 个测试：
  - `test_frozen_subagent_vms_cleared_on_begin_round`：验证跨轮次清空行为
  - `test_merge_frozen_subagents_empty_is_noop`：验证空 frozen_vms 的 noop 语义
