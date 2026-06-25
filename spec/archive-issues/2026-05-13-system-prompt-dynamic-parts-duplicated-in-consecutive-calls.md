> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls.md
# System prompt 动态部分在连续 API 调用中被重复注入，导致 Prompt Cache 命中率骤降

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13
**修复日期**：2026-05-14

## 问题描述

`StateSnapshot` 的范围计算在 `prepend_message` 的 `insert(0)` 右移效应下，将 middleware prepended 的 System 消息泄露到 `agent_state_messages`。下一轮执行时，旧 System 消息与新 prepend 的 System 消息被 OpenAI adapter 的 `from_base_messages()` 一并收集并 `join("

")`，导致动态部分（Deferred Tools、Skills、CLAUDE.md）重复出现。Prompt cache 命中率从 99.998% 骤降至 21.5%，浪费约 85,255 tokens。

## 根因（已确认）

### `prepend_message` 的 `insert(0)` 右移导致 StateSnapshot 包含 System 消息

`emit_snapshot_and_drain_notifications`（`final_answer.rs:48`）使用 `state.messages()[last_message_count..]` 计算快照范围。`last_message_count` 在 `add_message(human)` 后立即设置（`executor/mod.rs:186`），但 `before_agent` 链随后通过 `prepend_message` 即 `insert(0, msg)` 注入 3-4 条 System 消息。`insert(0, ...)` 将所有已有元素**右移**，导致 `messages[last_message_count..]` 包含被右移到该范围内的 System 消息。

### 具体数值追踪（空 history 场景）

```
Step 1: messages = []                                          len=0
Step 2: add_message(human)  →  messages = [human]              len=1
Step 3: last_message_count = 1
Step 4: before_agent prepend System(deferred)  insert(0)  →  [SysD, human]           len=2
Step 5: before_agent prepend System(skills)    insert(0)  →  [SysS, SysD, human]      len=3
Step 6: before_agent prepend System(claude_md) insert(0)  →  [SysC, SysS, SysD, human] len=4
Step 7: with_system_prompt prepend System(prompt) insert(0) → [SysP, SysC, SysS, SysD, human] len=5
```

快照范围：`messages[last_message_count..]` = `messages[1..]` = **[SysC, SysS, SysD, human, ...]**

**泄露了 3 条 System 消息**（SysC、SysS、SysD）。System(prompt) 在 index 0，被 `last_message_count=1` 跳过。

### 泄露链

1. 第一轮 StateSnapshot 将 [SysC, SysS, SysD, human, ai_1, tool_1, ...] extend 到 `agent_state_messages`
2. `agent_state_messages` 包含 3 条旧 System 消息
3. 第二轮 `history = agent_state_messages.clone()`，AgentState 以此初始化
4. `before_agent` prepend 新的 4 条 System 消息
5. `from_base_messages()` 收集所有 System 变体：4 新 + 3 旧 = 7 条
6. `join("

")` 合并：`[SysP + SysC + SysS + SysD]` + `[SysC + SysS + SysD]` = sys1 + 动态部分副本

### 数据验证

| 指标 | 计算 | 实际值 |
|------|------|--------|
| sys1 长度 | 34,016 | 34,016 ✓ |
| extra 长度 = 泄露的 3 条 System join | 22,595 | 22,595 ✓ |
| sys2 长度 = sys1 + extra | 34,016 + 22,595 = 56,611 | 56,610 ✓ (±1 因换行) |
| 泄露条数 = prepend_count - last_message_count | 4 - 1 = 3 | 3 ✓ |
| extra 内容 = sys1 中 Deferred Tools 到末尾 | 精确匹配 | `clean_extra == clean_dynamic` ✓ |

### 触发条件

`last_message_count <= prepend_count`（中间件 prepend 的 System 消息数 ≥ last_message_count 时触发）：

| 场景 | last_message_count | prepend_count | 泄露条数 |
|------|-------------------|---------------|----------|
| 新会话首条消息（空 history） | 1 | 4 | 3 |
| compact 后 resubmit | 1+ | 4 | 3+ |
| 正常多轮对话（history ≥ 4 条） | ≥5 | 4 | 0（不触发） |

## 症状详情

### 日志证据

来源：ZAI 代理日志（同一会话，两次 executor 执行）

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

Log 1 和 Log 2 的前 95 条非 System 消息**完全一致**（逐条 byte 级对比 0 不匹配）。Log 2 多出 5 条消息：1 条 assistant + 3 条 user（后台任务通知）+ 1 条 user（"完成了吧"）。

### 重复模式

sys2 的前 34,016 字符与 sys1 **完全一致**，之后多出 22,595 字符。多出的内容精确匹配 sys1 中从 `## Deferred Tools` 开始的动态部分：

```
sys1 = [static prompt (01-06) + boundary + dynamic sections (07_env)]
       + "

" + [Deferred Tools 描述]
       + "

" + [Skills 摘要]
       + "

" + [CLAUDE.md 内容]

sys2 = sys1
       + "

" + [Deferred Tools 描述]     ← 泄露的旧 System 消息
       + "

" + [Skills 摘要]              ← 泄露的旧 System 消息
       + "

" + [CLAUDE.md 内容]           ← 泄露的旧 System 消息
```

### 缓存影响

- Log 1：89,527/89,529 = 99.998% 命中
- Log 2：23,417/108,672 = 21.5% 命中（只有 static prompt 部分命中，重复的 22,595 字符 + 新增消息全部 miss）
- 净损失：85,255 tokens 未命中缓存

### 消息流上下文

Log 1 和 Log 2 属于**两次独立的 executor 执行**（非同一 ReAct 循环的两个 step）：

1. **Run 1（Log 1）**：Agent 执行 ReAct 循环，首条消息（空 history），`last_message_count=1`，StateSnapshot 泄露 3 条 System 消息到 `agent_state_messages`。LLM 响应被截断（stop_reason=length），executor 通过 `handle_final_answer` 结束
2. **Run 1 → Run 2 之间**：3 个后台 agent 完成，`handle_background_task_completed` 向 `agent_state_messages` 追加 3 条 Human 通知；用户输入"完成了吧"
3. **Run 2（Log 2）**：`history = agent_state_messages`（包含 3 条泄露的 System 消息），`before_agent` prepend 4 条新 System 消息，`from_base_messages()` 收集 7 条 System → 重复

## 补充：`handle_compact_done` 的独立泄露路径

`agent_compact.rs:61-77`：compact 完成后 `agent_state_messages = new_messages`（全为 `BaseMessage::system()` 类型）。如果 compact 后触发 resubmit，下一轮 `history` 包含 System 消息，`before_agent` 又 prepend 新的 System 消息，产生同样的重复。此路径是**独立的泄露来源**（不依赖 `insert(0)` 右移效应，直接将 System 消息写入 `agent_state_messages`）。

## 复现条件

- **复现频率**：必现（新会话首条消息或 compact 后 resubmit）
- **触发步骤**：
  1. 启动新会话（空 history）
  2. 发送任意消息触发 agent 执行
  3. 等待 agent 完成后发送第二条消息
  4. 检查第二条消息对应 API 调用的 system prompt 是否包含重复的动态内容
- **环境**：所有 OpenAI 兼容 provider（通过 `from_base_messages` 的 System 消息合并机制触发）

## 相关代码

- `peri-agent/src/agent/executor/mod.rs:186` — `last_message_count` 在 `before_agent` 之前设置
- `peri-agent/src/agent/executor/final_answer.rs:48` — `state.messages()[*last_message_count..]` 快照范围包含被右移的 System 消息
- `peri-agent/src/agent/state.rs:161-163` — `prepend_message` 使用 `insert(0, msg)` 右移所有元素
- `peri-agent/src/llm/openai.rs:192-267` — `from_base_messages()` 无条件收集所有 System 消息并 `join("

")`
- `peri-tui/src/app/agent_ops.rs:811-814` — `agent_state_messages.extend(msgs)` 接收含 System 的快照
- `peri-tui/src/app/agent_submit.rs:259-262` — `history = agent_state_messages.clone()` 传入含 System 的历史
- `peri-tui/src/app/agent_compact.rs:61-77` — compact 路径直接写入 System 消息到 `agent_state_messages`

## 关联 Issue

- `spec/issues/2026-05-13-system-prompt-dynamic-cache-invalidation.md`（Fixed）— 动态内容导致 Anthropic 缓存失效（边界标记修复），本 issue 是不同根因的重复问题
- `spec/issues/2026-05-13-input-history-message-duplication-after-background-tasks.md`（Fixed）— 后台任务 Human 消息重复（双写修复），本 issue 是 System 消息泄露
- `spec/issues/2026-05-13-prompt-cache-hit-rate-risks.md` — 缓存命中率风险报告

## 修复方案

在所有 StateSnapshot 发射点过滤 System 消息，确保 `agent_state_messages` 永远不包含 `BaseMessage::System` 变体。

### 修改文件

| 文件 | 修改点 |
|------|--------|
| `peri-agent/src/agent/executor/final_answer.rs` | `emit_snapshot_and_drain_notifications`、`handle_final_answer` 中 3 处快照添加 `.filter(\|m\| !m.is_system())` |
| `peri-agent/src/agent/executor/mod.rs` | 安全网快照添加 `.filter(\|m\| !m.is_system())` |

### 回归测试

`test_state_snapshot_excludes_system_messages`（`mod_test.rs`）：模拟空 history + `with_system_prompt` + middleware prepend 的完整场景，验证所有 StateSnapshot 均不包含 System 消息。全量 344 测试通过。
