> 归档于 2026-05-16，原路径 spec/issues/2026-05-13-missing-skillpreload-in-main-agent.md
# 主 Agent 中间件链缺少 SkillPreloadMiddleware，/skill-name 预加载失效

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-13

## 问题描述

主 Agent 的中间件链中缺少 `SkillPreloadMiddleware`，导致通过 `/skill-name` 格式引用 skill 时，对应的 skill 全文无法以 fake Read 工具调用序列注入到 LLM 上下文。系统提示和 `SkillsMiddleware` 告诉 LLM 可以引用 skill 名称来加载全文，但实际预加载机制从未生效。

这是一个功能性退化：主 Agent 场景下 skill 预加载功能完全不可用。

## 症状详情

| 检查点 | 结果 |
|--------|------|
| SkillsMiddleware 注入摘要 | ✅ 正常 — 在 `before_agent` 时将所有 skill 的 `name + description` 摘要注入 |
| 系统提示引导 /skill-name | ✅ 正常 — `13_skills.md` 告诉 LLM "Mention a skill by name when you want to load its full content" |
| SkillPreloadMiddleware 注入全文 | ❌ 不执行 — 主 Agent 中间件链未注册该中间件 |
| 用户输入 `/skill-name` 检测 | ❌ 不执行 — `submit_message()` 无 skill 名解析逻辑 |

### 主 Agent 中间件链中的缺失

**`agent_assembler.rs`（145 行）：**
```rust
// SkillsMiddleware 之后直接是 FilesystemMiddleware，没有 SkillPreloadMiddleware
.add_middleware(Box::new(SkillsMiddleware::new()))
.add_middleware(Box::new(FilesystemMiddleware::new()))
```

**`agent.rs`（320-323 行）：**
```rust
// 同样缺少 SkillPreloadMiddleware
SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs),
// ... 直接跳到 FilesystemMiddleware
FilesystemMiddleware::new(),
```

### SkillsMiddleware 注入的摘要内容

```text
你可以使用以下 Skills（专项能力），在需要时提及其名称：

- **diagnose**: /path/to/skill ... Disciplined diagnosis loop for hard bugs...

如需加载某 skill 的完整内容，在消息中提及其 name 即可。用户一般会使用 '/skill-name' 的形式。
```

### 子 Agent 路径正常

作为对比，子 Agent 在 `tool.rs:458-462` 正确注册了 `SkillPreloadMiddleware`：

```rust
if !agent_def.frontmatter.skills.is_empty() {
    agent_builder = agent_builder.add_middleware(Box::new(SkillPreloadMiddleware::new(
        agent_def.frontmatter.skills.clone(),
        &cwd,
    )));
}
```

### 预期的 SkillPreloadMiddleware 行为

根据 `skill_preload.rs` 文档注释，`SkillPreloadMiddleware.before_agent` 应注入以下消息序列：

```text
[Human "用户消息"]   ← 已由 executor 添加
[Ai]    [ToolUse{Read, call_{hex}}, ToolUse{Read, call_{hex}}, ...]
[Tool]  ToolResult{call_{hex}, skill_0_content}
[Tool]  ToolResult{call_{hex}, skill_1_content}
```

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 在 TUI 消息中输入 `/diagnose`（或引用任何已安装 skill 的名称）并发送
  2. 观察 LLM 第一轮推理——没有 `Read` 工具调用，没有 skill 全文内容
  3. 对比：`SkillsMiddleware` 注入的摘要包含 skill 名称（确认 skill 已安装）
- **环境**：所有 Provider，所有模型

## 期望行为

1. 用户在消息中引用 skill 名称（`/skill-name` 或 `#skill-name`）时，`SkillPreloadMiddleware` 应注入对应 skill 的全文
2. 至少应满足子 Agent 已有的行为标准：在 `before_agent` 时以 fake Read 工具调用序列注入 skill 全文

## 相关代码

- `peri-tui/src/app/agent.rs:308-335` —— 主 Agent 中间件链（缺少 SkillPreloadMiddleware）
- `peri-tui/src/app/agent_assembler.rs:140-161` —— 另一个构建路径（缺少 SkillPreloadMiddleware）
- `peri-tui/src/app/agent_submit.rs` —— `submit_message()` 无 skill 名提取/解析逻辑
- `peri-middlewares/src/subagent/skill_preload.rs` —— SkillPreloadMiddleware 实现（可用，测试覆盖完整）
- `peri-middlewares/src/subagent/tool.rs:458-462` —— 子 Agent 正确注册的参考实现
- `peri-tui/prompts/sections/13_skills.md` —— 系统提示中的 skills 使用说明
- `peri-middlewares/src/skills/mod.rs:131-150` —— SkillsMiddleware 注入的摘要内容

## 修复方向

需要解决的问题是两个层面的缺口：

**1. 用户主动引用 skill（`submit_message` 输入检测）**

当前 `submit_message()` 将用户输入原文直接传给 `AgentInput::text()`，没有任何解析。需要：
- 解析用户输入中的 `/skill-name` 或 `#skill-name` 模式
- 提取 skill 名称列表（多个 skill 用空格分隔）
- 将提取的 skill 名称通过某种机制传递给 `SkillPreloadMiddleware`

**2. 主 Agent 中间件链注入**

在两个构建路径中添加 `SkillPreloadMiddleware`：
- `agent.rs` 在 `SkillsMiddleware` 之后、`FilesystemMiddleware` 之前插入
- `agent_assembler.rs` 同样位置

**实现约束**：
- `SkillPreloadMiddleware::new(vec![], &cwd)` 时 `before_agent` early return（已有逻辑），空列表无性能损耗
- 使用 `add_message` 而非 `prepend_message` 避免影响 prompt cache（`skill_preload.rs:18` 已有注释说明）
- 遵循 CLAUDE.md 中的中间件链顺序规范（4. SkillPreloadMiddleware ← SkillsMiddleware 之后）
