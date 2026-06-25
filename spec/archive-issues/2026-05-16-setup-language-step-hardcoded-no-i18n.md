> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-language-step-hardcoded-no-i18n.md

# Language 步骤完全硬编码中英混合文本，忽略 i18n

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

`render_step_language` 中的标题、提示语、快捷键提示行全部硬编码为英语+中文混合字符串。FTL 文件中已定义 `setup-language-title`、`setup-language-prompt`、`setup-key-confirm` 等翻译 key，但完全未使用。`_lc` 参数被故意忽略。

## 症状详情

| 现象 | 详情 |
|------|------|
| 标题硬编码 | "── Peri Setup ── Language / 语言" |
| 提示硬编码 | "Choose your language / 选择语言：" |
| 快捷键硬编码 | "Enter :Confirm / 确认", "↑/↓ :Select / 选择", "Esc :Quit / 退出" |
| 中英混合 | 选择 zh-CN 后界面仍显示中英混合文本，而非纯中文 |

## 根因

`peri-tui/src/ui/main_ui/popups/setup_wizard.rs:91-143`

```rust
fn render_step_language(
    f: &mut Frame,
    wizard: &SetupWizardPanel,
    _lc: &crate::i18n::LcRegistry,  // ← 故意忽略
    area: Rect,
) {
    // 所有文本硬编码
    "── Peri Setup ── Language / 语言"
    "Choose your language / 选择语言："
    ":Confirm / 确认"
    // ...
}
```

## 期望

使用 `lc.tr("setup-language-title")`、`lc.tr("setup-key-confirm")` 等已定义的 i18n key，移除硬编码文本。

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_step_language()` (line 91-143)
- `peri-tui/locales/en/main.ftl` —— 检查已有 key 是否需要补充
- `peri-tui/locales/zh-CN/main.ftl` —— 同上
