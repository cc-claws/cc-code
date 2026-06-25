# Edit 工具内联 diff 视图渲染不优雅，内容贴到终端边缘

**状态**：Partial Fix（现象 1 已修复，现象 2 待确认）
**优先级**：低
**创建日期**：2026-06-06

## 问题描述

Edit 工具的内联 diff 视图（Ctrl+O 切换）渲染时，diff 行没有 `"  ⎿ "` 前缀（4 字符缩进），而 ToolBlock 结果行、Reasoning 详细内容等所有其他区域都有这个前缀。diff 行直接贴到终端左边缘，破坏了整体视觉一致性。

## 症状详情

### 现象 1：diff 行缺少 4 字符缩进前缀

`message_render.rs:642-644` 中 diff 行直接追加到渲染结果，没有加 `"  ⎿ "` 前缀：

```rust
if let Some(ref cached_lines) = diff_lines {
    lines.extend(cached_lines.iter().cloned());  // 无前缀，直接贴边
}
```

而同一文件中 ToolBlock 结果行（第 633 行）和 Reasoning 内容（第 488 行）都有 `"  ⎿ "` 前缀：

```rust
// ToolBlock 结果行
Span::styled("  ⎿ ".to_string(), ...)
// Reasoning 详细内容
Span::styled("  ⎿ ", ...)
```

### 现象 2：diff 渲染宽度硬编码 80

`build_diff_lines`（`message_view/mod.rs:70`）调用 `render_diff` 时硬编码宽度为 80，与终端实际宽度不匹配：

```rust
let lines = peri_widgets::diff::render_diff(&diff_input, 80, &peri_widgets::DarkTheme);
```

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 Edit 工具修改文件（new_string 包含较长代码）
  2. 开启内联 diff 显示（Ctrl+O）
  3. 观察 diff 视图的渲染效果
- **环境**：所有 OS

## 涉及文件

- `peri-tui/src/ui/message_view/mod.rs`（第 70 行）—— `build_diff_lines` 硬编码宽度 80
- `peri-tui/src/ui/message_render.rs`（第 642 行）—— diff 行直接 extend 无宽度约束
- `peri-widgets/src/diff/` —— `render_diff` 渲染实现

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-06 | — | Open | agent | 创建 |
| 2026-06-06 | Open | Partial Fix | agent | 现象 1 修复：diff 行添加 `  ⎿ ` 缩进前缀（commit 4f1a9212） |

## 修复记录

### 现象 1：diff 行缺少缩进前缀 — 已修复

- **Commit**: `4f1a9212` (`fix/diff-prefix-indent`)
- **改动**：`message_render.rs:641-655`，给 diff 行 insert `"  ⎿ "` 前缀 Span，与 result 行风格一致
- **验证**：新增断言 `detail_text.contains("  ⎿ ")`，18 个 message_render 测试全部通过

### 现象 2：diff 渲染宽度硬编码 80 — 待确认

需要架构层面改动（存 DiffInput，渲染时计算），非简单参数透传。待确认 bug 后单独处理。
