> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-empty-providers-no-hint.md
# 空 providers 时无空状态提示

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

当 `wizard.providers` 为空时，Browse 模式只渲染一个 Submit 按钮和 hint 行，没有任何"没有 provider"的提示。用户看到一个空白区域 + Submit 按钮，界面意义不明确。

## 症状详情

| 现象 | 详情 |
|------|------|
| 空列表无提示 | Browse 模式 providers 为空时界面上只有 Submit |
| 意义不明 | 用户不知道为什么会看到空白页面 |

## 根因

`peri-tui/src/ui/main_ui/popups/setup_wizard.rs:176-229`

`render_form_browse` 中 providers 迭代为空时，无任何空状态占位信息。

## 期望

显示友好提示信息（如 "No providers configured" / "未配置任何 Provider"）和添加 provider 的操作指引。

## 涉及文件

- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_form_browse()` (line 158-255)
