> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-form-edit-labels-hardcoded.md

# Form Edit 字段标签硬编码英文，未使用 i18n

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 修复方案

1. `render_field_line` 增加 `label_width` 参数，新增 `pad_display_columns` 辅助函数用 unicode-width 对齐
2. Edit 模式所有字段标签改用 `lc.tr("setup-field-*")` 替换硬编码英文
3. `ProviderType::label()` 改为接受 `&LcRegistry`，新增 FTL key `setup-provider-anthropic/openai`
4. Done 步骤 "Key:" 和 alias 标签改用 i18n
5. FTL 补全 zh-CN 翻译：字段标签、provider 类型名
6. 同步修复 P2-8：API Key 遮罩 `mp.api_key.len()` → `mp.api_key.chars().count()`

## 问题描述

`render_form_edit` 中的四个字段标签（Type、ID、Base URL、API Key）以及模型后缀 "Model" 全部硬编码为英文。FTL 中已定义 `setup-field-type`、`setup-field-id`、`setup-field-base-url`、`setup-field-api-key`、`setup-model-label` 但没有被调用。`ProviderType::label()` 也返回硬编码英文 "Anthropic" / "OpenAI Compatible"，不接受 i18n 参数。

## 症状详情

| 现象 | 详情 |
|------|------|
| 字段标签英文 | "Type"、"ID"、"Base URL"、"API Key" 在任何语言下都是英文 |
| 模型后缀英文 | "Opus  Model"、"Sonnet Model"、"Haiku  Model" 中 "Model" 硬编码 |
| Provider 类型英文 | "Anthropic"、"OpenAI Compatible" 无翻译 |

## 根因

`peri-tui/src/ui/main_ui/popups/setup_wizard.rs:285-352` 和 `peri-tui/src/app/setup_wizard.rs:52-57`

```rust
// 四个标签全硬编码
render_field_line("Type     ", ...)
render_field_line("ID       ", ...)
render_field_line("Base URL ", ...)
render_field_line("API Key  ", ...)

// ProviderType::label() 不接受 i18n
pub fn label(&self) -> &str {
    match self {
        Self::Anthropic => "Anthropic",
        Self::OpenAiCompatible => "OpenAI Compatible",
    }
}
```

## 期望

- `render_field_line` 使用 `lc.tr()` 获取标签文本
- `ProviderType::label()` 接受 `&LcRegistry` 参数并返回翻译字符串
- FTL 文件中补充 ProviderType 翻译

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_form_edit()` (line 258-378), `render_form_browse()` (line 207), `render_step_done()` (line 441, 450, 453-461)
- `peri-tui/src/app/setup_wizard.rs` —— `ProviderType::label()` (line 52-57)
- `peri-tui/locales/en/main.ftl`
- `peri-tui/locales/zh-CN/main.ftl`
