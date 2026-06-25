> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-language-injection-subagent-drift-cache-isolation.md

# 语言段落注入导致 SubAgent 语言漂移和缓存隔离失效

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27

## 问题描述

`build_system_prompt` 新增 `language` 参数后在 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 后注入 `# Language` 段落。但在实现中存在三个问题：(1) SubAgent 与 Main Agent 语言跨轮次漂移；(2) Anthropic `i == last_idx` fallback 破坏了动态块的缓存隔离；(3) session/load/resume/fork 丢失 frozen_language。

## 症状详情

### 现象 1：Main Agent vs SubAgent 语言漂移

| 场景 | Main Agent 语言 | SubAgent 语言 |
|------|----------------|---------------|
| session/new (语言=zh-CN) | 冻结为 zh-CN | zh-CN（从 config 捕获） |
| 用户执行 `/lang en` | **不变**（frozen） | **变为 en**（下一轮重新捕获） |
| 后续 SubAgent 调用 | 仍为 zh-CN | 变为 en |

同一会话中，Main Agent 和 SubAgent 使用不同的语言指令，导致 SubAgent 用英文回复而主 Agent 用中文。

### 现象 2：`i == last_idx` 缓存隔离失效

`peri-agent/src/llm/anthropic/invoke.rs:333` 的 fallback 逻辑：

```rust
if b.cache_control || i == last_idx {
    block["cache_control"] = json!({"type": "ephemeral"});
}
```

`split_system_blocks` 设计意图是仅缓存静态前缀（Block 0），但 `i == last_idx` 给动态 Block 1 也加了 `cache_control`。**完整系统提示词（含 date/cwd/language/动态段落）全部被 Anthropic 缓存**，任何细微变化都导致 cache miss + 重新计费。

| | 预期 | 实际 |
|---|---|---|
| Block 0（静态 01-06） | 缓存 | 缓存 ✓ |
| Block 1（动态 + language） | 不缓存 | **被错误缓存** ✗ |

### 现象 3：session/load/resume/fork 后 frozen_language 丢失

| 操作 | frozen_system_prompt | frozen_language |
|------|---------------------|-----------------|
| session/new | `Some(...)` | `Some(...)` ✓ |
| session/load | `None` | `None` ✗ |
| session/resume | `None` | `None` ✗ |
| session/fork | `None` | `None` ✗ |

重建系统提示词时使用**当前** `peri_config.config.language`（重建后新值），而非原始 session 的语言值。如果用户重启应用后 config.language 不同，系统提示词的语言段落会发生变化。

## 复现条件

- **复现频率**：
  - 现象 1（语言漂移）：必现，多轮 SubAgent 调用 + `/lang` 切换
  - 现象 2（缓存失效）：`i == last_idx` 导致每次动态内容变化都重新缓存（设计级问题）
  - 现象 3（frozen 丢失）：必现，session/load 或 session/fork 后重建
- **触发步骤（现象 1）**：
  1. 启动 TUI，语言设置为 zh-CN
  2. 发起需要 SubAgent 的对话（如"帮我在多个文件里搜索 xxx"）
  3. 执行 `/lang en` 切换语言
  4. 再次发起 SubAgent 对话 → SubAgent 用 en 回复，Main Agent 仍用 zh-CN
- **环境**：所有模型，Anthropic provider（现象 2 仅限于 Anthropic 路径）

## 涉及文件

- `peri-agent/src/llm/anthropic/invoke.rs:333` —— `i == last_idx` fallback 破坏缓存隔离
- `peri-acp/src/agent/builder.rs:155-168` —— overrides 路径 `language: None`（死代码 bug）
- `peri-acp/src/agent/builder.rs:257-258` —— SubAgent `frozen_language_for_sub` 从 `peri_config.config.language` 克隆而非从 frozen 数据获取
- `peri-acp/src/session/executor.rs:402` —— `agent_overrides: None` 硬编码
- `peri-tui/src/acp_server/requests.rs` —— session/load/resume/fork 设置 `frozen_language: None`

## 修复方向

1. **【优先】SubAgent 语言与 Main Agent 对齐**：在 `AcpAgentConfig` 中增加 `frozen_language: Option<String>` 字段，`executor.rs` 构造时从 frozen 数据传入。`builder.rs` SubAgent system_builder 使用 `cfg.frozen_language` 而非 `peri_config.config.language`
2. **overrides 路径修复**：`builder.rs:155-168` 传入 `frozen_date.as_deref()` 和 `peri_config.config.language.as_deref()`
3. **缓存隔离修复**：评估 `i == last_idx` fallback 的必要性——如果 Anthropic 要求至少一个 cache_control block，考虑只在 Block 0 标记 cache_control，移除对 Block 1 的 fallback
4. **session/load/resume/fork frozen 数据恢复**：考虑在持久化/恢复 session 时保留 frozen 字段
