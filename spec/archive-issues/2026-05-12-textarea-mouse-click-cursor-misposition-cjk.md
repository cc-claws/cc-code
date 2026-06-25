> 归档于 2026-05-16，原路径 spec/issues/2026-05-12-textarea-mouse-click-cursor-misposition-cjk.md

# 输入框鼠标点击光标定位不准

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-12

## 问题描述

在输入框中点击鼠标定位光标时，光标位置与鼠标点击位置存在偏差。包含中文等多字节字符时偏差更明显，纯 ASCII 文本也会因 padding 和水平滚动偏移而不准。

## 症状详情

- **复现频率**：必现
- **触发步骤**：
  1. 在输入框输入包含中文的文本（如 `你好世界hello`）
  2. 用鼠标点击中文字符中间位置
  3. 光标定位到错误位置（偏向行末）
- **环境**：所有平台

## 根因分析

`event.rs` 中 `MouseEventKind::Down(Left)` 和 `Drag(Left)` 处理输入框点击时，将 `mouse.column - area.x` 直接传给 `CursorMove::Jump(row, col)`，存在三个偏移问题：

### 问题 1：CJK 字符宽度

`tui_textarea` 的 `Jump(row, col)` 中 `col` 是**字符索引**（`fit_col` 使用 `line.chars().count()` 钳位），而 `mouse.column` 是**终端显示列坐标**。CJK 字符每个占 2 列宽：

| 场景 | 鼠标点击列 | Jump 收到的 col | 期望字符索引 |
|------|-----------|----------------|------------|
| `你好` 点击第 2 列 | 2 | 2（越界→行末） | 1 |
| `你好` 点击第 3 列 | 3 | 3（越界→行末） | 2 |

### 问题 2：Block border + padding 偏移

`build_textarea()` 配置了 `Padding::new(2, 0, 0, 0)`（左边 2 列 padding）和 `Borders::TOP | Borders::BOTTOM`。渲染时 `Block::inner(area)` 会去掉这些，但鼠标坐标减去的是 `area.x` 而非 `inner.x`，导致向右偏移 2 个字符。

### 问题 3：水平滚动偏移

当文本超长超出可见宽度时，`tui_textarea` 会水平滚动（`WrapMode::None`）。可见区域内的第 0 列对应文本的第 `top_col` 显示列。原代码没有考虑这个偏移，导致光标向左偏移。

`tui_textarea` 的 `viewport` 是私有的，无法直接获取 `top_col`，需要通过 cursor 位置和文本内容反推。

## 修复方案

新增 `textarea_mouse_to_cursor()` 函数，统一处理三个偏移：

1. **Block inner area**：通过 `textarea.block().inner(area)` 计算文本区域的精确坐标
2. **水平滚动反推**：通过 `cursor()` 位置 + `unicode_width` 计算当前 cursor 的显示列，推导 `scroll_col`
3. **CJK 宽度转换**：`display_col_to_char_idx()` 逐字符累加 `unicode_width`，将文本内显示列转换为字符索引

在 MouseDown 和 MouseDrag 两处 `Jump` 调用前统一使用此函数。

## 相关代码

- `peri-tui/src/event.rs:14-29` —— `display_col_to_char_idx()` 辅助函数
- `peri-tui/src/event.rs:31-90` —— `textarea_mouse_to_cursor()` 主函数
- `peri-tui/src/app/mod.rs:760-763` —— textarea block 配置（Padding + Borders）
- `peri-tui/src/ui/main_ui.rs:260-265` —— textarea 渲染和 area 赋值
