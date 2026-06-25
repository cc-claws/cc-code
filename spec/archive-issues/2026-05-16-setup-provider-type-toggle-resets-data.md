> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-provider-type-toggle-resets-data.md

# Edit 模式 ProviderType 切换静默重置所有已编辑数据

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

在 Edit 模式中，当焦点在 ProviderType 字段时，`←`/`→`/Space 键触发 `provider_type.cycle()` 后立即调用 `refresh_provider_defaults()`，重置 `provider_id`、`base_url` 和全部 3 个 model alias 为默认值（仅保留 api_key）。Left/Right 在其他字段是光标移动，在此字段却是数据破坏——用户极易误触发。

## 症状详情

| 现象 | 详情 |
|------|------|
| 静默数据丢失 | 用户手动修改了 provider_id 或 model alias 后，误按 ←/→ 导致全部重置 |
| 无任何警告 | 切换类型和覆盖默认值没有确认提示 |
| UI 不一致 | Up/Down 做字段导航，Left/Right 在此字段却做类型切换（不一致） |

## 根因

`peri-tui/src/app/setup_wizard.rs:677-696`

`Left`/`Right` 匹配臂在 ProviderType 分支直接调用 `cycle() + refresh_provider_defaults()`，没有重置保护或确认提示。

```rust
// handle_edit, line 677-685
tui_textarea::Input { key: Key::Left, .. } | tui_textarea::Input { key: Key::Right, .. } => {
    if wizard.form_focus == FormField::ProviderType {
        let mp = &mut wizard.providers[wizard.active_provider];
        mp.provider_type.cycle();
        mp.refresh_provider_defaults();  // ← 覆盖所有非 api_key 字段
    }
}
```

相同问题也存在于 Space 键处理（第 698-707 行）。

## 期望

切换 ProviderType 后不应无条件覆盖所有字段。要么：1) 仅切换类型不重置字段，2) 重置前提示用户将丢失数据，3) 仅 Space 做类型切换、←/→ 改为导航到相邻字段。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `handle_edit()` (line 677-696, 698-707), `MigratedProvider::refresh_provider_defaults()` (line 143-152)
