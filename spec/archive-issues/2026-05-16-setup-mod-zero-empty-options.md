> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-mod-zero-empty-options.md

# Language 步骤空选项下取模 panic 风险

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-16

## 问题描述

`handle_step_language` 和 `handle_step_choose` 中 Up/Down 导航使用 `(cursor + len - 1) % len`。如果 `LANGUAGE_OPTIONS.len()` 或 `SetupSource::ALL.len()` 变为 0，除以 0 导致 panic。当前两者都是固定常量（2 和 2），无实际风险，但缺乏防御性编程。

## 症状详情

| 现象 | 详情 |
|------|------|
| 当前无触发 | 常量 2 不为 0，实际不会 panic |
| 未来风险 | 如果移除所有语言选项或来源选项会 crash |

## 位置

`peri-tui/src/app/setup_wizard.rs:511-512, 561-562`

```rust
wizard.choose_cursor = 
    (wizard.choose_cursor + SetupSource::ALL.len() - 1) % SetupSource::ALL.len();
// ...
wizard.language_cursor = 
    (wizard.language_cursor + LANGUAGE_OPTIONS.len() - 1) % LANGUAGE_OPTIONS.len();
```

## 期望

添加 `assert!(len > 0)` 守卫，或使用 `checked_rem` 处理零长度情况。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `handle_step_choose()` (line 504-552), `handle_step_language()` (line 554-585)
