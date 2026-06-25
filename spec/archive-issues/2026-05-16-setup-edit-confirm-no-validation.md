> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-edit-confirm-no-validation.md
# Edit Confirm 不做字段完整性校验

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

在 Edit 模式的 Confirm 字段按 Enter 直接返回 Browse 模式，不检查 provider 字段是否完整。用户在编辑过程中可能填了一半数据就返回，不完整数据处理被推迟到 Browse 的 Submit 阶段。功能上没问题（Submit 的 `is_complete()` 会兜底），但 UX 上无提示。

## 症状详情

| 现象 | 详情 |
|------|------|
| 无校验提示 | 填写一半按 Confirm 返回 Browse，无任何警告 |
| 延迟发现 | 用户到 Submit 阶段才发现数据不完整 |

## 根因

`peri-tui/src/app/setup_wizard.rs:721-728`

```rust
tui_textarea::Input { key: Key::Enter, .. } => {
    if wizard.form_focus == FormField::Confirm {
        wizard.form_mode = FormMode::Browse;  // 无条件返回
        Some(SetupWizardAction::Redraw)
    }
}
```

## 期望

Confirm 时可选择性地做字段完整性校验——如果 provider 不完整，显示提示或高亮缺失字段。也可以保持当前宽松设计（不做校验），但增加注释说明。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `handle_edit()` (line 721-728)
