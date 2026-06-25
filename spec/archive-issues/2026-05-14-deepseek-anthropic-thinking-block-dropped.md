> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-deepseek-anthropic-thinking-block-dropped.md

# DeepSeek Anthropic 兼容端口：SkillPreloadMiddleware 注入的伪 assistant 消息缺少 thinking block

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-14

## 问题描述

使用 DeepSeek 的 Anthropic 兼容端口（`api.deepseek.com`）+ `deepseek-v4-pro` 模型，开启 thinking 模式时，`SkillPreloadMiddleware` 注入的伪 assistant 消息（fake ToolUse Read）不含 `thinking` block，导致 DeepSeek API 返回 400 错误：`"The content[].thinking in the thinking mode must be passed back to the API."`。

DeepSeek 要求 thinking 模式下**所有** assistant 消息都必须回传 thinking block（含 signature），但 `SkillPreloadMiddleware` 构造的伪造消息天然不含 thinking。

## 根因分析

### 触发链路

1. 用户输入 `/issue-create 提交为一个 issue`
2. `SkillPreloadMiddleware.before_agent()` 拦截，构造伪 Ai 消息（仅含 `ToolUse{Read, path=SKILL.md}`）+ Tool result，通过 `add_message` 追加到 state
3. ReAct 循环调用 DeepSeek API，`messages_to_anthropic()` 序列化所有消息
4. DeepSeek 发现 assistant 消息缺少 thinking block → 400

### 代码定位

`peri-middlewares/src/subagent/skill_preload.rs:115`：

```rust
state.add_message(BaseMessage::ai_from_blocks(tool_use_blocks));
```

构造的 Ai 消息只有 `ToolUse` blocks，没有 `Reasoning`（thinking）block。

### 为什么不是"偶发"

之前记录为"偶发"是因为需要同时满足两个条件：

1. 开启 thinking 模式（DeepSeek）
2. 用户输入触发 `/skill` 斜杠命令，导致 `SkillPreloadMiddleware` 注入伪消息

只要同时满足这两个条件，**必现**。

## 症状详情

### 现象 1（2026-05-14 09:30）

请求数据路径：`data/2026-05-14_09-30-28-483_0041/`

**请求体配置**：

- `model`: `deepseek-v4-pro`
- `thinking`: `{"budget_tokens": 8000, "type": "enabled"}`
- `output_config`: `{"effort": "high"}`
- `messages`: 17 条

**各 assistant 消息 thinking block 状态**：

| 消息索引 | thinking block | signature | 包含的 block 类型 |
|----------|---------------|-----------|------------------|
| msg[1] | ✓ | ✓ | thinking, tool_use |
| msg[3] | ✓ | ✓ | thinking, tool_use ×3 |
| msg[5] | ✓ | ✓ | thinking, tool_use ×3 |
| msg[7] | ✓ | ✓ | thinking, tool_use ×5 |
| msg[9] | ✓ | ✓ | thinking, tool_use ×2 |
| msg[11] | ✓ | ✓ | thinking, text, tool_use |
| msg[13] | ✓ | ✓ | thinking, text |
| **msg[15]** | **✗ 缺失** | — | **tool_use（仅 Read）** |

msg[15] 是 `SkillPreloadMiddleware` 注入的伪消息，仅包含 `tool_use(Read)` 读取 SKILL.md。

### 现象 2（2026-05-14 10:27）

请求数据路径：`data/2026-05-14_10-27-30-417_0060/`

**请求体配置**：

- `model`: `deepseek-v4-pro`
- `thinking`: `{"budget_tokens": 8000, "type": "enabled"}`
- `messages`: 18 条

| 消息索引 | thinking block | 包含的 block 类型 |
|----------|---------------|------------------|
| msg[1] | ✓ | thinking, tool_use |
| msg[3] | ✓ | thinking, text |
| msg[4] | ✓ | thinking, tool_use |
| msg[6] | ✓ | thinking, text |
| msg[8] | ✓ | thinking, tool_use ×5 |
| msg[10] | ✓ | thinking, text, tool_use |
| msg[12] | ✓ | thinking, tool_use ×4 |
| msg[14] | ✓ | thinking, text |
| **msg[16]** | **✗ 缺失** | **tool_use（Read SKILL.md）** |

同一根因，`/issue-create` 触发 `SkillPreloadMiddleware` 注入伪消息。

### 关键证据

Session `019e2603-d767-7592-b24a-6c9dcd89e6ce` 的 6 个成功 API 响应中，**所有 DeepSeek 响应都包含 thinking block**——无论 stop_reason 是 `tool_use` 还是 `end_turn`，无论是否有 text。这证明 DeepSeek 不会遗漏 thinking block，问题出在本地构造的伪消息。

## 复现条件

- **复现频率**：必现（满足条件即触发）
- **触发条件**：
  1. 使用 DeepSeek Anthropic 兼容端口（`api.deepseek.com`），开启 thinking 模式
  2. 用户输入 `/skill-name` 斜杠命令，触发 `SkillPreloadMiddleware` 注入伪 assistant 消息
- **环境**：DeepSeek Anthropic 兼容 API + thinking 模式

## 修复方向

**方案 A（推荐）**：在 `messages_to_anthropic()` 序列化时，对不含 thinking/redacted_thinking 的 assistant 消息自动注入 `redacted_thinking` block。Anthropic 的 `redacted_thinking` 类型专门用于此场景——不暴露 thinking 原文，只需一个 opaque `data` 字段即可通过 API 验证。需确认 DeepSeek 是否支持 `redacted_thinking`。

**方案 B**：`SkillPreloadMiddleware` 注入时携带一个占位 `Reasoning` block。但伪造的 thinking block 无合法 signature，DeepSeek 可能拒绝。

**方案 C**：将 skill 内容通过 system prompt 注入而非 fake tool_use，避免构造伪 assistant 消息。

## 相关代码

- `peri-middlewares/src/subagent/skill_preload.rs:115` — 构造伪 Ai 消息（不含 Reasoning）
- `peri-agent/src/llm/anthropic.rs:201` — `messages_to_anthropic()` 序列化
- `peri-agent/src/llm/anthropic.rs:458-462` — `redacted_thinking` block 解析
- `peri-agent/src/llm/anthropic.rs:134-140` — `Reasoning` 序列化为 thinking block

## 关联 Issue

- `spec/issues/2026-05-12-thinking-reasoning-dataflow-issues.md`（Partial）— Anthropic 原生 API 的占位 thinking 缺 signature 问题，同属 thinking 数据流问题域
