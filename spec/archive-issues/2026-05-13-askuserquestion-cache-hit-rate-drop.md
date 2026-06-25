> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-askuserquestion-cache-hit-rate-drop.md
# AskUserQuestion 导致缓存命中率极速下降

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

每次调用 `AskUserQuestion` 工具时，Prompt cache 命中率会极速下降（甚至降到 0%）。用户通过缓存警告通知观察到此现象。

## 症状详情

### 用户观察

| 观察维度 | 结果 |
|---------|------|
| 缓存下降时机 | 每次调用 AskUserQuestion 时 |
| 观察方式 | 缓存警告通知 |
| 问题选项内容 | 每次完全不同 |

### 相关代码

**中间件 `prepend_message` 调用点**：

| 中间件 | 文件 | 内容 |
|--------|------|------|
| `AgentsMdMiddleware` | `agents_md.rs:162, 230` | CLAUDE.md 内容 |
| `ToolSearchMiddleware` | `tool_search/middleware.rs:79` | deferred tools 列表（缓存） |
| `SkillsMiddleware` | `skills/mod.rs:179` | skills 摘要 |

**缓存边界策略**（`anthropic.rs:257-263`）：
```rust
/// **缓存策略**（3 断点）：
/// 1. **第一条 user 消息**：system + 首条 user 构成稳定缓存段
/// 2. **倒数第二条 user 消息**：多轮对话中，上一轮的 user+assistant+tool 整段可被缓存
/// 3. **最后一条 user 消息**：当前轮次的完整前缀可被缓存
```

## 复现条件

- **复现频率**：每次调用 AskUserQuestion
- **触发步骤**：
  1. 使用 Anthropic 模型进行对话
  2. 触发 AskUserQuestion 工具调用
  3. 观察缓存警告通知
- **环境**：Anthropic API，`enable_cache = true`

## 初步分析

### 可能原因

1. **System 消息顺序不稳定**：
   - 多个中间件使用 `prepend_message` 注入 System 消息
   - 中间件执行顺序：AgentsMdMiddleware → SkillsMiddleware → ToolSearchMiddleware
   - 如果各中间件的 System 消息内容或顺序不稳定，会导致 system 字段变化

2. **ToolSearch 提示词缓存失效**：
   - `ToolSearchMiddleware.cached_prompt()` 在首次时生成
   - 如果 deferred tools 列表发生变化（如 MCP 工具动态加载），缓存会失效

3. **AskUserQuestion ToolUse 差异**：
   - 每次 AskUserQuestion 的 input（问题选项）不同
   - 虽然这只影响 assistant/tool 消息，但可能间接触发某些中间件行为变化

### 需要验证

- [ ] 检查 `AskUserQuestion` 调用前后的 system 字段内容
- [ ] 检查 `ToolSearchMiddleware.cached_prompt()` 是否稳定
- [ ] 检查是否有中间件在 `AskUserQuestion` 调用时动态生成消息
- [ ] 检查 `AskUserQuestion` 的 ToolResult 是否被正确处理

## 修复记录

根因已由 `2026-05-13-system-prompt-dynamic-cache-invalidation.md` 修复：system prompt 的 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记将静态内容（~80% token）与动态内容分离，静态核心始终命中缓存。AskUserQuestion 调用时缓存下降是 system prompt 动态占位符（`{{date}}`/`{{cwd}}`）导致整段缓存失效的子集表现。

## 相关 Issue

- `2026-05-12-skill-preload-invalidates-prompt-cache.md` — 类似的缓存失效问题（已修复，使用 `add_message` 替代 `prepend_message`）
- `2026-05-12-deferred-tool-list-nondeterministic-order.md` — deferred tools 列表排序问题（已修复）

## 涉及文件

- `peri-middlewares/src/tool_search/middleware.rs` — ToolSearch 中间件
- `peri-middlewares/src/agents_md.rs` — AgentsMd 中间件
- `peri-middlewares/src/skills/mod.rs` — Skills 中间件
- `peri-agent/src/llm/anthropic.rs` — 缓存策略实现
- `peri-middlewares/src/tools/ask_user_tool.rs` — AskUserQuestion 工具
