> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-skill-preload-invalidates-prompt-cache.md

# Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-12

## 问题描述

`SkillPreloadMiddleware` 在 `before_agent` 时通过 `state.prepend_message()` 将 skill 全文以 Human → Ai[ToolUse] → Tool[ToolResult] 消息序列注入到 `state.messages()` 的最前面（index 0）。Anthropic 的 `apply_cache_to_messages()` 将 `cache_control: { type: "ephemeral" }` 放在第一条 user 消息上作为缓存边界。当 preload 激活时，第一条 user 消息从真实的用户输入变成合成的 `"(System: Preloading skill files)"`，导致首轮 LLM 调用的缓存前缀与后续轮次不同，首轮必然 cache miss。

## 症状详情

### 缓存失效机制

```
无 preload 时 messages 数组：
  [0] Human "用户的真实问题"  ← cache_control 放这里，前缀稳定
  [1] Ai ...
  [2] Human "第二轮问题"
  ...

有 preload 时 messages 数组（修复前）：
  [0] Human "(System: Preloading skill files)"  ← cache_control 放这里
  [1] Ai [ToolUse{Read, call_abc}, ...]
  [2] Tool ToolResult{call_abc, "skill 全文内容..."}  ← 每次 skill 内容/ID 不同
  [3] Human "用户的真实问题"
  ...
```

- 合成的 Human 消息固定为 `"(System: Preloading skill files)"`，看似稳定
- 但紧跟其后的 ToolResult 包含 skill 全文内容，tool_call_id 是 UUID 随机生成
- 不同 skill、不同 session 的 preload 内容不同，导致 cache_control 标记的缓存段无法跨请求复用
- 首轮之后合成消息已稳定在历史中，后续轮次缓存正常命中

### 影响范围

- **仅首轮失效**：第二轮起缓存恢复正常
- **仅 Anthropic 受影响**：OpenAI 的 prompt caching 机制不同，不受 `cache_control` 位置影响
- **仅使用 `/skill-name` 触发 preload 时受影响**：不使用 skill preload 的对话不受影响

## 修复方案

保持 Ai[ToolUse] → Tool[ToolResult] 消息序列不变，将 `prepend_message` 改为 `add_message`，使工具调用追加到用户消息之后（executor 在 `before_agent` 之前已将用户消息 `add_message` 到 state）。

```
修复前 messages 数组：
  [System "系统提示词"]  ← prepend
  [Human "(System: Preloading...")  ← cache_control 落这里，每次变化
  [Ai]    [ToolUse{Read, call_{uuid}}]
  [Tool]  ToolResult{skill 全文}
  [Human "用户消息"]

修复后 messages 数组：
  [System "系统提示词"]  ← prepend，独立缓存
  [...历史消息...]
  [N-1] Human "用户消息"  ← cache_control 放这里，前缀稳定
  [N]   Ai [ToolUse{Read, call_{uuid}}]  ← add_message 追加
  [N+1] Tool ToolResult{skill 全文}
  ...
```

- 第一条 user 消息始终是真实用户输入，cache_control 缓存段稳定
- 工具调用追加在用户消息之后，不影响缓存边界
- `extract_skills_paths()` 新增 System/Human 消息 `[Skill: path]` 扫描，兼容中间迭代格式的 session

## 复现条件

- **复现频率**：必现（修复前）
- **触发步骤**：
  1. 使用 Anthropic 模型
  2. 在消息中使用 `/skill-name` 触发 skill preload
  3. 观察首轮 LLM 调用的 `cache_creation_input_tokens` 偏高、`cached_tokens` 为 0
- **环境**：Anthropic API，`enable_cache = true`

## 修改文件

- `peri-middlewares/src/subagent/skill_preload.rs` — `prepend_message` 改为 `add_message`
- `peri-agent/src/agent/compact/re_inject.rs` — `extract_skills_paths()` 新增 System/Human 消息 `[Skill: path]` 扫描
- `peri-middlewares/src/subagent/tool_test.rs` — 测试适配新格式
- `peri-agent/src/llm/anthropic.rs` — 移除错误的占位 thinking block 注入逻辑
