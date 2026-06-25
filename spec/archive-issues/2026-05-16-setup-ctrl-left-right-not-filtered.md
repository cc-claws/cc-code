> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-ctrl-left-right-not-filtered.md
# Left/Right 匹配臂未过滤 Ctrl 修饰符——Ctrl+Left 仍切换 ProviderType

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

`handle_edit` 中 Left/Right 键的匹配使用 `tui_textarea::Input { key: Key::Left, .. }`——`..` 通配符匹配任意 ctrl/alt/shift 状态。Ctrl+Left 在 ProviderType 焦点上也触发 `cycle()` + `refresh_provider_defaults()`，导致数据覆盖。虽然这是编辑键位的常见约定（Ctrl+Left 跳词），但 ProviderType 分支没有过滤修饰符。

## 症状详情

| 现象 | 详情 |
|------|------|
| Ctrl+Left 触发类型切换 | ProviderType 焦点时，Ctrl+Left 等效无修饰 Left |
| 数据丢失 | 同 Bug-P1-2 的数据覆盖问题 |
| 文本字段正常 | `handle_edit_key` 内部检查 `ctrl: false`，返回 false 不处理 |

## 根因

`peri-tui/src/app/setup_wizard.rs:677-696`

```rust
tui_textarea::Input { key: Key::Left, .. }  // ← .. 不检查 ctrl
| tui_textarea::Input { key: Key::Right, .. } => {
    if wizard.form_focus == FormField::ProviderType {
        mp.provider_type.cycle();       // Ctrl+Left 也执行
        mp.refresh_provider_defaults();
    }
}
```

## 期望

ProviderType 分支应显式要求 `ctrl: false`，或用独立 match 臂处理带修饰符的 Left/Right。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `handle_edit()` (line 677-696)
