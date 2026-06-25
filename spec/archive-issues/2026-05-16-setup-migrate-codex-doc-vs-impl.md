> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-migrate-codex-doc-vs-impl.md
# migrate_from_claude_code 文档声称支持 CODEX 前缀但未实现

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

`migrate_from_claude_code` 的文档注释声明检测 `ANTHROPIC_`、`OPENAI_`、`CODEX_` 三种前缀，但实际 `prefixes` 数组只有 `ANTHROPIC` 和 `OPENAI`。CODEX 前缀凭据被静默忽略。

## 症状详情

| 现象 | 详情 |
|------|------|
| CODEX 凭据不迁移 | 用户有 `CODEX_API_KEY` 时不会被识别为任何 provider |
| 文档误导 | 代码注释和实际行为不一致 |

## 根因

`peri-tui/src/app/setup_wizard.rs:328 vs 355-368`

```rust
// 注释声称:
/// - `ANTHROPIC_` → Anthropic provider
/// - `OPENAI_` / `CODEX_` → OpenAI Compatible provider

// 实现:
let prefixes: &[(&str, ProviderType, &str, &[&str])] = &[
    ("ANTHROPIC", ProviderType::Anthropic, "anthropic", ...),
    ("OPENAI", ProviderType::OpenAiCompatible, "openai", ...),
    // CODEX 缺失
];
```

## 期望

添加 CODEX 前缀到 `prefixes` 数组，或将文档改为只声明已实现的前缀。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `migrate_from_claude_code()` (line 331-439)
