> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-compact-ephemeral-notes-not-cleared.md

# Compact 完成后残留 "正在压缩上下文…" 系统通知

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-12

## 修复记录

1. **删除 "正在压缩上下文…" SystemNote**（`thread_ops.rs`）：移除 compact 开始时的 `push_system_note("正在压缩上下文…")`，spinner 状态已有 "压缩上下文" 动词提示，无需额外系统消息。
2. **简化中断提示**（`agent_ops.rs`）：将工具调用中断时的冗长提示 `"⚠ 已中断（工具调用已以 error 结尾，消息已保存，可继续发送恢复）"` 简化为 `"⚠ 已中断"`，与其他中断提示保持一致。
3. **清除 ephemeral_notes**（`agent_compact.rs`）：在 `handle_compact_done` 触发 `RebuildAll { prefix_len: 0 }` 前先 `ephemeral_notes.clear()`，防止 CacheWarning、中断提示、Error 通知等旧的 `ephemeral_notes` 被 `saved_notes` 机制保留并重新插入到 compact 后的消息流中。

## 问题描述

Compact 完成后，compact 开始时添加的 `SystemNote`（"正在压缩上下文…"）以及可能存在的 `CacheWarning`（"⚠ Prompt cache 命中率 X% < 80%"）没有被清除，残留在消息流中。

## 症状详情

Compact 后消息流顶部显示：

```
✻ 上下文已压缩

· ⚠ Prompt cache 命中率 0% < 80%

· 正在压缩上下文…
```

这些是 compact 前添加的 `ephemeral_notes`，compact 完成后的 `RebuildAll { prefix_len: 0 }` 将所有锚点 >= 0 的 notes 全部保留。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 进行较长的对话，触发 auto-compact 或手动 `/compact`
  2. Compact 完成后查看消息流顶部

## 相关代码

- `peri-tui/src/app/thread_ops.rs:365` — `push_system_note("正在压缩上下文…")` 在 compact 开始时添加
- `peri-tui/src/app/agent_ops.rs:168-170` — `CacheWarning` 通过 `AddMessage` 添加为 `ephemeral_notes`
- `peri-tui/src/app/agent_compact.rs:92-102` — `handle_compact_done` 用 `prefix_len: 0` 的 `RebuildAll` 重建，所有 `ephemeral_notes` 被保留
- `peri-tui/src/app/agent_render.rs:80-86` — `apply_pipeline_action` 中 `RebuildAll` 的 ephemeral_notes 过滤逻辑

## 期望改进方向

Compact 完成后，应清除所有 compact 前的 `ephemeral_notes`，只保留 compact 过程中新增的通知。"正在压缩上下文…" 作为临时状态提示，compact 完成后无保留价值。
