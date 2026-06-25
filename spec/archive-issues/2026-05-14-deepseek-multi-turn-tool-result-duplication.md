> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-deepseek-multi-turn-tool-result-duplication.md

# DeepSeek API 400: 多轮对话中 tool_result 消息重复导致 invalid_request_error

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-14
**修复日期**：2026-05-15

## 问题描述

使用 DeepSeek Anthropic 兼容端口（`api.deepseek.com`）+ `deepseek-v4-pro` 模型的多轮对话中，第三轮 API 请求被拒绝，返回 400 错误：`"Each tool_result block must have a corresponding tool_use block in the previous message."`。

根因是 `agent_state_messages` 中出现了重复消息——上一轮的 tool results 和 assistant 回复被 StateSnapshot 机制重复追加到消息历史中。

## 症状详情

### 现象（2026-05-14 11:44）

请求数据路径：`data/2026-05-14_11-44-30-941_0065/`

**对话流程**：
1. 用户：`/issue-create kk`
2. 用户：`提交暂存区`
3. 用户：`提交他们` ← **本轮 API 请求失败**

**请求消息序列**（Anthropic 格式，共 11 条）：

| 索引 | role | 内容 | 状态 |
|------|------|------|------|
| [0] | user | text "/issue-create kk" | 正常 |
| [1] | assistant | thinking(空) + tool_use Read(SKILL.md) | 正常 |
| [2] | user | tool_result SKILL.md | 正常 |
| [3] | assistant | thinking + text "kk是什么" + tool_use Grep(call_00) + tool_use Glob(call_01) | 正常 |
| [4] | user | tool_result(call_01) + tool_result(call_00) | 正常 |
| [5] | assistant | thinking + text "代码库里没有找到..." | 正常 |
| **[6]** | **user** | **tool_result(call_01) + tool_result(call_00)** | **重复** |
| **[7]** | **assistant** | **thinking + text**（同 [5]） | **重复** |
| [8] | user | text "提交暂存区" | 正常 |
| [9] | assistant | thinking + text "好的，你遇到了..." | 正常 |
| [10] | user | text "提交他们" | 本轮输入 |

消息 [6]-[7] 是 [4]-[5] 的完整重复，插入在旧轮次和当前轮次之间。

**DeepSeek 报错**：
```
messages.6.content.0: tool_use_id found in tool_result blocks: call_01_XgIR0IH3Vvd6suiGazPM8063.
Each tool_result block must have a corresponding tool_use block in the previous message.
```

Assistant 消息 [5] 无 tool_calls，但 user 消息 [6] 却有 tool_result blocks，违反 API 规范。

### 对比：前一请求正常

请求 `0064`（"提交暂存区"）的消息序列只有 7 条，无重复，正常完成：

```
[0] user → [1] assistant(Read) → [2] user(tool_result) → [3] assistant(Grep+Glob) → [4] user(tool_result) → [5] assistant(text) → [6] user("提交暂存区")
```

## 根因分析

### 触发链路

1. `agent_state_messages` 通过 `StateSnapshot` 事件**增量扩展**（`agent_ops.rs:814` 的 `.extend(msgs)`）
2. `execute()` 在设置 `last_message_count` 之后执行 `prepend_message(System)`（`mod.rs:239`），`insert(0)` 右移所有消息，**`last_message_count` 索引失效**
3. 失效的索引使 `StateSnapshot` 捕获范围扩大，包含了本轮之前已在 state 中的旧消息
4. 这些旧消息被重复 `.extend()` 到 `agent_state_messages`
5. 下一轮 `execute()` 以 `agent_state_messages` 初始化 state → 旧消息在 state 中出现两次
6. Anthropic adapter 将重复的 `Tool` → `Assistant` → `Tool` → `Assistant` 序列化为合法的 Anthropic 消息流
7. 但 DeepSeek API 拒绝这种不符合规范的序列

### 代码定位

**索引设置**（时间点 A）：
`peri-agent/src/agent/executor/mod.rs:186`
```rust
let mut last_message_count: usize = state.messages().len();
```

**索引破坏**（时间点 B）：
`peri-agent/src/agent/executor/mod.rs:238-239`
```rust
if let Some(ref prompt) = self.system_prompt {
    state.prepend_message(BaseMessage::system(prompt.clone()));
}
```

`prepend_message` 实现在 `state.rs:161-163`：
```rust
fn prepend_message(&mut self, message: BaseMessage) {
    self.messages.insert(0, message);  // ← 所有元素右移 1
}
```

**受害者**：
- `mod.rs:260-264` — `emit_snapshot_and_drain_notifications` 中的 `state.messages()[*last_message_count..]`
- `mod.rs:102` — `handle_final_answer` 中的 `state.messages()[*last_message_count..]`
- `mod.rs:139` — `run_after_agent` 后的 `state.messages()[*last_message_count..]`

当 `prepend_message` 在 `before_agent` 之前、`last_message_count` 之后执行时，StateSnapshot 会捕获**包含所有旧消息在内的超集**。

**增量扩展**：
`peri-tui/src/app/agent_ops.rs:811-814`
```rust
self.session_mgr.sessions[self.session_mgr.active]
    .agent
    .agent_state_messages
    .extend(msgs.clone());
```

### 为什么 0064 正常但 0065 失败

- 第一轮 execute（0064 前）：state 为新创建，`last_message_count` 指向 Human 消息之后，变体涉及范围小，未产生可观察的重复
- 第二轮 execute（0064）：`agent_state_messages` 包含完整的第一轮历史（7 条消息），prepend 后的偏移使 StateSnapshot 多捕获了第一轮的最终 assistant 消息，但此消息在 `messages_to_anthropic` 中被合并到 user 消息数组中不显眼
- 第三轮 execute（0065）：`agent_state_messages` 已积累了两轮的重复 → 出现 tool_results + assistant 的整段重复

### 与已知 TRAP 的关系

CLAUDE.md 中已记录此问题：
> **[TRAP]** `prepend_message` 的 `insert(0)` 右移导致 StateSnapshot 快照范围扩大，泄露 System 消息到 `agent_state_messages`。

本 issue 是该 TRAP 的另一种表现形式：不仅泄露 System 消息，还**重复捕获旧轮次消息**。已有 issue `#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls` 记录了 System 重复问题，本 issue 聚焦消息级别的重复问题。

## 复现条件

- **复现频率**：多轮对话中必现（3 轮以上稳定触发）
- **触发步骤**：
  1. 使用 DeepSeek Anthropic 兼容端口 + `deepseek-v4-pro` + thinking 模式
  2. 第一轮：`/skill 命令` 触发 SkillPreloadMiddleware 注入后正常完成
  3. 第二轮：普通文本输入，正常完成
  4. 第三轮：再次输入 → API 请求中包含上一轮消息的重复
- **环境**：DeepSeek Anthropic 兼容 API，多轮对话，`prepend_message` 活跃

## 修复方向

**方案 A（推荐）**：在 `prepend` 之后更新 `last_message_count`，补偿偏移量。
```rust
// mod.rs:239 之后
state.prepend_message(BaseMessage::system(prompt.clone()));
last_message_count += 1;  // 补偿 insert(0) 的右移
```

**方案 B**：将 `last_message_count` 改为基于消息 ID 的标记，而非数组索引。

**方案 C**：将 `prepend_message` 移到 `last_message_count` 设置之前执行，避免事后补偿。

**方案 A** 最简，但需确认 `before_agent` 中间件也可能产生 `add_message` 调用，这些调用不影响索引偏移（`add_message` 是尾部追加）。如果中间件调用了 `prepend_message`，同样需要补偿。

## 涉及文件

- `peri-agent/src/agent/executor/mod.rs:186` — `last_message_count` 设置
- `peri-agent/src/agent/executor/mod.rs:238-239` — `prepend_message` 破坏索引
- `peri-agent/src/agent/state.rs:161-163` — `prepend_message` 实现
- `peri-tui/src/app/agent_ops.rs:811-814` — `agent_state_messages.extend()` 增量累积
- `peri-agent/src/agent/executor/final_answer.rs:102-110` — `handle_final_answer` 中的快照

## 关联 Issue

- `spec/global/domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls` — System 消息重复（同根因的不同表现）
- `spec/issues/2026-05-14-deepseek-anthropic-thinking-block-dropped.md`（Open）— 同 session 中的另一个 DeepSeek thinking 问题
