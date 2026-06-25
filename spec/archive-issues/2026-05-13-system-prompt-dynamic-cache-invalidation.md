> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-system-prompt-dynamic-cache-invalidation.md
# System prompt 动态内容导致 Anthropic prompt cache 频繁失效

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-13

## 问题描述

System 字段被序列化为单个 `TextBlockParam`，所有内容（静态段落 + `{{date}}`/`{{cwd}}` 动态占位符 + middleware 注入）合并为一个缓存段。任何动态内容变化（日期每日变化、cwd 跨项目变化、middleware 文件内容变化）都会导致整个 system 缓存失效。

## 症状详情

### 调查发现

4 个并行调查 agent 对 system prompt 构建链路进行了全面审查，发现以下动态化因素：

| 因素 | 位置 | 触发频率 | 影响 |
|------|------|---------|------|
| `{{date}}` 每日变化 | `prompt.rs:47` / `07_env.md:6` | 每天 | system 字段变化，**跨天必定缓存失效** |
| `{{cwd}}` 跨项目变化 | `prompt.rs:137` / `07_env.md:2` | 切换项目时 | 多分屏不同 cwd 无法共享缓存 |
| AgentsMdMiddleware 每轮读文件 | `agents_md.rs:162,230` | 编辑文件时 | CLAUDE.md/AGENTS.md 变化时 prepend 内容变化 |
| SkillsMiddleware 每轮磁盘扫描 | `skills/mod.rs:179` | skills 目录变化时 | 无缓存机制，每轮 IO + 解析 frontmatter |
| AskUserQuestion 改变 user 消息结构 | `anthropic.rs:219-222` | 每次调用 | ToolResult 合并到 user 消息，可能影响缓存断点 |

### 已确认安全的因素

| 因素 | 状态 |
|------|------|
| HashMap 迭代顺序不确定 | ✅ `sort_by_key()` 已修复 |
| SkillPreload prepend_message | ✅ 已改为 `add_message` |
| ToolSearchMiddleware 提示词 | ✅ `cached_prompt` + 按名称排序 |
| MCP 工具动态性 | ✅ deferred 过滤，不在 LLM 可见列表中 |
| 核心工具描述 | ✅ 全部硬编码，无动态内容 |
| `{{available_agents}}` | ✅ `scan_agents` 已排序 |

### 根因

`build_system_prompt()` 将所有段落（01-07 静态 + feature-gated）拼接为单个 `String`，动态占位符直接替换到字符串中。`messages_to_anthropic()` 将所有 System 消息 `join("

")` 为单个文本，序列化为一个带 `cache_control` 的 `TextBlockParam`。整个 system prompt 作为单一缓存段，任何位置的内容变化都导致前缀缓存完全失效。

## 修复方案

采用**边界标记（Boundary Marker）**方案，仿照 Claude Code 的 `splitSysPromptPrefix()` 模式。

### 实现变更（2 个文件）

**`peri-tui/src/prompt.rs`**：
- 将 `07_env.md` 从 `static_sections` 移到 `dynamic_sections`
- 在静态段落（01-06）和动态段落（07_env + feature-gated）之间插入 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__`

**`peri-agent/src/llm/anthropic.rs`**：
- 新增 `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` 常量和 `SystemPromptBlock` 结构体
- 新增 `split_system_blocks()` 方法：按边界标记拆分为缓存块/非缓存块
- `messages_to_anthropic()` 返回 `Vec<SystemPromptBlock>` 替代 `Option<String>`
- 序列化逻辑：缓存启用时输出多块数组（静态块标记 `cache_control`，动态块不标记）

### 修复后的 system 字段结构

```json
{
  "system": [
    {"type": "text", "text": "01-06 静态核心（~80% token）", "cache_control": {"type": "ephemeral"}},
    {"type": "text", "text": "07_env + feature-gated + middleware 注入（~20% token）"}
  ]
}
```

### 缓存效果

- 块1（~80% token）：始终命中缓存，跨天/跨项目均不受影响
- 块2（~20% token）：不缓存，`{{date}}`/`{{cwd}}`/middleware 变化不影响前缀

### 未变更的部分

- `ReactLLM` trait 签名 — 不变（避免 20+ impl 更新）
- `LlmRequest.system` 类型 — 保持 `Option<String>`
- `BaseModelReactLLM.system` — 保持 `Option<String>`
- `ReActAgent.system_prompt` — 保持 `Option<String>`
- `prepend_message` 机制 — 不变
- `with_system_builder` 路径 — 透明兼容
- OpenAI 路径 — 不受影响

## 涉及文件

- `peri-tui/src/prompt.rs` — 插入边界标记
- `peri-agent/src/llm/anthropic.rs` — 多块拆分和序列化

## 相关 Issue

- `2026-05-13-askuserquestion-cache-hit-rate-drop.md` — 缓存下降的症状（本 issue 的子集）
- `2026-05-12-skill-preload-invalidates-prompt-cache.md` — 类似缓存失效（已修复）
- `2026-05-12-deferred-tool-list-nondeterministic-order.md` — 工具列表排序（已修复）

## 后续优化方向

当前方案将所有动态内容合并为单个非缓存块（~20% token）。可进一步拆分为多块：
- 07_env 单独一块（不缓存）
- feature-gated 静态段落（10, 12, 13）单独一块（缓存，session 内稳定）
- 11_subagent 单独一块（不缓存，agents 列表动态）
- middleware 注入各为独立块（不缓存）

需变更 `ReactLLM` trait 签名或引入新的类型传递机制，变更面较大（7+ 文件、20+ 测试），建议作为独立优化。
