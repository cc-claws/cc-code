> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-system-prompt-missing-language-instruction.md

# 系统提示词缺少语言指示段落，AI 多轮对话后漂移至英文

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27

## 问题描述

用户使用中文与 AI 对话时，多轮对话后 AI 会中途切换为英文回复。系统提示词中没有任何语言指示段落，LLM 无法获知用户期望的响应语言。与 Claude Code 对比发现，Claude Code 在系统提示词中动态注入 `# Language` 段落来锁定响应语言。

## 症状详情

| 轮次 | 用户输入语言 | AI 响应语言 | 说明 |
|------|-------------|-------------|------|
| 第 1-3 轮 | 中文 | 中文 | 正常 |
| 第 4+ 轮 | 中文 | 英文 | 突然切换，后续持续英文 |

- 用户界面语言已设为中文（`/lang zh-CN`）
- AI 系统提示词中无任何语言相关指令
- 切换后即使继续用中文输入，AI 仍可能用英文回复

## 对比分析

### Perihelion 现状

- **UI i18n**：完整的 Fluent 国际化系统，`/lang` 命令可切换界面语言
- **配置字段**：`AppConfig.language: Option<String>` 已存在（`peri-acp/src/provider/config.rs:128`）
- **系统提示词**：`build_system_prompt()` 的 11 个段落（01-07 + 10-13）中**无任何语言指示**
- **语言偏好未传递给 LLM**：`language` 字段仅用于 UI 渲染，从未传入系统��示词

### Claude Code 实现

```typescript
// src/constants/prompts.ts
function getLanguageSection(languagePreference: string | undefined): string | null {
  if (!languagePreference) return null
  return `# Language
Always respond in ${languagePreference}. Use ${languagePreference} for all explanations,
comments, and communications with the user. Technical terms and code identifiers should
remain in their original form.`
}

// 在 getSystemPrompt() 中动态注入
return [
  // ... 其他段落
  getLanguageSection(settings.language),
]
```

### 关键差异

| 维度 | Perihelion | Claude Code |
|------|-----------|-------------|
| 界面语言切换 | `/lang` 命令 | `/lang` 命令 |
| 语言配置持久化 | `AppConfig.language` | `preferredLanguage` |
| 系统提示词语言段落 | **缺失** | `# Language` 段落 |
| LLM 响应语言控制 | 无 | 明确指令 |

## 复现条件

- **复现频率**：必现（多轮对话后）
- **触发步骤**：
  1. 设置界面语言为中文（`/lang zh-CN`）
  2. 用中文与 AI 进行 4+ 轮对话
  3. AI 回复语言从中文漂移到英文
- **环境**：所有模型、所有 OS

## 涉及文件

- `peri-acp/src/prompt/mod.rs` —— `build_system_prompt()` 函数，需新增语言段落注入
- `peri-tui/prompts/sections/` —— 系统提示词段落目录，可能需要新增语言段落文件
- `peri-acp/src/provider/config.rs:128` —— `AppConfig.language` 字段（已存在）
- `peri-acp/src/session/executor.rs` —— executor 传递 language 参数到 `build_system_prompt()`
- `peri-tui/src/acp_server/prompt.rs` —— TUI 侧 prompt 入口，传递 language 到 frozen data

## 修复方向

1. 在 `build_system_prompt()` 的**动态段落区域**（`__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之后）注入 `# Language` 段落
2. 从 `AppConfig.language` 读取语言偏好，传入 `build_system_prompt()`
3. 语言偏好应在 `session/new` 时冻结到 `FrozenSessionData`（与 cwd/date 同策略）
4. 支持 `auto` 模式：无显式配置时从系统 locale 推断
5. 语言段落模板参考 Claude Code："Always respond in {language}. Technical terms and code identifiers should remain in their original form."
