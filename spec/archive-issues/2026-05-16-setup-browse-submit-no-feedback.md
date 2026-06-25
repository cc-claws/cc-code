> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-browse-submit-no-feedback.md

# Browse 模式 Submit 失败时无任何反馈

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

在 Browse 模式的 Submit 位置按 Enter，如果没有任何 provider 满足 `selected && is_complete()`，`step` 不变，仅返回 `Redraw`。用户界面无变化、无错误消息，完全不知道为何无法提交。

## 症状详情

| 现象 | 详情 |
|------|------|
| 界面无响应 | 按 Enter 后界面无变化，用户以为程序卡住 |
| 无错误消息 | 没有提示"请选中至少一个完整配置的 Provider"或类似信息 |
| 状态不变 | step 保持 Form，用户陷入死循环 |

## 根因

`peri-tui/src/app/setup_wizard.rs:640-649`

```rust
// handle_browse, line 640-649
tui_textarea::Input { key: Key::Enter, .. } => {
    if wizard.browse_cursor < wizard.providers.len() {
        // 进入 Edit
    } else {
        let has_valid = wizard.providers.iter()
            .any(|p| p.selected && p.is_complete());
        if has_valid {
            wizard.step = SetupStep::Done;  // 只有这里有反馈
        }
        // has_valid=false 时：返回 Redraw，界面无变化
        Some(SetupWizardAction::Redraw)
    }
}
```

## 期望

Submit 失败时应给出明显反馈。可通过以下任一方式实现：
- 设置临时错误消息显示在界面上
- 改变 Submit 按钮的样式（如短暂变红）
- 使用 `ephemeral_notes` 机制显示提示

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `handle_browse()` (line 640-649)
- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— `render_form_browse()` (line 158-255)
