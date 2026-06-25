> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-subagent-skip-assistant-bubble.md

# SubAgent 内部跳过 AssistantBubble 渲染

**状态**：Fixed + Verify
**优先级**：低
**创建日期**：2026-05-12
**修复日期**：2026-05-12

## 问题描述

SubAgent 展开时，内部 recent_messages 会渲染 AssistantBubble，但当 AssistantBubble 只有 Reasoning/ToolUse 块而无实际文本时，会产生 `●` 前缀后无内容的空白行。用户希望 SubAgent 内部不渲染 AssistantBubble，只显示工具调用。

## 症状详情

### 当前显示

```
❯ Agent(hello-agent)
  say hello
●
  ⎿ Hello! 👋 How can I help you today?
```

AssistantBubble 只有一个 `●` 前缀，后面没有实际内容（因为只有 Reasoning 或 ToolUse 块），导致出现空白行。

### 期望显示

```
❯ Agent(hello-agent)
  say hello
  ⎿ Hello! 👋 How can I help you today?
```

跳过 AssistantBubble，直接显示工具调用结果。

## 相关代码

- `peri-tui/src/ui/message_render.rs:398-409` — SubAgentGroup 展开状态的 recent_messages 渲染循环

  ```rust
  for inner_vm in iter_messages.iter() {
      let inner_lines = render_view_model(inner_vm, None, _width);
      if inner_lines.is_empty() {
          continue;
      }
      for line in inner_lines {
          let mut new_spans = vec![Span::styled("  ", bg_style)];
          new_spans.extend(line.spans.into_iter().map(|s| s.patch_style(bg_style)));
          lines.push(Line::from(new_spans));
      }
  }
  ```

- `peri-tui/src/ui/message_render.rs:110-194` — AssistantBubble 渲染逻辑

## 期望改进方向

在 SubAgent 内部的 recent_messages 渲染循环中，跳过 `MessageViewModel::AssistantBubble`，只渲染：

- `ToolBlock` — 工具调用及结果
- `ToolCallGroup` — 聚合的只读工具

## 影响范围

- SubAgent 展开状态下的内部消息显示
- 不影响功能，仅影响显示美观度
