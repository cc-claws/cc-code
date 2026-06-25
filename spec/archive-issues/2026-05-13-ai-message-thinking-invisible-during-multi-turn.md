> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-ai-message-thinking-invisible-during-multi-turn.md
# 多轮对话中 AI message 和 thinking 在进行时不可见

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13

已通过重构消息管线完成

## 问题描述

在多轮对话过程中，AI message 和 thinking 内容在执行时不可见，只有在全部结束后才显示。但工具调用的结果在执行时是可见的。

## 症状详情

### 用户观察

| 内容类型 | 进行时可见性 | 结束后可见性 |
|---------|-------------|-------------|
| AI message 文本 | ❌ 不可见 | ✅ 可见 |
| Thinking 内容 | ❌ 不可见 | ✅ 可见 |
| 工具调用结果 | ✅ 可见 | ✅ 可见 |

### 影响范围

- **所有轮次**：从第一轮开始就存在此问题
- **仅影响 AI 内容**：工具调用（Read、Bash 等）的结果正常显示
- **最终一致性**：对话结束后，所有内容都正确显示

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 发起一个需要多轮对话的请求（如需要调用多个工具的复杂任务）
  2. 观察 AI 的回复和 thinking 内容
  3. 注意工具调用结果是否正常显示

## 相关代码

### 核心文件

- `peri-tui/src/app/message_pipeline.rs` — 统一消息渲染管线
  - `build_tail_vms()` — 构建尾部 VMs，决定流式显示内容
  - `build_streaming_bubble()` — 构建流式 AssistantBubble
  - `has_streaming_content()` — 判断是否有流式内容

- `peri-tui/src/ui/message_view.rs` — 视图模型定义
  - `MessageViewModel::AssistantBubble` — AI 消息视图
  - `ContentBlockView::Reasoning` — thinking 内容视图

### 可能原因分析

1. **`has_snapshot_this_round` 判断问题**：

   ```rust
   // message_pipeline.rs:640
   if self.has_snapshot_this_round {
       // reconcile 路径：从 completed 重建
   } else {
       // 无 snapshot：跳过 reconcile
   }
   ```

   如果 `has_snapshot_this_round` 在多轮对话中的状态不正确，可能导致流式内容不被包含在 `tail_vms` 中。

2. **流式内容与 reconcile 的分离**：
   - 工具调用结果通过 `completed_tools` 直接添加到 `tail_vms`
   - AI message 和 thinking 通过 `build_streaming_bubble()` 添加
   - 如果 `has_streaming_content()` 返回 false，流式 bubble 不会被添加

3. **`build_streaming_bubble()` 条件**：

   ```rust
   // message_pipeline.rs:666
   if self.has_streaming_content() {
       tail_vms.push(self.build_streaming_bubble());
   }
   ```

## 相关 Issue

- `2026-05-12-systemnote-position-drift-on-rebuild.md` — 消息位置漂移问题
- `2026-05-12-cache-percentage-disappears-after-done.md` — Done 后缓存百分比消失

## 涉及文件

- `peri-tui/src/app/message_pipeline.rs`
- `peri-tui/src/ui/message_view.rs`
- `peri-tui/src/app/agent_ops.rs` — agent 事件处理
