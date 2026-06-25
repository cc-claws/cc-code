# 流式 Markdown 表格 Holdback 机制 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在流式 Markdown 渲染中添加表格 holdback 机制，检测以 `|` 开头的行，在表格行不完整时保持 holdback 不提交渲染，直到行完整或流结束后批量提交。消除流式过程中表格列错位和列宽闪烁。

**Architecture:** 新建 `TableHoldbackScanner` 结构体，位于 `peri-tui/src/ui/markdown/table_holdback.rs`。集成点在 `ensure_rendered_incremental` 内——在增量解析前用 scanner 判断哪些尾部文本需要 holdback。`HoldbackDecision` 枚举控制提交/保留/flush 语义。不修改 `peri-widgets` 的表格渲染逻辑。

**Tech Stack:** Rust, pulldown-cmark (markdown 解析), ratatui (Text/Line 渲染)

**Issue:** `spec/issues/2026-05-30-table-holdback-during-streaming.md`

---

### Task 1: 定义 `HoldbackDecision` 枚举和 `TableHoldbackScanner` 结构体

纯数据结构定义，无外部依赖。

**Files:**
- Create: `peri-tui/src/ui/markdown/table_holdback.rs`
- Test: `peri-tui/src/ui/markdown/table_holdback_test.rs`

- [ ] **Step 1: 创建 `table_holdback.rs` 文件，定义核心类型**

```rust
//! 流式 Markdown 表格 Holdback 扫描器
//!
//! 在增量渲染过程中，检测 Markdown 表格行并判断是否应 holdback（暂不提交渲染）。
//! 表格行以 `|` 开头，列数由表头决定。流式过程中不完整的行（列数不足或行未以 `|` 或换行结束）
//! 保持 holdback 状态，直到行完整或流结束。

/// Holdback 决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldbackDecision {
    /// 全部提交渲染，无 holdback
    Commit,
    /// 从 `holdback_offset` 开始到文本末尾 holdback，只提交 offset 之前的内容
    Hold {
        holdback_offset: usize,
    },
    /// 流结束，强制提交所有 holdback 内容（包括不完整的表格）
    FlushAll,
}

/// Markdown 表格行扫描器状态
#[derive(Debug, Clone, Default)]
enum TableScanState {
    /// 不在表格模式中
    #[default]
    Idle,
    /// 已看到表头行，期望分隔行
    HeaderSeen {
        /// 表头列数
        col_count: usize,
    },
    /// 在表格数据区域中
    InTable {
        /// 表格列数（由表头决定）
        col_count: usize,
    },
}

/// 流式 Markdown 表格 holdback 扫描器
///
/// 维护跨多次 `scan()` 调用的表格检测状态。每次调用传入当前完整的 raw 文本，
/// scanner 判断尾部是否存在不完整的表格行需要 holdback。
#[derive(Debug, Clone, Default)]
pub struct TableHoldbackScanner {
    /// 当前扫描状态
    state: TableScanState,
    /// 上次 scan 的文本长度（用于增量扫描）
    last_scan_len: usize,
    /// 是否处于流式状态（streaming=true 时启用 holdback；false 时总是 FlushAll）
    streaming: bool,
}

impl TableHoldbackScanner {
    /// 创建新的扫描器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置流式状态
    pub fn set_streaming(&mut self, streaming: bool) {
        self.streaming = streaming;
    }

    /// 扫描文本，决定 holdback 策略
    ///
    /// `text` 是当前 ContentBlockView::Text 的完整 raw 文本。
    /// 返回 HoldbackDecision 告知调用方如何处理。
    pub fn scan(&mut self, text: &str) -> HoldbackDecision {
        // 非流式模式总是全部提交
        if !self.streaming {
            self.last_scan_len = text.len();
            return HoldbackDecision::FlushAll;
        }

        // 增量扫描：只处理新增部分
        if text.len() <= self.last_scan_len {
            // 无新内容
            return HoldbackDecision::Commit;
        }

        let new_text = &text[self.last_scan_len..];
        self.last_scan_len = text.len();

        // 逐行扫描新增文本
        self.process_new_text(text, new_text)
    }

    /// 重置扫描器状态（新消息或 finalize 时调用）
    pub fn reset(&mut self) {
        self.state = TableScanState::Idle;
        self.last_scan_len = 0;
    }
}

/// 辅助函数：计算 `|` 分隔的列数
///
/// 对于 `"| a | b | c |"` 返回 3。
/// 对于 `"| a | b | c"` 也返回 3（尾部 `|` 可选）。
/// 非 `|` 开头的行返回 0。
fn count_pipe_columns(line: &str) -> usize {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return 0;
    }
    // 去掉首尾 `|` 后按 `|` 分隔
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    if inner.is_empty() {
        return 0;
    }
    inner.split('|').count()
}

/// 判断行是否为表格分隔行（如 `|---|---|`）
fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return false;
    }
    // 去掉首尾 `|`
    let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
    // 每个单元格只含 `-`、`:`、空格
    inner.split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty()
            && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
            && c.contains('-')
    })
}

/// 判断行是否以 `|` 开头（可能是表格行）
fn is_table_line(line: &str) -> bool {
    line.trim().starts_with('|')
}

/// 判断文本末尾的行是否完整（以换行或 `|` 结尾）
fn is_line_complete(text: &str) -> bool {
    let trimmed = text.trim_end();
    trimmed.is_empty() || trimmed.ends_with('\n') || trimmed.ends_with('|')
}

/// 判断表格行是否列数完整
fn is_row_complete(line: &str, expected_cols: usize) -> bool {
    let cols = count_pipe_columns(line);
    cols > 0 && cols >= expected_cols
}

impl TableHoldbackScanner {
    /// 处理新增文本，更新状态并返回决策
    fn process_new_text(&self, full_text: &str, new_text: &str) -> HoldbackDecision {
        // 策略：从 full_text 末尾回溯，检查是否存在未完成的表格
        //
        // 简化实现：从 full_text 尾部开始分析
        // 1. 如果最后非空行不是以 `|` 开头 → Commit（无表格）
        // 2. 如果最后行以 `|` 开头但不完整 → Holdback
        // 3. 如果最后行完整且是表格行 → Commit（但可能影响下一行）

        // 收集尾部连续的表格行
        let lines: Vec<&str> = full_text.lines().collect();
        if lines.is_empty() {
            return HoldbackDecision::Commit;
        }

        // 从末尾向前找连续的表格行
        let mut table_end = lines.len();
        let mut table_start = lines.len();

        // 找到末尾连续表格行的起始位置
        for i in (0..lines.len()).rev() {
            if is_table_line(lines[i]) || is_separator_row(lines[i]) {
                table_start = i;
            } else {
                break;
            }
        }

        // 末尾没有表格行
        if table_start == lines.len() {
            return HoldbackDecision::Commit;
        }

        // 检查文本末尾是否以换行结束（行是否完整）
        let last_line_complete = full_text.ends_with('\n');

        // 如果最后一行以 `\n` 结束，说明所有行都完整
        // 如果最后一行不以 `\n` 结束，说明最后一行可能不完整

        if !last_line_complete {
            // 最后一行未结束，检查它是否是表格行
            let last_line = lines.last().unwrap();
            if is_table_line(last_line) {
                // 不完整的表格行，需要 holdback
                // 计算 holdback 起始位置：从未完成行的开始
                let holdback_offset = self.find_line_start_offset(full_text, lines.len() - 1);
                return HoldbackDecision::Hold { holdback_offset };
            }
        }

        // 所有行都完整（以 `\n` 结尾或最后不是表格行）
        // 但需要检查：最后一个表格行是否列数完整
        // 找到表头列数
        let header_col_count = self.find_header_col_count(&lines[table_start..table_end]);

        if header_col_count > 0 {
            // 检查每个数据行是否列数足够
            for line in &lines[table_start..table_end] {
                if is_separator_row(line) {
                    continue;
                }
                if is_table_line(line) && !is_row_complete(line, header_col_count) {
                    // 有不完整的行，holdback 从该行开始
                    let line_idx = lines.iter().position(|&l| l == *line).unwrap_or(0);
                    let holdback_offset = self.find_line_start_offset(full_text, line_idx);
                    return HoldbackDecision::Hold { holdback_offset };
                }
            }
        }

        HoldbackDecision::Commit
    }

    /// 在完整文本中找到第 line_idx 行的字节起始偏移
    fn find_line_start_offset(&self, text: &str, line_idx: usize) -> usize {
        let mut offset = 0;
        for (i, line) in text.lines().enumerate() {
            if i == line_idx {
                // 跳过前导换行符
                while offset > 0 && text.as_bytes().get(offset - 1) == Some(&b'\n') {
                    // offset 已经在行首
                }
                return offset;
            }
            offset += line.len();
            if offset < text.len() && text.as_bytes()[offset] == b'\n' {
                offset += 1;
            }
        }
        offset
    }

    /// 从表格行片段中找到表头的列数
    fn find_header_col_count(&self, table_lines: &[&str]) -> usize {
        for line in table_lines {
            if !is_separator_row(line) && is_table_line(line) {
                return count_pipe_columns(line);
            }
        }
        0
    }
}
```

- [ ] **Step 2: 创建测试文件 `table_holdback_test.rs`**

```rust
use super::*;

#[test]
fn test_count_pipe_columns_basic() {
    assert_eq!(count_pipe_columns("| a | b | c |"), 3);
    assert_eq!(count_pipe_columns("| a | b | c"), 3);
    assert_eq!(count_pipe_columns("| single |"), 1);
    assert_eq!(count_pipe_columns("no pipes"), 0);
    assert_eq!(count_pipe_columns(""), 0);
}

#[test]
fn test_count_pipe_columns_with_spaces() {
    assert_eq!(count_pipe_columns("|  Name  |  Value  |"), 2);
}

#[test]
fn test_is_separator_row() {
    assert!(is_separator_row("|---|---|"));
    assert!(is_separator_row("|------|-------|"));
    assert!(is_separator_row("|:---:|:---:|"));
    assert!(is_separator_row("|---|:---:|---:|"));
    assert!(!is_separator_row("| a | b |"));
    assert!(!is_separator_row("not a separator"));
    assert!(!is_separator_row(""));
}

#[test]
fn test_is_table_line() {
    assert!(is_table_line("| a | b |"));
    assert!(is_table_line("|---|---|"));
    assert!(is_table_line("  | a | b |")); // 前导空格
    assert!(!is_table_line("not a table"));
    assert!(!is_table_line(""));
}

#[test]
fn test_holdback_decision_no_table() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let decision = scanner.scan("just some text\nwith no tables\n");
    assert_eq!(decision, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_decision_complete_table() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let table = "| A | B |\n|---|---|\n| 1 | 2 |\n";
    let decision = scanner.scan(table);
    assert_eq!(decision, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_decision_incomplete_row() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 第三行不完整：只有 1 列，表头有 2 列
    let table = "| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(table);
    // 应该 holdback 未完成的行
    match decision {
        HoldbackDecision::Hold { .. } => {} // pass
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_holdback_decision_incomplete_last_line_no_newline() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 表头行未完成（不以 \n 结尾）
    let table = "| A | B";
    let decision = scanner.scan(table);
    match decision {
        HoldbackDecision::Hold { .. } => {} // pass
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_holdback_flush_on_non_streaming() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(false);
    let table = "| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(table);
    assert_eq!(decision, HoldbackDecision::FlushAll);
}

#[test]
fn test_holdback_after_row_completed() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    // 第一轮：不完整的行
    let table1 = "| A | B |\n|---|---|\n| 1";
    let decision1 = scanner.scan(table1);
    assert!(matches!(decision1, HoldbackDecision::Hold { .. }));

    // 第二轮：行已完成
    let table2 = "| A | B |\n|---|---|\n| 1 | 2 |\n";
    scanner.last_scan_len = 0; // 重置后重新扫描完整文本
    let decision2 = scanner.scan(table2);
    assert_eq!(decision2, HoldbackDecision::Commit);
}

#[test]
fn test_holdback_table_after_paragraph() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    let text = "Some paragraph\n\n| A | B |\n|---|---|\n| 1";
    let decision = scanner.scan(text);
    match decision {
        HoldbackDecision::Hold { holdback_offset } => {
            // holdback_offset 应指向表格开始位置之后的不完整行
            assert!(holdback_offset > 0, "holdback_offset 应大于 0");
        }
        other => panic!("期望 Hold，实际: {:?}", other),
    }
}

#[test]
fn test_reset_clears_state() {
    let mut scanner = TableHoldbackScanner::new();
    scanner.set_streaming(true);
    scanner.scan("| A | B |\n|---|---|\n| 1");
    scanner.reset();
    assert_eq!(scanner.last_scan_len, 0);
}
```

- [ ] **Step 3: 在 `mod.rs` 中注册模块**

在 `peri-tui/src/ui/markdown/mod.rs` 添加模块声明：

```rust
mod table_holdback;
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-tui --lib -- table_holdback
```

预期：全部 PASS

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/ui/markdown/table_holdback.rs peri-tui/src/ui/markdown/table_holdback_test.rs peri-tui/src/ui/markdown/mod.rs
git commit -m "feat(markdown): add TableHoldbackScanner for streaming table detection

Introduces HoldbackDecision enum and TableHoldbackScanner struct that
detects incomplete Markdown table rows during streaming. When a table
row has fewer columns than the header, the scanner returns Hold to
defer rendering until the row is complete.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: 将 `TableHoldbackScanner` 集成到 `ensure_rendered_incremental`

在增量渲染路径中集成 holdback 逻辑。核心思路：在 `ensure_rendered_incremental` 中，对 `ContentBlockView::Text` 的 raw 文本执行 holdback 扫描，如果需要 holdback 则只渲染 holdback_offset 之前的部分。

**Files:**
- Modify: `peri-tui/src/ui/markdown/mod.rs`（集成 holdback 逻辑）
- Modify: `peri-tui/src/ui/message_view/mod.rs`（在 `ContentBlockView::Text` 中添加 scanner 字段）
- Test: `peri-tui/src/ui/markdown/mod_test.rs`（新增表格流式渲染测试）

- [ ] **Step 1: 在 `ContentBlockView::Text` 中添加 `holdback_scanner` 字段**

修改 `peri-tui/src/ui/message_view/mod.rs` 第 339-349 行：

```rust
pub enum ContentBlockView {
    /// 文本内容（含 markdown 解析缓存）
    Text {
        raw: String,
        rendered: Text<'static>,
        dirty: bool,
        /// 已渲染到 `raw` 的字节偏移（增量解析用）
        rendered_prefix_len: usize,
        /// `rendered` 中对应前缀的行数（避免重解析计数）
        rendered_prefix_lines: usize,
        /// 流式表格 holdback 扫描器
        holdback_scanner: crate::ui::markdown::table_holdback::TableHoldbackScanner,
    },
    // ... Reasoning, ToolUse 不变
```

**注意**：`TableHoldbackScanner` 的 `PartialEq` 和 `Hash` 实现需要考虑——由于 scanner 不参与内容比较，可以在 `PartialEq` 和 `Hash` impl 中忽略该字段。现有实现已只比较 `raw` 和 `dirty`，`holdback_scanner` 字段不在比较范围内，无需修改。

- [ ] **Step 2: 更新所有创建 `ContentBlockView::Text` 的位置**

需要搜索所有 `ContentBlockView::Text { ... }` 构造点并添加 `holdback_scanner` 字段。使用 `Default::default()` 初始化。

位置列表（`peri-tui/src/ui/message_view/mod.rs`）：

1. **第 457 行** `from_base_message_with_cwd` 中 `ContentBlock::Text` → 添加 `holdback_scanner: Default::default()`
2. **第 470 行** `ContentBlock::Image` 回退 → 添加 `holdback_scanner: Default::default()`
3. **第 481 行** `ContentBlock::ToolResult` success → 添加 `holdback_scanner: Default::default()`
4. **第 494 行** `ContentBlock::ToolResult` error → 添加 `holdback_scanner: Default::default()`
5. **第 503 行** `_` 回退 → 添加 `holdback_scanner: Default::default()`

位置列表（`peri-tui/src/ui/message_view/mod.rs` 的 `append_chunk`）：

6. **第 642 行** `append_chunk` 中创建新 Text block → 添加 `holdback_scanner: Default::default()`

位置列表（`peri-tui/src/app/message_pipeline/transform.rs`）：

7. **第 30 行** `build_streaming_bubble` 中创建 Text block → 此处需要设置 `streaming: true`：

```rust
let mut scanner = crate::ui::markdown::table_holdback::TableHoldbackScanner::new();
scanner.set_streaming(true);
blocks.push(ContentBlockView::Text {
    raw: self.current_ai_text.clone(),
    rendered,
    dirty: false,
    rendered_prefix_len: self.current_ai_text.len(),
    rendered_prefix_lines,
    holdback_scanner: scanner,
});
```

位置列表（`peri-tui/src/ui/markdown/mod_test.rs`）：

8-13. 所有测试中的 `ContentBlockView::Text { ... }` 构造点都需要添加 `holdback_scanner: Default::default()`。

- [ ] **Step 3: 实现 holdback 感知的 `ensure_rendered_incremental`**

修改 `peri-tui/src/ui/markdown/mod.rs` 中的 `ensure_rendered_incremental` 函数。核心变更：在增量解析前，使用 scanner 决定 holdback 范围，只渲染到 holdback 截止点。

```rust
pub fn ensure_rendered_incremental(block: &mut ContentBlockView, max_width: usize) {
    if let ContentBlockView::Text {
        raw,
        rendered,
        dirty,
        rendered_prefix_len,
        rendered_prefix_lines,
        holdback_scanner,
    } = block
    {
        if !*dirty || raw.len() == *rendered_prefix_len {
            return;
        }

        // 表格 holdback 检查
        let decision = holdback_scanner.scan(raw);

        // 确定实际可渲染的文本范围
        let effective_end = match &decision {
            HoldbackDecision::Hold { holdback_offset } => {
                // 只渲染到 holdback 位置
                let offset = (*holdback_offset).min(raw.len());
                // 不要回退到已渲染的前面
                offset.max(*rendered_prefix_len)
            }
            HoldbackDecision::Commit | HoldbackDecision::FlushAll => raw.len(),
        };

        if effective_end <= *rendered_prefix_len {
            // 没有新内容可渲染（全部被 holdback）
            return;
        }

        // 根据实际渲染范围决定渲染策略
        let text_to_render = &raw[..effective_end];
        let effective_prefix_len = *rendered_prefix_len;

        let last_stable_boundary = find_last_block_boundary(raw, effective_prefix_len)
            .min(effective_end);

        if last_stable_boundary == effective_prefix_len {
            // 路径 1：前文稳定，只解析新增部分
            let new_text = &text_to_render[effective_prefix_len..];
            if !new_text.is_empty() {
                let new_lines = parse_markdown(new_text, max_width);
                for line in new_lines.lines {
                    rendered.lines.push(line);
                }
            }
        } else if last_stable_boundary > 0 {
            // 路径 2：有不稳定块，保留前缀，重解析 boundary 之后
            let reparse_text = &text_to_render[last_stable_boundary..];
            let new_lines = parse_markdown(reparse_text, max_width);
            rendered.lines.truncate(*rendered_prefix_lines);
            if *rendered_prefix_lines > 0 && last_stable_boundary < effective_prefix_len {
                let full_new = parse_markdown(&text_to_render[last_stable_boundary..], max_width);
                rendered.lines.truncate(0);
                for line in full_new.lines {
                    rendered.lines.push(line);
                }
            } else {
                for line in new_lines.lines {
                    rendered.lines.push(line);
                }
            }
        } else {
            // 路径 3：全量重解析
            *rendered = parse_markdown(text_to_render, max_width);
        }

        *rendered_prefix_len = effective_end;
        *rendered_prefix_lines = rendered.lines.len();

        // FlushAll 时重置 scanner（流结束）
        if matches!(decision, HoldbackDecision::FlushAll) {
            holdback_scanner.reset();
        }

        *dirty = false;
    }
}
```

**关键设计决策**：
- `rendered_prefix_len` 追踪的是实际渲染到的位置（可能小于 `raw.len()`）
- 当 holdback 生效时，`dirty` 仍被设为 false（因为本次已处理到 effective_end）
- 下一次 chunk 到达时，`raw` 增长但 `rendered_prefix_len` 没变，`dirty=true` 会触发新的扫描
- `FlushAll`（非流式模式）确保历史恢复和最终状态不受影响

- [ ] **Step 4: 添加表格流式渲染测试到 `mod_test.rs`**

```rust
#[test]
fn test_ensure_rendered_incremental_table_holdback_incomplete() {
    // 模拟流式输入：表头完整，数据行不完整
    let mut block = ContentBlockView::Text {
        raw: "| A | B |\n|---|---|\n| 1".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(true);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);

    // 应该渲染了表头和分隔行，但 holdback 了不完整的数据行
    // rendered_prefix_len 应小于 raw.len()
    let prefix_len = get_prefix_len(&block);
    assert!(
        prefix_len < "| A | B |\n|---|---|\n| 1".len(),
        "不完整的表格行应被 holdback，prefix_len={}, raw.len()={}",
        prefix_len,
        "| A | B |\n|---|---|\n| 1".len()
    );
}

#[test]
fn test_ensure_rendered_incremental_table_complete() {
    // 完整表格：不应 holdback
    let mut block = ContentBlockView::Text {
        raw: "| A | B |\n|---|---|\n| 1 | 2 |\n".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(true);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);
    assert_eq!(
        get_prefix_len(&block),
        "| A | B |\n|---|---|\n| 1 | 2 |\n".len(),
        "完整表格不应 holdback"
    );
}

#[test]
fn test_ensure_rendered_incremental_table_streaming_then_complete() {
    // 第一步：流式输入不完整的表格
    let mut block = ContentBlockView::Text {
        raw: "| H1 | H2 |\n|----|----|\n| da".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(true);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);
    let prefix_after_first = get_prefix_len(&block);
    assert!(
        prefix_after_first < "| H1 | H2 |\n|----|----|\n| da".len(),
        "第一步：数据行不完整应 holdback"
    );

    // 第二步：补全数据行
    append_to_block(&mut block, "ta | val |\n");
    set_dirty(&mut block, true);
    ensure_rendered_incremental(&mut block, 80);
    let full_text = "| H1 | H2 |\n|----|----|\n| data | val |\n";
    assert_eq!(
        get_prefix_len(&block),
        full_text.len(),
        "第二步：数据行完整后应全部渲染"
    );
}

#[test]
fn test_ensure_rendered_incremental_non_table_no_holdback() {
    // 非 `|` 开头的普通文本不应 holdback
    let mut block = ContentBlockView::Text {
        raw: "Just some text without tables\nSecond line\n".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(true);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);
    assert_eq!(
        get_prefix_len(&block),
        "Just some text without tables\nSecond line\n".len(),
        "非表格文本不应 holdback"
    );
}

#[test]
fn test_ensure_rendered_incremental_table_flush_on_non_streaming() {
    // 非流式模式不应 holdback
    let mut block = ContentBlockView::Text {
        raw: "| A | B |\n|---|---|\n| 1".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(false);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);
    assert_eq!(
        get_prefix_len(&block),
        "| A | B |\n|---|---|\n| 1".len(),
        "非流式模式不应 holdback"
    );
}
```

- [ ] **Step 5: 运行全部相关测试**

```bash
cargo test -p peri-tui --lib -- table_holdback
cargo test -p peri-tui --lib -- test_ensure_rendered_incremental
cargo test -p peri-tui --lib -- test_md_table
```

预期：全部 PASS

- [ ] **Step 6: 构建验证**

```bash
cargo build -p peri-tui
```

预期：编译通过，无 warning。如果有 `#[allow(dead_code)]` 需要清理。

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/ui/markdown/mod.rs peri-tui/src/ui/markdown/table_holdback.rs peri-tui/src/ui/markdown/table_holdback_test.rs peri-tui/src/ui/markdown/mod_test.rs peri-tui/src/ui/message_view/mod.rs
git commit -m "feat(markdown): integrate TableHoldbackScanner into incremental rendering

ensure_rendered_incremental now consults the holdback scanner before
rendering. Incomplete table rows are held back during streaming,
preventing column misalignment and width flickering. When the row
completes or streaming ends, all held content is flushed.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: 处理流结束时的 flush 语义

确保流结束时（`is_streaming` 变为 false，或 `done()`/`finalize` 路径），所有 holdback 内容被提交渲染。

**Files:**
- Modify: `peri-tui/src/ui/render_thread.rs`（在 `render_one` 中处理 streaming → finalized 的过渡）
- Modify: `peri-tui/src/ui/markdown/mod.rs`（添加 `ensure_rendered_flush` 函数）
- Test: `peri-tui/src/ui/markdown/mod_test.rs`

- [ ] **Step 1: 添加 `ensure_rendered_flush` 函数**

在 `peri-tui/src/ui/markdown/mod.rs` 中添加：

```rust
/// 强制提交所有 holdback 内容（流结束时调用）
///
/// 将 scanner 设为非流式模式，然后执行一次完整渲染。
pub fn ensure_rendered_flush(block: &mut ContentBlockView, max_width: usize) {
    if let ContentBlockView::Text {
        holdback_scanner,
        dirty,
        raw,
        ..
    } = block
    {
        // 如果 raw 为空，无需处理
        if raw.is_empty() {
            return;
        }
        // 切换到非流式模式（触发 FlushAll）
        holdback_scanner.set_streaming(false);
        // 标记 dirty 以确保渲染发生
        *dirty = true;
        ensure_rendered_incremental(block, max_width);
    }
}
```

- [ ] **Step 2: 在 `render_one` 中处理 finalized 状态**

当 `is_streaming` 从 `true` 变为 `false` 时，需要 flush holdback。修改 `render_thread.rs` 中 `render_one` 的逻辑：

在现有的 `ensure_rendered_incremental` 调用前，检查是否已 finalized：

```rust
if let MessageViewModel::AssistantBubble { blocks, is_streaming } = vm {
    for block in blocks.iter_mut() {
        if *is_streaming {
            ensure_rendered_incremental(block, width);
        } else {
            ensure_rendered_flush(block, width);
        }
    }
}
```

**注意**：这里的关键是 `is_streaming` 字段。当 `build_streaming_bubble` 构建时 `is_streaming=true`，当 `done()` 调用 `finalize_current_ai()` 后消息通过 `messages_to_view_models` 重建，此时创建的 `ContentBlockView::Text` 中 `holdback_scanner` 的 `streaming=false`（默认值），所以自然走 `FlushAll` 路径。

对于 `append_chunk` 路径，chunk 到达时 `is_streaming=true`，渲染时走 holdback 路径。当 `Done` 事件到达后 `finalize_current_ai()` 被调用，下一次 `build_tail_vms()` 时如果 `has_streaming_content()=true` 仍会构建 streaming bubble；如果 `has_snapshot_this_round=true` 则走 reconcile 路径，从 `completed` 重建 VM（scanner 默认 `streaming=false`）。

- [ ] **Step 3: 添加 flush 测试**

```rust
#[test]
fn test_ensure_rendered_flush_releases_holdback() {
    // 先以 streaming 模式创建，有 holdback
    let mut block = ContentBlockView::Text {
        raw: "| A | B |\n|---|---|\n| 1".to_string(),
        rendered: Text::raw(""),
        dirty: true,
        rendered_prefix_len: 0,
        rendered_prefix_lines: 0,
        holdback_scanner: {
            let mut s = TableHoldbackScanner::new();
            s.set_streaming(true);
            s
        },
    };
    ensure_rendered_incremental(&mut block, 80);
    let held_prefix = get_prefix_len(&block);
    assert!(held_prefix < "| A | B |\n|---|---|\n| 1".len());

    // flush 释放所有 holdback
    ensure_rendered_flush(&mut block, 80);
    assert_eq!(
        get_prefix_len(&block),
        "| A | B |\n|---|---|\n| 1".len(),
        "flush 应提交所有 holdback 内容"
    );
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-tui --lib -- test_ensure_rendered
```

预期：全部 PASS

- [ ] **Step 5: 构建验证**

```bash
cargo build -p peri-tui
```

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/ui/markdown/mod.rs peri-tui/src/ui/render_thread.rs peri-tui/src/ui/markdown/mod_test.rs
git commit -m "feat(markdown): add flush semantics for table holdback on stream end

ensure_rendered_flush forces all held-back content to render when
streaming ends. render_one now checks is_streaming to decide between
incremental (holdback-aware) and flush rendering paths.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: 全量构建和测试验证

在所有变更合入后执行完整的构建和测试，确保没有回归。

**Files:** 无新增修改

- [ ] **Step 1: 全量构建**

```bash
cargo build
```

预期：所有 crate 编译通过

- [ ] **Step 2: 运行 TUI 相关测试**

```bash
cargo test -p peri-tui --lib
```

预期：全部 PASS

- [ ] **Step 3: 运行 Markdown 和表格相关测试**

```bash
cargo test -p peri-tui --lib -- markdown
cargo test -p peri-tui --lib -- table
cargo test -p peri-widgets --lib
```

预期：全部 PASS

- [ ] **Step 4: Lint 检查**

```bash
cargo clippy -p peri-tui --lib -- -D warnings
```

预期：无 warning

- [ ] **Step 5: 手动验证**

```bash
cargo run -p peri-tui
```

手动验证：
1. 让 LLM 生成一个 Markdown 表格，观察流式过程中表格是否平滑显示（无列错位闪烁）
2. 让 LLM 生成普通文本（不含表格），确认无影响
3. 恢复历史会话，确认表格显示正常（非流式模式，无 holdback）

- [ ] **Step 6: Final commit（如有 lint/测试修复）**

```bash
git add -A
git commit -m "chore: fix lint warnings from table holdback implementation

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Self-Review

### Spec coverage

| 需求 | Task |
|------|------|
| 检测以 `\|` 开头的行，标记为表格行 | Task 1（`is_table_line`） |
| 不完整行（列数少于表头）保持 holdback | Task 1（`HoldbackDecision::Hold`） |
| 行完整后批量提交 | Task 2（`ensure_rendered_incremental` 集成） |
| 流结束时批量提交 | Task 3（`ensure_rendered_flush`） |
| 表头检测、分隔行、数据行 | Task 1（`count_pipe_columns`/`is_separator_row`） |
| 与 `ensure_rendered_incremental` 的集成 | Task 2 Step 3 |
| `HoldbackDecision` 枚举 | Task 1 Step 1 |

### Placeholder scan

无 TBD/TODO/占位符。所有步骤包含具体代码或命令。

### Type consistency

- `ContentBlockView::Text` 新增 `holdback_scanner` 字段——所有构造点（7 处 + 测试 6 处）均已更新
- `HoldbackDecision` 是纯枚举，无泛型
- `TableHoldbackScanner` 实现 `Default`，与 `ContentBlockView` 的构造兼容
- `PartialEq` 和 `Hash` impl 不包含 `holdback_scanner` 字段，不影响 hash diff 和比较逻辑
- `rendered_prefix_len` 现在追踪的是 "实际渲染到的位置"（可能 < `raw.len()`），而非 "已扫描到的位置"。`dirty` 在 holdback 时仍设为 false（本次处理完毕），下次 chunk 增长 raw 时 `append_chunk` 设 `dirty=true` 触发新一轮扫描

### 边界情况

- 空 raw 文本：scanner 返回 `Commit`，无 holdback
- 只有 `\n` 的文本：`Commit`
- 表格行中间有非表格行（如段落分隔）：scanner 只看末尾连续的表格行，不影响已渲染的内容
- CJK 内容在表格中：`count_pipe_columns` 按 `|` 分隔，与内容编码无关
- 代码围栏内的 `|`：不会被误检测（代码围栏内容不以 `|` 开头，pulldown-cmark 不把代码块内容视为表格行）
- 多个表格：scanner 分析末尾的连续表格行，前面的表格不受影响（已被增量渲染提交）
