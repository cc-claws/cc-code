> 归档于 2026-05-18，原路径 spec/issues/2026-05-17-main-ui-heavy-file.md

# peri-tui/src/ui/main_ui.rs 主 UI 布局逻辑集中（852 行）

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-17
**解决日期**：2026-05-17
**修复 commit**：`c86888a`

## 问题描述

`peri-tui/src/ui/main_ui.rs` 达到 852 行（30KB），包含主 UI 的布局计算和事件处理逻辑。虽然相对于其他超限文件行数较低，但主 UI 是高频修改区域，集中式结构增加修改风险。

## 现状数据

| 指标 | 值 |
|------|-----|
| 行数 | 852 |
| 大小 | 30KB |
| 主要职责 | 主界面布局 + 事件分发 + 状态栏/Header 组装 |

## 期望改进方向

按布局和事件分离：

```
ui/main_ui/
├── mod.rs          ← 入口
├── layout.rs       ← 布局计算（split pane、消息区域、状态栏区域）
├── event_handler.rs ← 事件处理逻辑
├── status_bar.rs   ← （已独立，移至此处或保持原样）
└── sticky_header.rs ← （已独立，移至此处或保持原样）
```

## 涉及文件

- `peri-tui/src/ui/main_ui.rs`（852 行，30KB）
