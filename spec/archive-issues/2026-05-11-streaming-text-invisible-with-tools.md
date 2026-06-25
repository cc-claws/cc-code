> 归档于 2026-05-16，原路径 spec/issues/2026-05-11-streaming-text-invisible-with-tools.md

# 流式过程中 AI 文本不可见（工具调用场景）

**状态**：Fixed（待用户验证）
**优先级**：高
**创建日期**：2026-05-11

## 问题描述

当 AI 回复中包含工具调用时，**整轮回复的 AI 文本在流式过程中完全不可见**，只有工具调用块（ToolBlock）正常渲染。文本内容直到 Done 事件后才一次性出现。纯文本回复（无工具调用）流式正常。

## 复现条件

- **必现**：所有场景下稳定复现，不受模型、对话轮次、工具数量影响
- **触发条件**：AI 回复中包含至少 1 个工具调用（Read/Write/Bash 等普通工具）
- **第 1 轮即可复现**，非多轮累积问题
- 单个工具调用和多工具并发调用均可复现

## 症状详情

### 正常行为（纯文本回复，无工具调用）

- AI 文本流式实时显示，正常

### 异常行为（回复中包含工具调用）

| 元素 | 流式过程中 | Done 后 |
|------|-----------|---------|
| AI 文本（工具前） | 不可见 | 正常显示 |
| AI 文本（工具后） | 不可见 | 正常显示 |
| 工具调用块（ToolBlock） | 正常更新 | 正常显示 |
| Reasoning 提示（"Thought for N chars"） | 可见 | 正常显示 |
| AssistantBubble 气泡 | 完全看不到 | 正常显示 |

### 典型场景：文字→工具→文字 混合模式

```
时间线：
1. AI 输出第一段文字        → 不可见（但 reasoning 提示能看到）
2. AI 调用工具（ToolStart） → 工具块正常出现
3. 工具执行中（ToolEnd）    → 工具块正常更新
4. AI 输出第二段文字        → 不可见
5. Done                    → 所有文本突然出现，渲染正确
```

## 关键观察

1. **RebuildAll 机制正常工作**：工具块能流畅响应 ToolStart/ToolEnd 事件，说明 `request_rebuild()` → `RebuildWithAnchor` → 渲染线程的链路畅通
2. **Reasoning 可见但文本不可见**：`ContentBlockView::Reasoning` 正常渲染，但 `ContentBlockView::Text` 内容缺失——说明 AssistantBubble 的构建逻辑本身在执行，但文本内容没有被正确包含
3. **气泡完全不可见**：流式过程中看不到 AssistantBubble 的边框/背景（不像是有气泡但内容为空），但 Done 后气泡和文本同时正确出现
4. **非最近回归**：此问题一直存在，不是近期代码改动引入的回归

## 根因分析

### 断言：核心框架将工具前文本作为 `AiReasoning` 发射，TUI 将其显示为推理提示而非实际文本

**核心框架事件类型**（`peri-agent/src/agent/events.rs`）：

| 事件 | 用途 | 发射位置 |
|------|------|---------|
| `AiReasoning(String)` | 推理/思考内容 | `tool_dispatch.rs:40` |
| `TextChunk { message_id, chunk }` | 最终回答文本 | `final_answer.rs:86` |

注意：核心框架**没有** `AssistantChunk` 事件。

**TUI 事件映射**（`peri-tui/src/app/agent.rs:471-474`）：

| 核心事件 | TUI 事件 | Pipeline 处理 | 显示效果 |
|---------|---------|-------------|---------|
| `AiReasoning(text)` | `AiReasoning(text)` | `push_reasoning()` → `current_ai_reasoning` | `ContentBlockView::Reasoning { char_count }` → "Thought for N chars" |
| `TextChunk { chunk }` | `AssistantChunk(text)` | `push_chunk()` → `current_ai_text` | `ContentBlockView::Text { raw, rendered }` → 实际文本 |

**Bug 所在**（`peri-agent/src/agent/executor/tool_dispatch.rs:40`）：

```rust
agent.emit(AgentEvent::AiReasoning(reasoning.thought.clone()));
```

AI 的工具前文本（如 "Let me read the file"）通过 `AiReasoning` 发射。TUI pipeline 将其存入 `current_ai_reasoning`，`build_streaming_bubble()` 只生成 `ContentBlockView::Reasoning { char_count }`——**只显示字符数，不存储也不显示实际文本内容**。

**对比最终回答路径**（`final_answer.rs:86`）：

```rust
agent.emit(AgentEvent::TextChunk { message_id: ai_msg_id, chunk: answer.clone() });
```

最终回答通过 `TextChunk` 发射 → 映射为 `AssistantChunk` → `push_chunk()` → `current_ai_text` → 正确渲染为 `ContentBlockView::Text`。**纯文本回复正常的原因。**

**Done/StateSnapshot 后恢复正常的原理**：`set_completed()` 清空流式缓冲区，`messages_to_view_models()` 从 `BaseMessage::Ai` 的 content blocks 重建，文本在 `source_message` 中完整保留，因此渲染正确。

### 事件流对比

**工具调用路径（有 bug）**：
```
generate_reasoning() → 返回 Reasoning { thought: "Let me read the file", tool_calls: [...] }
  ↓
dispatch_tools():
  emit AiReasoning("Let me read the file")  ← 文本被当作推理内容
  emit ToolStart { name: "Read", ... }
  emit ToolEnd { output: "file contents" }
  ↓
TUI pipeline:
  push_reasoning("Let me read the file") → current_ai_reasoning = "Let me read the file"
  build_streaming_bubble() → Reasoning { char_count: 19 }  ← 只显示 "Thought for 19 chars"
```

**最终回答路径（正常）**：
```
generate_reasoning() → 返回 Reasoning { final_answer: "Here is the result", tool_calls: [] }
  ↓
handle_final_answer():
  emit TextChunk { chunk: "Here is the result" }  ← 文本作为正式文本
  ↓
TUI pipeline:
  map TextChunk → AssistantChunk("Here is the result")
  push_chunk("Here is the result") → current_ai_text = "Here is the result"
  build_streaming_bubble() → Text { raw: "Here is the result" }  ← 正确显示
```

## 修复方案

### 核心修复（已实施）

将 `tool_dispatch.rs:40` 的 `AiReasoning` 改为 `TextChunk`，使工具前文本走 `AssistantChunk` → `push_chunk()` → `current_ai_text` 路径：

```rust
// 修复前
agent.emit(AgentEvent::AiReasoning(reasoning.thought.clone()));

// 修复后
if !reasoning.thought.trim().is_empty() {
    agent.emit(AgentEvent::TextChunk {
        message_id: ai_msg_id,
        chunk: reasoning.thought.clone(),
    });
}
```

`ai_msg_id` 已在 line 35 捕获（`let ai_msg_id = ai_msg.id();`），可直接使用。

### TUI 层补充修复（未提交）

`message_pipeline.rs` 的未提交修改在 `ToolStart` 时检查 `throttle_armed` 并立即发射 `RebuildAll`，确保工具调用前的流式文本被显示：

```rust
AgentEvent::ToolStart { ... } => {
    // Fire pending throttle before disarming to ensure text streamed before
    // tool call is displayed. This fixes the bug where AI text disappears
    // when tool calls arrive.
    let action = if self.throttle_armed {
        self.throttle_armed = false;
        // Return RebuildAll with the current streaming content
        Some(PipelineAction::RebuildAll {
            prefix_len: self.completed_len_at_round_start,
            tail_vms: self.build_tail_vms(),
        })
    } else {
        self.throttle_armed = false;
        None
    };

    if self.in_subagent() {
        self.subagent_tool_start(&tool_call_id, &name, input);
    } else {
        self.tool_start_internal(&tool_call_id, &name, input);
    }

    if let Some(a) = action {
        vec![a]
    } else {
        vec![PipelineAction::None]
    }
}
```

## 相关代码

- `peri-agent/src/agent/executor/tool_dispatch.rs:40`：**Bug 所在** — `AiReasoning` 发射工具前文本
- `peri-agent/src/agent/executor/final_answer.rs:86`：**对照** — `TextChunk` 发射最终回答
- `peri-tui/src/app/agent.rs:471-474`：事件映射层（`AiReasoning`/`TextChunk` → TUI 事件）
- `peri-tui/src/app/message_pipeline.rs:493-534`：`has_streaming_content()` + `build_streaming_bubble()`
- `peri-tui/src/ui/message_view.rs`：`ContentBlockView::Reasoning` vs `ContentBlockView::Text` 渲染差异

## 修复记录

**改动文件**：`peri-agent/src/agent/executor/tool_dispatch.rs`

**改动内容**：将 `AiReasoning(reasoning.thought.clone())` 替换为 `TextChunk { message_id: ai_msg_id, chunk: reasoning.thought.clone() }`，并增加空文本检查。

**测试结果**：
- `peri-agent`：313 passed, 0 failed
- `peri-tui`：391 passed, 0 failed

**待提交的 TUI 层补充修复**：`peri-tui/src/app/message_pipeline.rs` 的未提交修改

## 待验证

需要用户实际运行 TUI（`cargo run -p peri-tui`）验证以下场景：

1. **工具调用场景**：发送一个会触发工具调用的问题（如 "读取一下当前目录的文件"），确认 AI 的工具前文本在流式过程中可见（不再只显示 "Thought for N chars"）
2. **纯文本场景**：发送一个不需要工具的纯文本问题，确认纯文本回复仍然正常流式显示
3. **混合模式**：发送一个需要多轮工具调用的问题，确认每轮工具调用前后的文本都能正常显示
4. **Reasoning 场景**：如果使用带 reasoning 的模型，确认 "Thought for N chars" 推理提示仍然正常显示（推理内容由 LLM 适配器流式发射的 `AiReasoning` 事件驱动，不受此次修改影响）

## 用户报告问题仍存在的可能原因

1. **未重新编译代码**：如果用户直接运行了旧的二进制文件，修复不会生效。需要使用 `cargo run -p peri-tui` 重新编译
2. **TUI 层补充修复未生效**：`message_pipeline.rs` 的未提交修改可能需要与核心修复一起生效
