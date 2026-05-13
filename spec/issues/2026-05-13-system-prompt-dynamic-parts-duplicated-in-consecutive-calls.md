# System prompt 动态部分在连续 API 调用中被重复注入，导致 Prompt Cache 命中率骤降

**状态**：Open
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

在同一个 executor 执行的 ReAct 循环中，连续两次 LLM API 调用的 system prompt 出现动态部分（Deferred Tools、Skills、CLAUDE.md）被完整重复追加的现象。第二次调用的 system prompt = 第一次的完整内容 + 动态部分的精确副本（22,595 字符），导致 prompt cache 命中率从 99.998% 骤降至 21.5%，浪费约 85,255 tokens。

## 症状详情

### 日志证据

来源：ZAI 代理日志（同一会话，同一 executor 执行）

| 指标 | Log 1 (dbd9d6ff) | Log 2 (6d71787b) |
|------|-------------------|-------------------|
| 时间戳 | 14:58:14 UTC | 15:01:17 UTC（+3 分钟） |
| 模型 | glm-5-turbo | glm-5-turbo |
| stop_reason | length（输出截断） | tool_calls |
| prompt_tokens | 89,529 | 108,672 |
| cached_tokens | 89,527（99.998%） | 23,417（21.5%） |
| completion_tokens | 4,096 | 663 |
| system prompt 长度 | 34,016 字符 | 56,611 字符 |
| input_history 消息数 | 96（1 system + 1 user + 36 assistant + 58 tool） | 101（1 system + 5 user + 37 assistant + 58 tool） |

### 重复模式

sys2 的前 34,016 字符与 sys1 **完全一致**，之后多出 22,595 字符。多出的内容精确匹配 sys1 中从 `## Deferred Tools` 开始的动态部分：

```
sys1 = [static prompt (01-06) + boundary + dynamic sections (07_env)]
       + "\n\n" + [Deferred Tools 描述]
       + "\n\n" + [其他 middleware 注入]
       + "\n\n" + [Skills 摘要]
       + "\n\n" + [CLAUDE.md 内容]

sys2 = sys1
       + "\n\n" + [Deferred Tools 描述]     ← 精确副本
       + "\n\n" + [其他 middleware 注入]     ← 精确副本
       + "\n\n" + [Skills 摘要]              ← 精确副本
       + "\n\n" + [CLAUDE.md 内容]           ← 精确副本
```

验证：`clean_extra == clean_dynamic` 返回 `True`（去除前导换行后字节级一致）。

### 消息流上下文

两次调用属于**同一个 executor 执行**（同一 ReAct 循环的不同 step）：

1. **Step 0（Log 1）**：Agent 发出 3 个并行后台 agent，LLM 响应被截断（stop_reason=length, completion_tokens=4096），包含 1 个 tool_call（Write）
2. **Step 0 → Step 1 之间**：后台任务完成，3 条 user 通知到达（bg-57cb3/bg-53337/bg-50d9a），用户消息"完成了吧"
3. **Step 1（Log 2）**：Agent 看到后台结果，继续处理。system prompt 多出 22,595 字符副本

### 缓存影响

- Log 1：89,527/89,529 = 99.998% 命中（几乎全部命中上一轮的前缀）
- Log 2：23,417/108,672 = 21.5% 命中（只有 static prompt 部分命中，22,595 字符重复内容 + 新增消息全部 miss）
- 净损失：85,255 tokens 未命中缓存

## 根因分析

### 已排除的路径

| 假设 | 排除理由 |
|------|----------|
| middleware `before_agent` 被调用两次 | `before_agent` 只在 `execute()` 开始时调用一次（`executor/mod.rs:235`），ReAct 循环内不重复调用 |
| `agent_state_messages` 包含旧 System 消息 | `StateSnapshot` 范围 `messages[last_message_count..]` 正确排除了 prepend 的 System 消息（`last_message_count` 在 `before_agent` 之前设置） |
| middleware 用 `add_message` 注入 System | grep 确认所有 middleware 只用 `prepend_message(BaseMessage::system(...))` 或 `add_message` 非System 类型 |
| compact 路径写入 System 消息 | 消息数量 96→101（增加 5 条），不是 compact（compact 会大幅减少消息数） |
| thread 恢复路径 | 同一 executor 执行内不重新加载 thread |

### `handle_compact_done` 的潜在泄露路径

`agent_compact.rs:61-77`：compact 完成后 `agent_state_messages = new_messages`（全为 `BaseMessage::system()` 类型）。如果 compact 后触发 resubmit，下一轮 `history = agent_state_messages` 包含 System 消息，`before_agent` 又 prepend 新的 System 消息，OpenAI adapter 的 `from_base_messages()` 收集**所有** System 消息并 `join("\n\n")`，产生重复。

此路径**已确认可导致重复**，但本日志场景不是 compact 触发。需要排查是否有其他类似的 System 消息泄露路径。

### 待确认的假设

1. **`state.messages()` 在 step 之间被意外修改**：虽然代码审查未发现 step 之间添加 System 消息的路径，但日志数据明确显示 `from_base_messages()` 收集到了额外的 System 消息。可能存在一个未被代码审查覆盖的路径（如某个 middleware 的 `after_tool` 钩子、或 `run_on_error` 路径）
2. **OpenAI adapter 的 `from_base_messages()` 被调用了两次**：如果 LLM 调用失败后重试（`RetryableLLM`），第二次调用时 `state.messages()` 可能已包含上次的 System 消息
3. **`drain_notifications` 或 `emit_snapshot_and_drain_notifications` 触发了 `run_on_error`**：如果 `drain_notifications` 中的 `add_message` 触发了某种副作用

### 关键代码位置

`from_base_messages()` 的 System 消息合并逻辑（`openai.rs:188-267`）：

```rust
for msg in messages.iter() {
    match msg {
        BaseMessage::System { content, .. } => {
            system_parts.push(content.text_content());
        }
        // ...
    }
}
let system_text = system_parts.join("\n\n")
    .replace("__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__", "");
result.insert(0, json!({ "role": "system", "content": system_text }));
```

此函数**无条件收集所有 System 消息**并合并为一条。如果 `state.messages()` 中存在任何额外的 System 消息，它们会被追加到 system prompt 末尾，破坏缓存前缀。

## 复现条件

- **复现频率**：偶发（基于日志分析确认）
- **触发步骤**：
  1. 向 Agent 提交一个会触发后台 Agent 的任务（如并行调查、多方向 code review）
  2. Agent 调度 3+ 个后台 Agent，首个 LLM 响应被截断（stop_reason=length）
  3. 等待后台任务完成后 Agent 继续处理
  4. 检查第二次 LLM 调用的 system prompt 是否包含重复的动态内容
- **环境**：Provider: ZAI（OpenAI 兼容），Model: glm-5-turbo，日志时间 2026-05-13 22:58-23:01 CST

## 建议排查方向

1. **在 `from_base_messages()` 入口添加 debug 日志**：记录 `system_parts` 的数量和每条的长度，确认额外的 System 消息来源
2. **检查 `RetryableLLM` 的重试路径**：重试时 `state.messages()` 是否包含上一次尝试产生的 System 消息
3. **检查 `run_on_error` 链中是否有 middleware 注入 System 消息**：当 `stop_reason=length` 时，executor 的后续处理可能触发了 on_error 钩子

## 相关代码

- `rust-create-agent/src/llm/openai.rs:188-267` — `from_base_messages()` System 消息合并
- `rust-create-agent/src/agent/executor/mod.rs:182-239` — execute 流程（add_message → last_message_count → before_agent → prepend）
- `rust-create-agent/src/agent/executor/final_answer.rs:42-56` — `emit_snapshot_and_drain_notifications`
- `rust-create-agent/src/agent/executor/llm_step.rs:18-123` — `call_llm` 及其错误处理
- `rust-create-agent/src/agent/state.rs:142-163` — `add_message`/`prepend_message`
- `rust-agent-tui/src/app/agent_compact.rs:61-77` — compact 路径的 System 消息写入（已确认可导致重复，但非本日志场景）
- `rust-agent-middlewares/src/tool_search/middleware.rs:53-82` — Deferred Tools prepend
- `rust-agent-middlewares/src/skills/mod.rs:165-182` — Skills prepend
- `rust-agent-middlewares/src/agents_md.rs:144-233` — CLAUDE.md prepend

## 关联 Issue

- `spec/issues/2026-05-13-system-prompt-dynamic-cache-invalidation.md`（Fixed）— 动态内容导致 Anthropic 缓存失效（边界标记修复），本 issue 是不同维度的重复问题
- `spec/issues/2026-05-13-input-history-message-duplication-after-background-tasks.md`（Fixed）— 后台任务消息重复（Human 消息双写），本 issue 是 System 消息重复
- `spec/issues/2026-05-13-prompt-cache-hit-rate-risks.md` — 缓存命中率风险报告，H4（micro-compact 修改 cache 断点前的消息）仍 Open
