> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-cache-warning-discarded-by-rebuild.md

# CacheWarning 消息被 RebuildAll 立即丢弃，用户无法看到

**状态**：Fixed + Verify
**优先级**：中
**类型**：Bug
**创建日期**：2026-05-12

## 问题描述

当 prompt cache 累计命中率低于 80% 时，系统会生成 `CacheWarning` VM 通过 `AddMessage` 加入 view_messages。但由于 `CacheWarning` 是 ephemeral 合成 VM（不在 BaseMessage[] 中），下一次 `RebuildAll` 触发时会被 drain 丢弃。由于 agent 运行期间 `RebuildAll` 频繁触发（每个 ToolStart/ToolEnd/StateSnapshot 都会立即触发），`CacheWarning` 几乎在添加瞬间就被清掉，用户在消息流中看不到这条警告。

## 症状详情

**期望行为**：缓存命中率低于阈值时，消息流中应持续显示警告提示（如 "Prompt cache 累计命中率 48% < 80%"）。

**实际行为**：警告一闪而过或完全不可见，被下一次 `RebuildAll` 清除。

**根因**：`apply_pipeline_action` 中 `RebuildAll` 的 `saved_notes` 过滤器只保留 `SystemNote` 变体，不保留 `CacheWarning`：

```rust
// agent_render.rs:71-76
let saved_notes: Vec<MessageViewModel> = session
    .messages
    .view_messages
    .drain(prefix_len..)
    .filter(|vm| matches!(vm, MessageViewModel::SystemNote { .. }))  // ← CacheWarning 不匹配
    .collect();
```

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用支持 prompt cache 的模型（如 Anthropic Claude）
  2. 进行多轮对话使累计缓存命中率低于 80%
  3. 观察 `TokenUsageUpdate` 事件触发后消息流中是否出现缓存警告
- **环境**：任何使用 Anthropic API 的对话

## 修复方向

将 `CacheWarning` 改为使用 `SystemNote` 变体发送，这样 `RebuildAll` 的 `saved_notes` 过滤器会自动保留它，改动最小。

## 相关代码

- `peri-tui/src/app/agent_ops.rs:170` —— 创建 `CacheWarning` VM 并通过 `AddMessage` 添加
- `peri-tui/src/app/agent_render.rs:71-76` —— `saved_notes` 过滤器仅保留 `SystemNote`，丢弃 `CacheWarning`
- `peri-tui/src/ui/message_view.rs:214` —— `CacheWarning` 变体定义
- `peri-tui/src/ui/message_render.rs:441` —— `CacheWarning` 的渲染逻辑
