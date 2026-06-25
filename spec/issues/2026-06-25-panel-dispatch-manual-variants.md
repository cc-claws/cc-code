# [TECH_DEBT] 面板事件分发依赖手动枚举变体列表，新增面板时易遗漏

**状态**: Open
**优先级**: P3
**模块**: peri-tui/event
**创建时间**: 2026-06-25
**发现方式**: bug 修复后复盘

## 现象

Tasks 面板打开后完全冻结，无法接收任何键盘输入（Escape、方向键均无效），也无法关闭。

## 根因

面板事件分发（键盘 + Paste）在两处文件中通过 `matches!()` 手动枚举所有 `PanelKind` 变体来路由到 Session/Global PanelManager。新增面板变体时需同步更新 4 个 `matches!()` 列表，编译器不强制检查完整性，漏加导致静默失败。

本次遗漏：

| 位置 | 漏加的变体 |
|------|-----------|
| `keyboard/panels.rs:75`（键盘 Global） | `Tasks` |
| `event/mod.rs:351`（Paste Session） | `CommandPalette` |
| `event/mod.rs:365`（Paste Global） | `Tasks` |

## 改进方向

`PanelKind::scope()` 已正确定义每个变体属于 `Session` 还是 `Global`，但分发逻辑未使用它。

将手动 `matches!()` 替换为基于 `scope()` 的自动路由：

```rust
// Before（手动枚举，易遗漏）:
if matches!(kind, Some(PanelKind::Status) | Some(PanelKind::Memory) | ...)

// After（自动路由，新增变体无需改分发代码）:
if kind.map(|k| k.scope()) == Some(PanelScope::Global)
```

目标：新增 `PanelKind` 变体时只需定义 `scope()`，分发逻辑自动适配。

## 涉及文件

- `peri-tui/src/event/keyboard/panels.rs` — `handle_panels` 函数，2 处 `matches!()`
- `peri-tui/src/event/mod.rs:341-369` — `Event::Paste` 处理，2 处 `matches!()`
- `peri-tui/src/app/panel_manager.rs:100-116` — `PanelKind::scope()` 定义
