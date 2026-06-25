> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-glm-reasoning-field-not-parsed.md

# GLM 模型 reasoning 字段未被解析，thinking 内容跨轮次丢失

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-12

## 问题描述

GLM 系列模型（如 `ZAI/glm-5.1`）通过 OpenAI 兼容 API 返回 thinking 内容时，使用 `reasoning` 顶层字段而非 `reasoning_content`。代码中 `parse_assistant_message` 只检查了 `reasoning_content` 和 content 数组中的 `thinking` 块，完全忽略了 `reasoning` 字段。导致：

1. GLM 模型的 thinking 内容在第一轮 LLM 响应解析时就被丢弃
2. 后续轮次无 thinking 内容可回传，模型无法看到自己的先前推理
3. 多轮会话中 reasoning 始终为空（`ai_with_reasoning=0`）

## 症状详情

日志诊断显示：

```
# GLM 模型 API 原始响应的 JSON key 列表
json_keys=["content", "reasoning", "reasoning_details", "role", "tool_calls"]
has_reasoning_content=false   ← 代码只检查了这个字段
has_reasoning=true            ← GLM 实际用的字段名

reasoning_preview="The user wants me to search the project for ..."
```

StateSnapshot 中所有 AI 消息均无 Reasoning block：

```
ai_count=5  ai_with_reasoning=0
```

### 附带发现：invariant check 误报

消息序列不变量检查逻辑存在误报。当 assistant 发起多个并行 tool_calls（如 `assistant(tc=2)`）时，后续连续的 tool 结果消息中，只有第一个的前一条是 assistant，其余的前一条是 tool 消息。旧逻辑对每个 tool 消息都要求 immediate prev 是 assistant with tool_calls，导致大量虚假 ERROR 日志。

实际合法的消息序列：

```
[2]assistant(tc=2/true)  [3]tool(id=call_6a)  [4]tool(id=call_f8)
```

旧逻辑在 position=4 报错（prev=tool），但这是 OpenAI 格式的合法布局。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 配置 GLM 系列模型（如 `ZAI/glm-5.1`）作为 OpenAI 兼容 provider
  2. 发送需要推理的用户消息
  3. 观察日志：`has_reasoning_key=false`，`ai_with_reasoning=0`
- **环境**：模型 `ZAI/glm-5.1`，OpenAI 兼容接口

## 修复方案

### 1. 解析侧：同时识别 `reasoning` 和 `reasoning_content`

`parse_assistant_message`（`openai.rs`）和 `OpenAiAdapter::to_base_message`（`adapters/openai.rs`）中，对 `reasoning_content` 和 `reasoning` 两个字段用 `or_else` 链式尝试提取：

```rust
let reasoning_text = assistant_msg["reasoning_content"]
    .as_str()
    .or_else(|| assistant_msg["reasoning"].as_str());
```

### 2. 序列化侧：同时回传两个字段

`messages_to_json`（`openai.rs`）和 `OpenAiAdapter::from_base_messages`（`adapters/openai.rs`）中，assistant 消息同时设置 `reasoning_content` 和 `reasoning` 两个顶层字段：

```rust
let rv = json!(reasoning_text.as_deref().unwrap_or(""));
msg["reasoning_content"] = rv.clone();
msg["reasoning"] = rv;
```

### 3. Invariant check 修复

改为按连续 tool 块检查——遍历消息序列，找到连续 tool 消息块时，检查块首前面是否为 `assistant with tool_calls`，而非逐条检查 immediate prev。

## 相关代码

- `peri-agent/src/llm/openai.rs` — LLM 响应解析（`parse_assistant_message`）+ 序列化回传（`messages_to_json`）+ invariant check
- `peri-agent/src/messages/adapters/openai.rs` — 持久化层序列化/反序列化（`from_base_messages` / `to_base_message`）
