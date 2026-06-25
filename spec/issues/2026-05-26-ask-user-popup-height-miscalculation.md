# AskUser 弹窗高度计算不准确导致内容被截断

**状态**：Open
**优先级**：中
**创建日期**：2026-05-26
**最后尝试**：2026-05-26（4 次修复尝试均未解决）

## 问题描述

AskUser 弹窗的高度估算函数 `active_panel_height` 与实际渲染函数 `render_ask_user_popup` 存在多处不一致，导致当问题文本、选项 label、选项 description 任何一项文字较长触发自动换行时，弹窗高度偏小，底部内容被截断需要滚动才能看到。

## 症状详情

| 场景 | 预期 | 实际 |
|------|------|------|
| 选项 label 超长换行 | 弹窗高度恰好容纳所有内容 | 底部选项被截断 |
| 选项 description 超长换行 | 同上 | 同上 |
| 问题文本超长换行 | 同上 | 同上 |

用户需要额外滚动才能看到完整内容，尤其是底部的自定义输入区域和最后的选项。

## 原始不一致细节

高度估算（`mod.rs:active_panel_height`）vs 实际渲染（`ask_user.rs:render_ask_user_popup`）的差异：

1. **选项前缀宽度不匹配**：估算统一用 `label_w + 6`，实际单选 `❯ N. ` 约 6-8 列，多选 `❯ ○ N. ` 约 10 列
2. **description 缩进宽度不匹配**：估算用 `desc_w + 6`，实际多选 7 空格单选 5 空格
3. **选项之间空行未计入**
4. **自定义输入行未计入换行**
5. **"多选/单选提示行"估算中有但渲染中没有**（+1 多余）

## 根本困难

### 鸡生蛋问题：高度→布局→宽度→换行→高度

1. `active_panel_height` 在渲染**之前**被调用，返回的面板高度决定布局
2. 布局决定面板的 `content_area` 尺寸
3. `ScrollableArea` 在 `content_area` 内渲染，当内容超出时显示滚动条
4. 滚动条占 1 列，减少文本渲染宽度，导致更多换行
5. 更多换行 = 更多视觉行数 = 需要更高的面板

这个循环导致：估算高度偏小→面板偏小→滚动条出现→文本更窄→换行更多→高度更不够。

### 逻辑行数 vs 视觉行数

- `Text::height()` / `lines.len()` 返回**逻辑行数**
- ratatui `Paragraph::wrap()` 产生**视觉行数**（长行 wrap 后的实际显示行数）
- `ScrollableArea` 用逻辑行数判断是否需要滚动条，但渲染用视觉行数
- 视觉行数 >= 逻辑行数，差距随文本长度增大

## 修复尝试记录

### 尝试 1：手工对齐行结构

提取 `ask_user_content_height()` 函数，用 `div_ceil(panel_width)` 逐行估算换行行数。

**结果**：失败。`div_ceil` 是字符级换行，ratatui 用词级换行（`WordWrapper`），实际视觉行数更多。且自定义输入行未计入换行、前缀宽度硬编码不精确。

### 尝试 2：词级换行模拟

`build_line_texts()` 构建文本行 → `count_wrapped_lines()` 按空格拆词逐词累加宽度模拟词级换行。

**结果**：失败。模拟逻辑无法完全匹配 ratatui 的 `WordWrapper`（grapheme 级处理 vs 字符级），且 CJK 无空格文本需 fallback 到 `div_ceil`。

### 尝试 3：渲染时测量 + 二帧修正

在 `render_ask_user_popup` 中存储 `lines.len()`（后改为 `div_ceil` 视觉行数）到 `last_rendered_content_lines`，`active_panel_height` 优先使用实测值。

**结果**：失败。第一帧估算仍不准确；且实测值基于当前帧的 `content_area.width`（可能因滚动条而偏窄），导致第二帧高度仍不足。

### 尝试 4：使用 ratatui `Paragraph::line_count()`

两端（估算 + 实测）都改用 `Paragraph::new(text).wrap(Wrap{trim:false}).line_count(width)` 精确计算。

**结果**：仍失败。`line_count` 返回的视觉行数与实际渲染仍有偏差，可能与 `content_area.width` 的滚动条扣除或 `Paragraph` block 空间计算有关。

## 可能的解决方向

1. **改为 `Constraint::Max` 布局**：不预计算高度，用 `Constraint::Max(screen_height * 3/4)` 让面板自适应。需要在布局层改动
2. **两遍渲染**：先做一次 dry-run 渲染（不输出到 Frame），测量实际高度，再用精确值做正式布局
3. **固定高度 + 滚动条**：给 AskUser 弹窗一个足够大的固定高度（如 `min(visual_estimate, screen_height * 3/4)`），放弃"恰好 fit"的目标
4. **从 `ScrollableArea` 回传实际行数**：在 `ScrollableArea::render` 内部渲染完成后回传实际视觉行数，供下一帧使用

## 涉及文件

- `peri-tui/src/ui/main_ui/mod.rs`（`active_panel_height` 函数）—— 高度估算入口
- `peri-tui/src/ui/main_ui/popups/ask_user.rs`（`render_ask_user_popup`）—— 实际渲染
- `peri-widgets/src/scrollable.rs`（`ScrollableArea`）—— 滚动渲染，含逻辑行数 vs 视觉行数不一致
- `peri-widgets/src/bordered_panel.rs`（`BorderedPanel`）—— 边框布局

## 关联 Issue

- `spec/issues/2026-05-13-popup-cursor-scroll-not-following.md` —— 同一弹窗的光标滚动问题
