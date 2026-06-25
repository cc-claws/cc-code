> 归档于 2026-05-27，原路径 spec/issues/2026-05-26-skillpreload-anthropic-400-tool-result-orphan.md

# SkillPreload 触发 Anthropic 400 Bad Request：tool_result 缺少配对 tool_use

**状态**：Fixed
**优先级**：高
**类型**：Bug
**创建日期**：2026-05-26
**修复日期**：2026-05-26

## 问题描述

用户在对话中输入 `/skill-name` 触发 SkillPreloadMiddleware 预加载 skill 全文时，Anthropic API 返回 400 错误：`tool_result` 中的 `tool_use_id` 在前一条 assistant 消息中找不到对应的 `tool_use` block。Agent 因此停止执行，显示错误。

## 症状详情

| 维度 | 表现 |
|------|------|
| 触发方式 | 用户输入 `/skill-name` 触发 skill preload |
| 错误信息 | `LLM HTTP 错误 (400): API 错误 400 Bad Request: unexpected 'messages.0.content.0: tool_use_id' found in 'tool_result' blocks: call_01_BBirgJPy2oKL0QuOrSf14156. Each 'tool_result' block must have a corresponding 'tool_use' block in the previous message.` |
| Provider | Anthropic 原生 API |
| Agent 行为 | 停止执行，显示 Agent Error |
| 复现频率 | 多轮对话后出现 |

### 用户可见输出

```
✗ Agent Error
  ⎿ LLM HTTP 错误 (400): API 错误 400 Bad Request: unexpected `messages.0.content.0: tool_use_id` found in `tool_result` blocks: call_01_BBirgJPy2oKL0QuOrSf14156. Each `tool_result` block must have a corresponding `tool_use` block in the previous message.
```

## 复现条件

- **复现频率**：多轮对话后出现（非首轮）
- **触发步骤**：
  1. 启动 TUI，使用 Anthropic 原生 API
  2. 进行多轮正常对话
  3. 在后续轮次中输入 `/skill-name` 触发 skill preload
  4. SkillPreloadMiddleware 创建 Read tool 读取 skill 文件内容
  5. 发送请求到 Anthropic API 时返回 400
- **环境**：Anthropic 原生 API，所有 OS

## 涉及文件

- `peri-agent/src/agent/executor/mod.rs` —— `prepended_ids` 计算逻辑（根因所在）
- `peri-middlewares/src/subagent/skill_preload.rs` —— SkillPreloadMiddleware，用 `add_message` 注入 Ai+Tool 消息

## 修复

**根因**：`executor::execute()` 中 `prepended_ids` 的计算逻辑错误。旧逻辑用 `state.messages().len() - len_before_prepend` 计算 `prepended_count`，然后取头部 `prepended_count` 条消息的 ID。但 `before_agent` 期间 SkillPreloadMiddleware 用 `add_message`（尾部追加）注入了 Ai[ToolUse]+Tool[ToolResult] 消息，这些消息也被计入 count，导致从头部多取了等量的 ID。`cleanup_prepended` 误删了头部的原始消息（包括配对的 Ai 消息），留下孤立的 Tool[ToolResult]，触发 Anthropic 400。

**修复**：改用 `take_while(|m| m.is_system())` 收集头部连续 System 消息。所有 `prepend_message` 注入的都是 System 消息且集中在头部；`add_message` 注入的非 System 消息在尾部，不受影响。
