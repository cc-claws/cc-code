> 归档于 2026-06-06，原路径 spec/issues/2026-06-06-input-bar-cursor-mispositioned-after-cjk-input.md

# 输入框闪烁光标 Bug 集合

**状态**：Closed
**优先级**：中
**创建日期**：2026-06-06
**关闭日期**：2026-06-06

## 问题描述

peri-tui 输入框的光标闪烁机制存在多个 Bug，核心原因是 `render_textarea` 中 `cursor_visible` 仅控制自绘 `│` 光标的显示/隐藏，但未同步控制 `tui_textarea` 自带块光标的可见性，导致闪烁周期内出现两个光标交替。

## Bug 1：CJK 文字输入后光标位置偏移 [已修复]

输入中文、日文、emoji 等宽字符后，闪烁光标 `│` 的绘制位置偏左，没有跟随到文字末尾。

**根因**：`draw_bar_cursor`（`main_ui/mod.rs:400`）直接用 `tui_textarea::cursor()` 返回的字符索引 `data_col` 作为屏幕列宽。CJK 字符显示宽度为 2，字符索引 ≠ 显示列宽。

**修复**：改用 `unicode_width::UnicodeWidthChar::width` 累加计算显示列宽。

## Bug 2：闪烁灭相时 textarea 自带光标泄漏 [已修复]

**症状**：输入 `:测试` 后，光标在 `│` 和 `s`（或字符）之间交替闪烁。

**根因**：`render_textarea` 中，闪烁灭相（`cursor_visible=false`）时只跳过了 `draw_bar_cursor` 调用，但 `tui_textarea` 自带的块光标仍然渲染。`tui-textarea` 默认光标样式是 REVERSED，灭相时未隐藏。

**修复**：textarea 自带块光标始终设为与文本同色（`Style::default().fg(theme::TEXT)`），视觉不可见，由 `draw_bar_cursor` 统一控制 `│` 的显示/隐藏。

## Bug 3：退格/方向键后光标闪烁位置异常 [已修复]

**症状**：输入 `123` 后按退格或左方向键，光标在字符上闪烁位置异常。

**根因**：与 Bug 2 相同机制，两种光标交替导致视觉异常。

**修复**：随 Bug 2 一并修复，统一由 `draw_bar_cursor` 控制光标渲染。

## 修复方案

### 1. draw_bar_cursor 字符索引→显示列宽转换

```rust
// data_col 是字符索引，需转换为显示列（累加 unicode-width）
let display_col: usize = line
    .chars()
    .take(data_col)
    .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
    .sum();
```

### 2. render_textarea 统一光标控制

```rust
// textarea 自带块光标始终设为与文本同色（视觉不可见），由 draw_bar_cursor 画 │
display_textarea.set_cursor_style(Style::default().fg(theme::TEXT));
```

## 涉及文件

- `peri-tui/src/ui/main_ui/mod.rs` — `render_textarea` + `draw_bar_cursor`

## 关联 Issue

- `spec/global/domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk` — 鼠标点击同样的字符索引 vs 显示列宽问题

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-06 | — | Open | agent | 创建 |
| 2026-06-06 | Open | Closed | wismyzhizi2018 | Bug 1/2/3 全部修复，draw_bar_cursor 改用 unicode-width，render_textarea 统一光标控制 |
