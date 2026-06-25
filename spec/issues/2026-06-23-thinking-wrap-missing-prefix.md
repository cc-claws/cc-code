# thinking 渲染换行后续行缺少前缀缩进

**状态**：Open
**优先级**：中
**创建日期**：2026-06-23

## 问题描述

reasoning/thinking 内容渲染时，每个 markdown 逻辑行会加 4 列前缀（`  ⎿ ` / `    `）。但当段落文本较长触发 `Paragraph::wrap` 自动换行后，换行产生的续行**没有 4 列前缀缩进**，顶到终端最左边，视觉上与正文混在一起。

根因：`parse_markdown(text, content_width)` 的 `max_width` 只影响表格列宽计算，**不影响普通段落文本**。pulldown-cmark 解析 markdown 时不做 word wrap，长段落返回一个超长的 `Line`。加了 4 列前缀后超过终端宽度，`Paragraph::wrap` 折行后续行缺少缩进。

## 症状详情

```
终端宽度 80 列时的实际表现：

∴ Thought for 500 chars (ctrl+o to expand)
  ⎿ 这是一个很长的思考过程第一段，内容会一直延伸超过终端宽度
    然后被自动换行到了这里 ← 这行应该有 4 个空格缩进，但没有
实际的显示效果是：

∴ Thought for 500 chars (ctrl+o to expand)
  ⎿ 这是一个很长的思考过程第一段，内容会一直延伸超过终端宽度
然后被自动换行到了这里 ← 顶到最左边，缺少缩进
```

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 使用 thinking 模式（如 GLM-5.2 / DeepSeek 等支持 reasoning 的模型）
  2. 发送一个需要较长思考的问题
  3. thinking 内容段落超过终端宽度时触发换行
- **环境**：任何终端宽度，窄屏更明显

## 涉及文件

- `peri-tui/src/ui/message_render.rs:548-591` — reasoning 渲染逻辑（`ContentBlockView::Reasoning` 分支）
- `peri-tui/src/ui/render_thread.rs:133-169` — `build_wrap_map`（折行预测，本 bug 不需改动）
- `peri-widgets/src/markdown/render_state.rs:313-377` — `wrap_cell_text`（已有 word wrap 算法参考）

---

## 修复方案（计划A：wrap_line_spans）

### 方案对比

| | A. wrap_line_spans | B. 纯文本 wrap | C. render_state 预折行 |
|---|---|---|---|
| 代码量 | ~80 行 | ~30 行 | ~30 行 |
| 保留 span 样式 | ✅ 完整 | ❌ 丢失高亮色相 | ✅ |
| 影响范围 | 仅 reasoning | 仅 reasoning | **所有 markdown** |
| 风险 | 低 | 极低 | 中（需全量回归） |
| 可单元测试 | ✅ | ✅ | 难 |

**选定 A**。`dim_markdown_lines`（message_render.rs:15-37）刻意保留语法高亮色相（有 fg 的 span → 保留色相 + DIM 修饰），B 方案丢失高亮违反既有设计意图。C 方案影响面太大。

### 安全性不变式

```
wrap_line_spans(line, content_width)  →  每行 ≤ content_width (= width - 4)
加 4 列前缀后                          →  每行 ≤ width
build_wrap_map(lines, width)           →  line_count = 1，不二次折行
```

`wrap_map` / `viewport_clip`（二分裁剪）/ 选区高亮（`visual_to_logical`）/ 滚动条全部自动正确，零改动。

VM 层 `width` 与渲染线程 `self.width` 同源（`RenderTask` 内 `messages_to_view_models(width)` 和 `build_wrap_map(lines, width)` 用同一 width），保证一致。

### 前置条件验证

| 依赖 | 状态 |
|------|------|
| `unicode-width` crate | ✅ `peri-tui/Cargo.toml:50` 已有 `unicode-width = "0.2"` |
| `UnicodeWidthChar` trait | ✅ 项目中已广泛使用（`main_ui/mod.rs:416` 等 16 处） |
| `Style: PartialEq` | ✅ `ratatui-core-0.1.0/src/style.rs:103` `#[derive(..., PartialEq, ...)]` |
| `Line<'static>` 所有权转移 | ✅ `dimmed.into_iter()` 已消费 spans |

### 算法：flatten → 贪心折行 → reassemble

```
fn wrap_line_spans(line: Line<'static>, max_width: usize) -> Vec<Line<'static>>

  [快速路径] total_width ≤ max_width → return vec![line]

  [1] flatten: spans → Vec<(char, Style)>
      每个 char 携带所属 span 的 style
      span 边界消失，断行可发生在 span 内部

  [2] 贪心折行 (while pos < len):
      a. 逐 char 累积 width (UnicodeWidthChar, CJK=2)
         超宽时 break: content_end > pos 保证至少推进 1 字符（防死循环）
      b. 单词边界优先: 从 content_end 往回找最后一个 whitespace
         找到 → 在空格处断（丢弃空格）
         没找到 → 硬断（CJK 场景，逐字断行正确）
      c. trim 行首行尾空白
      d. 推进 pos，跳过断行点后的连续空白

  [3] reassemble: Vec<(char,Style)> → Line
      相邻同 Style 的 char 合并为一个 Span
      输出 span 数 ≤ 输入字符数（高效）
```

### 完整实现代码

```rust
use unicode_width::UnicodeWidthChar;

/// 将一个含多 span 的 Line 按视觉宽度折行，保留各 span 样式。
///
/// 算法：flatten → 贪心宽度折行（单词边界优先）→ reassemble。
/// 用于 reasoning 渲染：确保每行宽度 ≤ max_width，加前缀后不触发 Paragraph::wrap 二次折行。
fn wrap_line_spans(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 || line.spans.is_empty() {
        return vec![line];
    }

    // [1] flatten: spans → Vec<(char, Style)>
    let flat: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();

    // [快速路径] 不超宽原样返回
    let total_width: usize = flat.iter().map(|(c, _)| c.width().unwrap_or(0)).sum();
    if total_width <= max_width {
        return vec![line];
    }

    // [2] 贪心折行
    let mut result: Vec<Line<'static>> = Vec::new();
    let mut pos = 0;
    while pos < flat.len() {
        let mut cur_width = 0usize;
        let mut content_end = pos;
        for i in pos..flat.len() {
            let cw = flat[i].0.width().unwrap_or(0);
            if content_end > pos && cur_width + cw > max_width {
                break;
            }
            cur_width += cw;
            content_end = i + 1;
        }

        // 单词边界优先
        let mut break_at = content_end;
        for i in (pos..content_end).rev() {
            if flat[i].0.is_whitespace() {
                break_at = i;
                break;
            }
        }

        // trim 行首行尾空白
        let mut seg_start = pos;
        while seg_start < break_at && flat[seg_start].0.is_whitespace() {
            seg_start += 1;
        }
        let mut seg_end = break_at;
        while seg_end > seg_start && flat[seg_end - 1].0.is_whitespace() {
            seg_end -= 1;
        }

        // [3] reassemble
        if seg_start < seg_end {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut cur_text = String::new();
            let mut cur_style = flat[seg_start].1;
            for &(ch, st) in &flat[seg_start..seg_end] {
                if st == cur_style {
                    cur_text.push(ch);
                } else {
                    spans.push(Span::styled(std::mem::take(&mut cur_text), cur_style));
                    cur_text = ch.to_string();
                    cur_style = st;
                }
            }
            if !cur_text.is_empty() {
                spans.push(Span::styled(cur_text, cur_style));
            }
            result.push(Line::from(spans));
        }

        // 推进 pos
        pos = break_at;
        while pos < flat.len() && flat[pos].0.is_whitespace() {
            pos += 1;
        }
    }

    if result.is_empty() {
        vec![Line::default()]
    } else {
        result
    }
}
```

### 调用点改动（message_render.rs:579-585）

```rust
// 改前
for (i, mut line) in dimmed.into_iter().enumerate() {
    let prefix = if i == 0 { "  ⎿ " } else { "    " };
    let mut spans = vec![Span::styled(prefix, Style::default().fg(theme::DIM))];
    spans.append(&mut line.spans);
    lines.push(Line::from(spans));
}

// 改后
for (i, line) in dimmed.into_iter().enumerate() {
    for (j, wline) in wrap_line_spans(line, content_width).into_iter().enumerate() {
        let prefix = if i == 0 && j == 0 { "  ⎿ " } else { "    " };
        let mut spans = vec![Span::styled(prefix, Style::default().fg(theme::DIM))];
        spans.extend(wline.spans);
        lines.push(Line::from(spans));
    }
}
```

关键：`i == 0 && j == 0` — 只有第一个逻辑行的第一个视觉行用 `⎿`。

### 边界条件

| 场景 | 处理 |
|------|------|
| 空行 | 返回 `vec![Line::default()]` |
| 单 char 宽度 > max_width | 强制放入（防死循环） |
| CJK 无空格长串 | 硬断，逐字推进 |
| 中英混排 | CJK 逐字断，ASCII 空格断 |
| span 跨越断行点 | flatten 消除边界，reassemble 重新合并 |
| 零宽字符 | `width().unwrap_or(0)` = 0，不触发 break |

### 性能分析

| 指标 | 分析 |
|------|------|
| 时间复杂度 | O(n)，n = 字符数 |
| 典型 reasoning | 200-2000 字符，< 0.1ms |
| 快速路径 | 90%+ 行不超宽，直接返回原 line |
| MarkdownCache | 无影响——wrap 在缓存之后执行 |

### 单元测试（message_render_test.rs）

```
test_wrap_line_spans_no_wrap              短行原样返回
test_wrap_line_spans_ascii_word_boundary  在空格处断行
test_wrap_line_spans_cjk_per_char         纯 CJK 逐字断
test_wrap_line_spans_cjk_mixed_ascii      中英混排
test_wrap_line_spans_span_boundary_split  单词跨 span，样式保留
test_wrap_line_spans_single_char_exceeds  单字符超 max_width 不死循环
test_wrap_line_spans_preserves_style      多 span 不同样式各自保留
test_wrap_line_spans_empty_line           空行
```

### 不受影响

- `dim_markdown_lines`：签名不变，在 wrap 前调用
- `parse_markdown` / `MarkdownCache`：不变
- `build_wrap_map` / `viewport_clip` / 选区高亮：自动适配
- UserBubble / ToolBlock / AssistantBubble Text block：不经过 `wrap_line_spans`

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-23 | — | Open | agent | 创建，含完整修复方案（计划A） |

## 修复记录

（待实现）
