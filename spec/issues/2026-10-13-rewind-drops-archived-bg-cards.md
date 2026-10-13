# /rewind 后归档的后台子 Agent 卡片消失（v0.6.130 引入的回归）

**状态**：Fixed（v0.6.131，PR #469）
**优先级**：高
**创建日期**：2026-10-13
**GitHub Issue**：#468（已关闭）

## 问题描述

执行 `/rewind`（`/undo`）后，历史中已归档的后台子 Agent 分组卡片凭空消失，且没有任何后续事件能把它们放回来——当时底栏抽屉是空的，视觉上就是历史少了几块。

## 根因

v0.6.130 修「后台分组卡片漏回主时间线」时，把剔除逻辑放在 `apply_pipeline_action` 里、对收到的整条 `tail_vms` 生效。而 `/rewind` 用 `RebuildAll { prefix_len: 0, 全量历史 }` 表达「替换整条时间线」，剔除于是连用户正在回看的归档卡片一并删掉。v0.6.130 为该修复新增的 `ChatSession::bg_drawer_seen` 会话状态代理同样会过度剔除历史重放。

判据落在「会话状态代理」上是这类误伤的根源：同一个 `RebuildAll` 动作，活跃轮次尾部必须剔除、全量历史重放必须保留，区别在动作语义，不在会话状态。

## 修复

- 剔除下移到「活跃轮次尾部」的唯一产地 `MessagePipeline::build_rebuild_all`，判据落在动作语义上。
- 删除 `bg_drawer_seen` 字段。
- 回归用例一对，互为反面：`test_bg_group_absent_from_live_rebuild_tail_when_pool_drained`（活跃尾部必须剔除）与 `test_rewind_keeps_archived_bg_cards`（全量重放必须保留）。
