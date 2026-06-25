# Inline Diff Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add inline unified diff view to Write/Edit tool results in the TUI message stream, with word-level highlighting.

**Architecture:** TUI pairs `ToolStart`/`ToolEnd` events (already carrying tool input JSON with file_path/old_string/new_string) to construct `DiffInput`, then passes it to a new `peri-widgets::diff` module for computation and rendering. Zero changes to peri-agent, peri-middlewares, or event types.

**Tech Stack:** `similar` crate (already in Cargo.lock), `ratatui` spans/styles, existing `Theme::diff_*` colors.

---

## File Structure

| File | Responsibility |
|------|---------------|
| `peri-widgets/Cargo.toml` | Add `similar` dependency |
| `peri-widgets/src/diff/mod.rs` | **New**: `DiffInput`, `DiffHunk`, `DiffLine`, `WordDiff` types + `compute_diff()` entry point |
| `peri-widgets/src/diff/mod_test.rs` | **New**: unit tests for diff computation |
| `peri-widgets/src/diff/renderer.rs` | **New**: `render_diff()` → `Vec<Line>` |
| `peri-widgets/src/diff/renderer_test.rs` | **New**: unit tests for rendering |
| `peri-widgets/src/lib.rs` | Export `diff` module |
| `peri-tui/src/ui/message_view/mod.rs` | Add `diff_view: Option<DiffInput>` to `ToolBlock` variant |
| `peri-tui/src/app/message_pipeline/reconcile.rs` | Construct `DiffInput` from `CompletedTool` when building `ToolBlock` |
| `peri-tui/src/app/message_pipeline/transform.rs` | `build_tool_start_vm()`: no diff_view yet (ToolStart has no result) |
| `peri-tui/src/ui/message_render.rs` | Render diff_view after result_lines in `ToolBlock` branch |
| `peri-tui/src/ui/message_view/mod.rs` | Update `PartialEq`/`Hash` for new `diff_view` field + `from_base_message_with_cwd` |

---

### Task 1: Add `similar` dependency to peri-widgets

**Files:**
- Modify: `peri-widgets/Cargo.toml`

- [ ] **Step 1: Add dependency**

In `peri-widgets/Cargo.toml`, add `similar` to `[dependencies]`:

```toml
similar = "2"
```

After the `rand = "0.10"` line.

- [ ] **Step 2: Verify build**

Run: `cargo build -p peri-widgets`
Expected: Build succeeds (similar is already in Cargo.lock from transitive deps)

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/Cargo.toml
git commit -m "chore(widgets): add similar dependency for diff computation"
```

---

### Task 2: Diff types and computation (`peri-widgets/src/diff/mod.rs`)

**Files:**
- Create: `peri-widgets/src/diff/mod.rs`
- Create: `peri-widgets/src/diff/mod_test.rs`
- Modify: `peri-widgets/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `peri-widgets/src/diff/mod_test.rs`:

```rust
use super::*;

#[test]
fn test_compute_diff_simple_edit() {
    let input = DiffInput {
        file_path: "main.rs".to_string(),
        old_content: "fn main() {\n    println!(\"old\");\n}\n".to_string(),
        new_content: "fn main() {\n    println!(\"new\");\n}\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    // 应该只有 1 个 hunk
    assert_eq!(result.hunks.len(), 1, "should have exactly 1 hunk");
    let hunk = &result.hunks[0];
    // 应该包含至少 1 个 Add 和 1 个 Remove
    let has_add = hunk.lines.iter().any(|l| matches!(l, DiffLine::Add { .. }));
    let has_remove = hunk.lines.iter().any(|l| matches!(l, DiffLine::Remove { .. }));
    assert!(has_add, "should have at least one Add line");
    assert!(has_remove, "should have at least one Remove line");
}

#[test]
fn test_compute_diff_new_file() {
    let input = DiffInput {
        file_path: "new.txt".to_string(),
        old_content: String::new(),
        new_content: "hello\nworld\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(result.is_new_file, "should be marked as new file");
    // 新文件的 hunk 中所有行应该是 Add
    for hunk in &result.hunks {
        for line in &hunk.lines {
            assert!(
                matches!(line, DiffLine::Add { .. } | DiffLine::HunkHeader { .. }),
                "new file should only have Add lines, got {:?}",
                line
            );
        }
    }
}

#[test]
fn test_compute_diff_identical_content() {
    let input = DiffInput {
        file_path: "same.txt".to_string(),
        old_content: "hello\n".to_string(),
        new_content: "hello\n".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(!result.hunks.is_empty() || result.is_empty(), "identical content should produce empty or no hunks");
}

#[test]
fn test_compute_diff_large_file_truncation() {
    // 超过 1MB 应该触发截断
    let big_old = "x".repeat(600_000);
    let big_new = "y".repeat(600_000);
    let input = DiffInput {
        file_path: "big.txt".to_string(),
        old_content: big_old,
        new_content: big_new,
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let result = compute_diff(&input);
    assert!(result.is_truncated, "large diff should be truncated");
}

#[test]
fn test_word_diff_basic() {
    let diff = compute_word_diff("hello world", "hello earth");
    assert!(!diff.segments.is_empty(), "word diff should produce segments");
    // 应该包含 unchanged "hello " 和 changed parts
    let has_unchanged = diff.segments.iter().any(|(_, t)| *t == DiffWordType::Unchanged);
    assert!(has_unchanged, "should have unchanged segment for 'hello '");
}

#[test]
fn test_diff_input_equality() {
    let a = DiffInput {
        file_path: "a.rs".to_string(),
        old_content: "old".to_string(),
        new_content: "new".to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let b = a.clone();
    assert_eq!(a, b, "cloned DiffInput should be equal");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p peri-widgets --lib -- diff::mod_test 2>&1 | head -20`
Expected: Compilation error — `compute_diff`, `DiffInput`, etc. not defined.

- [ ] **Step 3: Implement diff types and computation**

Create `peri-widgets/src/diff/mod.rs`:

```rust
use ratatui::text::Line;

use crate::theme::Theme;

// ── 常量 ──────────────────────────────────────────────────
/// 大文件截断阈值（old + new 合计字节数）
const MAX_DIFF_SIZE_BYTES: usize = 1_000_000; // 1 MB

// ── 类型定义 ──────────────────────────────────────────────

/// Diff 输入（TUI 从 ToolStart.input 构造）
#[derive(Debug, Clone, PartialEq)]
pub struct DiffInput {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
    pub is_new_file: bool,
    pub is_deleted_file: bool,
    pub is_binary: bool,
}

/// 单词级 diff 片段类型
#[derive(Debug, Clone, PartialEq)]
pub enum DiffWordType {
    Unchanged,
    Added,
    Removed,
}

/// 单词级 diff 结果
#[derive(Debug, Clone, PartialEq)]
pub struct WordDiff {
    pub segments: Vec<(String, DiffWordType)>,
}

/// Diff 行类型
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    Context {
        text: String,
        old_line_num: u32,
        new_line_num: u32,
    },
    Add {
        text: String,
        line_num: u32,
        word_diff: Option<WordDiff>,
    },
    Remove {
        text: String,
        line_num: u32,
        word_diff: Option<WordDiff>,
    },
    HunkHeader {
        text: String,
    },
}

/// 结构化 Hunk
#[derive(Debug, Clone, PartialEq)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

/// Diff 计算结果
#[derive(Debug, Clone, PartialEq)]
pub struct DiffResult {
    pub hunks: Vec<DiffHunk>,
    pub is_new_file: bool,
    pub is_deleted_file: bool,
    pub is_binary: bool,
    pub is_truncated: bool,
}

impl DiffResult {
    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }
}

// ── 计算逻辑 ──────────────────────────────────────────────

/// 主入口：DiffInput → DiffResult
pub fn compute_diff(input: &DiffInput) -> DiffResult {
    // 大文件截断
    if input.old_content.len() + input.new_content.len() > MAX_DIFF_SIZE_BYTES {
        return DiffResult {
            hunks: Vec::new(),
            is_new_file: input.is_new_file,
            is_deleted_file: input.is_deleted_file,
            is_binary: input.is_binary,
            is_truncated: true,
        };
    }

    if input.is_binary {
        return DiffResult {
            hunks: Vec::new(),
            is_new_file: false,
            is_deleted_file: false,
            is_binary: true,
            is_truncated: false,
        };
    }

    let diff = similar::TextDiff::from_lines(&input.old_content, &input.new_content);
    let mut hunks: Vec<DiffHunk> = Vec::new();

    for hunk in diff.unified_diff().iter_hunks() {
        let mut lines: Vec<DiffLine> = Vec::new();
        let header = format!("{}", hunk);
        lines.push(DiffLine::HunkHeader {
            text: header,
        });

        let mut old_line = hunk.old_range().start;
        let mut new_line = hunk.new_range().start();

        for change in hunk.changes() {
            match change.tag() {
                similar::ChangeTag::Equal => {
                    lines.push(DiffLine::Context {
                        text: change.to_string_lossy().to_string(),
                        old_line_num: old_line,
                        new_line_num: new_line,
                    });
                    old_line += 1;
                    new_line += 1;
                }
                similar::ChangeTag::Delete => {
                    lines.push(DiffLine::Remove {
                        text: change.to_string_lossy().to_string(),
                        line_num: old_line,
                        word_diff: None,
                    });
                    old_line += 1;
                }
                similar::ChangeTag::Insert => {
                    lines.push(DiffLine::Add {
                        text: change.to_string_lossy().to_string(),
                        line_num: new_line,
                        word_diff: None,
                    });
                    new_line += 1;
                }
            }
        }

        // 单词级 diff：配对连续的 Remove+Add 行
        fill_word_diffs(&mut lines);

        hunks.push(DiffHunk {
            old_start: hunk.old_range().start,
            old_lines: hunk.old_range().end - hunk.old_range().start,
            new_start: hunk.new_range().start,
            new_lines: hunk.new_range().end - hunk.new_range().start,
            lines,
        });
    }

    DiffResult {
        hunks,
        is_new_file: input.is_new_file,
        is_deleted_file: input.is_deleted_file,
        is_binary: false,
        is_truncated: false,
    }
}

/// 为连续的 Remove+Add 行对填充单词级 diff
fn fill_word_diffs(lines: &mut Vec<DiffLine>) {
    // 收集需要配对的索引对
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if matches!(&lines[i], DiffLine::Remove { .. }) {
            let remove_idx = i;
            let mut j = i + 1;
            while j < lines.len() && matches!(&lines[j], DiffLine::Remove { .. }) {
                j += 1;
            }
            // j 现在指向 Remove 块后的第一个非 Remove 行
            let mut add_start = j;
            while add_start < lines.len() && matches!(&lines[add_start], DiffLine::Add { .. }) {
                pairs.push((remove_idx, add_start));
                remove_idx += 1;
                add_start += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }

    for (rm_idx, add_idx) in pairs {
        let rm_text = match &lines[rm_idx] {
            DiffLine::Remove { text, .. } => text.clone(),
            _ => continue,
        };
        let add_text = match &lines[add_idx] {
            DiffLine::Add { text, .. } => text.clone(),
            _ => continue,
        };

        let word_diff = compute_word_diff(&rm_text, &add_text);
        // 40% 阈值检查：如果变化词超过 40%，跳过单词级 diff（避免视觉噪声）
        let total_chars: usize = word_diff.segments.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        let changed_chars: usize = word_diff
            .segments
            .iter()
            .filter(|(_, t)| *t != DiffWordType::Unchanged)
            .map(|(s, _)| s.len())
            .sum();
        let ratio = if total_chars > 0 {
            changed_chars as f32 / total_chars as f32
        } else {
            0.0
        };

        if ratio < 0.4 {
            // 设置 Remove 的 word_diff
            if let DiffLine::Remove { word_diff: ref mut wd, .. } = lines[rm_idx] {
                *wd = Some(word_diff.clone());
            }
            // 设置 Add 的 word_diff
            if let DiffLine::Add { word_diff: ref mut wd, .. } = lines[add_idx] {
                *wd = Some(word_diff);
            }
        }
    }
}

/// 计算两行文本的单词级 diff
pub fn compute_word_diff(old_line: &str, new_line: &str) -> WordDiff {
    let diff = similar::TextDiff::from_words(old_line, new_line);
    let mut segments: Vec<(String, DiffWordType)> = Vec::new();

    for change in diff.iter_all_changes() {
        let text = change.to_string_lossy().to_string();
        let tag = match change.tag() {
            similar::ChangeTag::Equal => DiffWordType::Unchanged,
            similar::ChangeTag::Delete => DiffWordType::Removed,
            similar::ChangeTag::Insert => DiffWordType::Added,
        };
        segments.push((text, tag));
    }

    WordDiff { segments }
}

// ── 渲染入口（Task 3 实现）───────────────────────────────

/// 渲染 diff 为 ratatui Lines
pub fn render_diff(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>> {
    let _ = (input, width, theme);
    Vec::new() // 占位，Task 3 填充
}

#[cfg(test)]
mod tests {
    include!("mod_test.rs");
}
```

Update `peri-widgets/src/lib.rs` — add after `pub mod tool_call;`:

```rust
pub mod diff;
```

And add to the re-exports section:

```rust
pub use diff::{DiffInput, DiffHunk, DiffLine, DiffResult, DiffWordType, WordDiff};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p peri-widgets --lib -- diff::mod_test`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/diff/ peri-widgets/src/lib.rs
git commit -m "feat(widgets): add diff computation module with types, compute_diff, and word-level diff"
```

---

### Task 3: Diff rendering (`peri-widgets/src/diff/renderer.rs`)

**Files:**
- Create: `peri-widgets/src/diff/renderer.rs`
- Create: `peri-widgets/src/diff/renderer_test.rs`
- Modify: `peri-widgets/src/diff/mod.rs` (implement `render_diff` body, add `pub mod renderer`)

- [ ] **Step 1: Write failing renderer tests**

Create `peri-widgets/src/diff/renderer_test.rs`:

```rust
use ratatui::style::Color;

use super::*;
use crate::diff::{compute_diff, DiffInput, DiffLine};
use crate::theme::DarkTheme;

fn make_input(old: &str, new: &str) -> DiffInput {
    DiffInput {
        file_path: "test.rs".to_string(),
        old_content: old.to_string(),
        new_content: new.to_string(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    }
}

#[test]
fn test_render_diff_produces_lines() {
    let input = make_input("hello\n", "world\n");
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    assert!(!lines.is_empty(), "should produce at least one line");
}

#[test]
fn test_render_diff_hunk_header_is_cyan() {
    let input = make_input("old\n", "new\n");
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 查找 hunk header 行（包含 "@@"）
    let hunk_line = lines.iter().find(|l| {
        l.spans.iter().any(|s| s.content.contains("@@"))
    });
    assert!(hunk_line.is_some(), "should have hunk header");
    // hunk header 的第一个 span 颜色应该是 cyan
    let hunk = hunk_line.unwrap();
    let first_span = &hunk.spans[0];
    assert_eq!(
        first_span.style.fg,
        Some(Color::Cyan),
        "hunk header should be cyan"
    );
}

#[test]
fn test_render_diff_add_line_is_green() {
    let input = make_input("old line\n", "new line\n");
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 查找 "+" 开头的行
    let add_line = lines.iter().find(|l| {
        l.spans.iter().any(|s| s.content.starts_with('+'))
    });
    assert!(add_line.is_some(), "should have add line with + prefix");
}

#[test]
fn test_render_diff_remove_line_is_red() {
    let input = make_input("old line\n", "new line\n");
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    let remove_line = lines.iter().find(|l| {
        l.spans.iter().any(|s| s.content.starts_with('-'))
    });
    assert!(remove_line.is_some(), "should have remove line with - prefix");
}

#[test]
fn test_render_new_file_all_green() {
    let input = DiffInput {
        file_path: "new.txt".to_string(),
        old_content: String::new(),
        new_content: "line1\nline2\n".to_string(),
        is_new_file: true,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 所有非 hunk-header 行应该有绿色内容
    let non_header_lines: Vec<_> = lines.iter()
        .filter(|l| !l.spans.iter().any(|s| s.content.contains("@@")))
        .collect();
    assert!(non_header_lines.len() >= 2, "new file should have at least 2 content lines");
}

#[test]
fn test_render_truncated_diff() {
    let input = DiffInput {
        file_path: "big.txt".to_string(),
        old_content: "x".repeat(600_000),
        new_content: "y".repeat(600_000),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: false,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    // 截断 diff 应该只有摘要行
    assert!(lines.len() <= 3, "truncated diff should be short, got {} lines", lines.len());
    let combined: String = lines.iter().flat_map(|l| l.spans.iter().map(|s| s.content.clone())).collect();
    assert!(combined.contains("too large"), "truncated diff should mention 'too large'");
}

#[test]
fn test_render_binary_file() {
    let input = DiffInput {
        file_path: "image.png".to_string(),
        old_content: String::new(),
        new_content: String::new(),
        is_new_file: false,
        is_deleted_file: false,
        is_binary: true,
    };
    let theme = DarkTheme;
    let lines = render_diff_impl(&input, 80, &theme);
    let combined: String = lines.iter().flat_map(|l| l.spans.iter().map(|s| s.content.clone())).collect();
    assert!(combined.contains("Binary"), "binary file should show Binary message");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p peri-widgets --lib -- diff::renderer_test 2>&1 | head -20`
Expected: Compilation error — `render_diff_impl` not defined.

- [ ] **Step 3: Implement renderer**

Create `peri-widgets/src/diff/renderer.rs`:

```rust
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::theme::Theme;

use super::{compute_diff, DiffInput, DiffLine, DiffWordType};

/// 渲染 diff 为 ratatui Lines（公开入口，被 mod.rs 的 render_diff 委托调用）
pub fn render_diff_impl(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>> {
    let result = compute_diff(input);

    if result.is_binary {
        return vec![Line::from(Span::styled(
            format!("  Binary file {} - cannot display diff", input.file_path),
            Style::default().fg(theme.dim()),
        ))];
    }

    if result.is_truncated {
        return vec![Line::from(Span::styled(
            format!("  Diff too large for {} - changes not displayed", input.file_path),
            Style::default().fg(theme.dim()),
        ))];
    }

    if result.is_empty() && !result.is_new_file && !result.is_deleted_file {
        return Vec::new();
    }

    // 计算 gutter 宽度
    let max_line_num = result
        .hunks
        .iter()
        .flat_map(|h| {
            h.lines.iter().filter_map(|l| match l {
                DiffLine::Context { new_line_num, .. } => Some(*new_line_num),
                DiffLine::Add { line_num, .. } => Some(*line_num),
                DiffLine::Remove { line_num, .. } => Some(*line_num),
                DiffLine::HunkHeader { .. } => None,
            })
        })
        .max()
        .unwrap_or(1);
    let gutter_digits = max_line_num.to_string().len().max(1);
    // marker(1) + old_num(N) + space(1) + new_num(N) + separator(3 " │ ")
    let gutter_width = 1 + gutter_digits + 1 + gutter_digits + 3;

    let mut lines: Vec<Line<'static>> = Vec::new();

    // 文件标题行
    let prefix = if result.is_new_file { "+" } else if result.is_deleted_file { "-" } else { " " };
    lines.push(Line::from(Span::styled(
        format!("{} {}", prefix, input.file_path),
        Style::default().fg(if result.is_new_file {
            theme.diff_add()
        } else if result.is_deleted_file {
            theme.diff_remove()
        } else {
            theme.muted()
        }),
    )));

    for hunk in &result.hunks {
        for diff_line in &hunk.lines {
            match diff_line {
                DiffLine::HunkHeader { text } => {
                    lines.push(Line::from(Span::styled(
                        text.to_string(),
                        Style::default().fg(theme.diff_hunk()),
                    )));
                }
                DiffLine::Context {
                    text,
                    old_line_num,
                    new_line_num,
                } => {
                    let gutter = format!(
                        " {:>width$} {:>width$} │ ",
                        old_line_num,
                        new_line_num,
                        width = gutter_digits,
                    );
                    // 截断内容到可用宽度
                    let content = truncate_to_width(text.trim_end_matches('\n'), width.saturating_sub(gutter_width));
                    lines.push(Line::from(vec![
                        Span::styled(gutter, Style::default().fg(theme.dim())),
                        Span::raw(content),
                    ]));
                }
                DiffLine::Add {
                    text,
                    line_num,
                    word_diff,
                } => {
                    let gutter = format!(
                        "+{:>width$} {:>width$} │ ",
                        "",
                        line_num,
                        width = gutter_digits,
                    );
                    let content_text = text.trim_end_matches('\n');
                    let content_spans = if let Some(wd) = word_diff {
                        render_word_diff_spans(wd, theme.diff_add(), content_text)
                    } else {
                        vec![Span::styled(
                            truncate_to_width(content_text, width.saturating_sub(gutter_width)),
                            Style::default().fg(theme.diff_add()),
                        )]
                    };
                    let mut spans = vec![Span::styled(gutter, Style::default().fg(theme.diff_add()))];
                    spans.extend(content_spans);
                    lines.push(Line::from(spans));
                }
                DiffLine::Remove {
                    text,
                    line_num,
                    word_diff,
                } => {
                    let gutter = format!(
                        "-{:>width$} {:>width$} │ ",
                        line_num,
                        "",
                        width = gutter_digits,
                    );
                    let content_text = text.trim_end_matches('\n');
                    let content_spans = if let Some(wd) = word_diff {
                        render_word_diff_spans(wd, theme.diff_remove(), content_text)
                    } else {
                        vec![Span::styled(
                            truncate_to_width(content_text, width.saturating_sub(gutter_width)),
                            Style::default().fg(theme.diff_remove()),
                        )]
                    };
                    let mut spans = vec![Span::styled(gutter, Style::default().fg(theme.diff_remove()))];
                    spans.extend(content_spans);
                    lines.push(Line::from(spans));
                }
            }
        }
    }

    lines
}

/// 渲染单词级 diff 的 spans（在 Add/Remove 行内高亮变化片段）
fn render_word_diff_spans(
    wd: &super::WordDiff,
    base_color: Color,
    _fallback_text: &str,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (text, tag) in &wd.segments {
        let style = match tag {
            DiffWordType::Unchanged => Style::default().fg(base_color),
            DiffWordType::Added | DiffWordType::Removed => {
                Style::default().fg(base_color).add_modifier(Modifier::BOLD)
            }
        };
        spans.push(Span::styled(text.clone(), style));
    }
    spans
}

/// 字符级截断（CJK 安全）
fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    let full_width = UnicodeWidthStr::width(s);
    if full_width <= max_width {
        return s.to_string();
    }
    let mut result = String::new();
    let mut current_width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > max_width {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }
    result
}

#[cfg(test)]
mod tests {
    include!("renderer_test.rs");
}
```

Update `peri-widgets/src/diff/mod.rs` — add `pub mod renderer;` after the imports, and replace the `render_diff` stub:

```rust
pub mod renderer;
```

Replace the `render_diff` stub body with:

```rust
pub fn render_diff(input: &DiffInput, width: usize, theme: &dyn Theme) -> Vec<Line<'static>> {
    renderer::render_diff_impl(input, width, theme)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p peri-widgets --lib -- diff`
Expected: All tests PASS (6 mod_test + 7 renderer_test = 13 total)

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/diff/
git commit -m "feat(widgets): add diff renderer with gutter, hunk headers, word-level highlighting"
```

---

### Task 4: Add `diff_view` field to `MessageViewModel::ToolBlock`

**Files:**
- Modify: `peri-tui/src/ui/message_view/mod.rs`

- [ ] **Step 1: Add field to ToolBlock variant**

In `peri-tui/src/ui/message_view/mod.rs`, find the `ToolBlock` variant of `MessageViewModel` enum (around line 36-46) and add the `diff_view` field:

```rust
    ToolBlock {
        #[allow(dead_code)]
        tool_name: String,
        tool_call_id: String,
        display_name: String,
        args_display: Option<String>,
        content: String,
        is_error: bool,
        collapsed: bool,
        color: Color,
        /// 内嵌 diff 视图（Write/Edit 工具执行成功后填充）
        diff_view: Option<peri_widgets::DiffInput>,
    },
```

- [ ] **Step 2: Update PartialEq for ToolBlock**

Find the `PartialEq` match arm for `ToolBlock` (around line 100-116). Add `diff_view` comparison:

```rust
            (
                MessageViewModel::ToolBlock {
                    tool_name: a_name,
                    tool_call_id: a_tc,
                    args_display: a_args,
                    content: a_content,
                    is_error: a_err,
                    diff_view: a_diff,
                    ..
                },
                MessageViewModel::ToolBlock {
                    tool_name: b_name,
                    tool_call_id: b_tc,
                    args_display: b_args,
                    content: b_content,
                    is_error: b_err,
                    diff_view: b_diff,
                    ..
                },
            ) => {
                a_name == b_name
                    && a_tc == b_tc
                    && a_args == b_args
                    && a_content == b_content
                    && a_err == b_err
                    && a_diff == b_diff
            }
```

- [ ] **Step 3: Update Hash for ToolBlock**

Find the `Hash` match arm for `ToolBlock` (around line 207-218). Add `diff_view` hashing:

```rust
            MessageViewModel::ToolBlock {
                tool_name,
                tool_call_id,
                display_name,
                args_display,
                content,
                is_error,
                collapsed,
                diff_view,
                ..
            } => {
                2u8.hash(state);
                tool_name.hash(state);
                tool_call_id.hash(state);
                display_name.hash(state);
                args_display.hash(state);
                content.hash(state);
                is_error.hash(state);
                collapsed.hash(state);
                diff_view.hash(state);
            }
```

- [ ] **Step 4: Update all ToolBlock construction sites**

Every place that constructs `MessageViewModel::ToolBlock { ... }` must now include `diff_view: None`.

Search for all `MessageViewModel::ToolBlock {` in the codebase and add `diff_view: None,` to each. Key locations:

1. `peri-tui/src/ui/message_view/mod.rs` — `from_base_message_with_cwd()` (around line 544)
2. `peri-tui/src/ui/message_view/mod.rs` — `tool_block_with_id()` (around line 662)
3. `peri-tui/src/app/message_pipeline/transform.rs` — `build_tool_start_vm()` (around line 124)
4. `peri-tui/src/app/message_pipeline/reconcile.rs` — completed tools loop (around line 179)

Each gets `diff_view: None,` added as the last field.

- [ ] **Step 5: Verify build**

Run: `cargo build -p peri-tui`
Expected: Build succeeds (all ToolBlock construction sites updated)

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/ui/message_view/mod.rs peri-tui/src/app/message_pipeline/
git commit -m "feat(tui): add diff_view field to ToolBlock MessageViewModel"
```

---

### Task 5: Construct `DiffInput` from tool input in pipeline

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/reconcile.rs`
- Modify: `peri-tui/src/app/message_pipeline/transform.rs`

- [ ] **Step 1: Add helper function to construct DiffInput**

In `peri-tui/src/app/message_pipeline/reconcile.rs`, add a helper function at the top of the file (after imports):

```rust
/// 从工具名和入参构造 DiffInput（仅 Write/Edit 工具）
fn try_build_diff_input(name: &str, input: &serde_json::Value) -> Option<peri_widgets::DiffInput> {
    match name {
        "Edit" => {
            let old_string = input.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new_string = input.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            let file_path = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if old_string.is_empty() || file_path.is_empty() {
                return None;
            }
            Some(peri_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: old_string.to_string(),
                new_content: new_string.to_string(),
                is_new_file: false,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        "Write" => {
            let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let file_path = input.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if content.is_empty() || file_path.is_empty() {
                return None;
            }
            // Write 工具无法获取旧内容，标记为 is_new_file
            // 如果文件已存在，TUI 不碰文件系统，降级为纯文本
            Some(peri_widgets::DiffInput {
                file_path: file_path.to_string(),
                old_content: String::new(),
                new_content: content.to_string(),
                is_new_file: true,
                is_deleted_file: false,
                is_binary: false,
            })
        }
        _ => None,
    }
}
```

- [ ] **Step 2: Use helper when building ToolBlock in reconcile**

In the same file, find the completed tools loop that builds `MessageViewModel::ToolBlock` (the `for ct in &self.completed_tools` block). Add the diff_view computation:

```rust
        for ct in &self.completed_tools {
            let display = tool_display::format_tool_name(&ct.name);
            let args = tool_display::format_tool_args(&ct.name, &ct.input, Some(&self.cwd));
            // 构造 diff_view（仅成功执行的 Write/Edit）
            let diff_view = if !ct.is_error {
                try_build_diff_input(&ct.name, &ct.input)
            } else {
                None
            };
            tail_vms.push(MessageViewModel::ToolBlock {
                tool_name: ct.name.clone(),
                tool_call_id: ct.tool_call_id.clone(),
                display_name: display,
                args_display: args,
                content: ct.output.clone(),
                is_error: ct.is_error,
                collapsed: true,
                color: if ct.is_error {
                    theme::ERROR
                } else {
                    tool_color(&ct.name)
                },
                diff_view,
            });
        }
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p peri-tui`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/message_pipeline/reconcile.rs
git commit -m "feat(tui): construct DiffInput from Write/Edit tool input in pipeline reconcile"
```

---

### Task 6: Render diff_view in ToolBlock display

**Files:**
- Modify: `peri-tui/src/ui/message_render.rs`

- [ ] **Step 1: Add diff rendering to ToolBlock branch**

In `peri-tui/src/ui/message_render.rs`, find the `MessageViewModel::ToolBlock { ... }` match arm (around line 245). After the destructured fields, add `diff_view` to the pattern:

```rust
        MessageViewModel::ToolBlock {
            collapsed,
            display_name,
            args_display,
            content,
            color: _color,
            is_error,
            tool_name,
            diff_view,
            ..
        } => {
```

Then, at the end of this match arm, just before the closing `lines` return — after the `result_lines` rendering block and `error_summary_lines` block — add the diff rendering:

```rust
            // 内嵌 diff 视图
            if let Some(ref diff_input) = diff_view {
                let diff_lines = peri_widgets::diff::render_diff(
                    diff_input,
                    80, // 使用默认宽度，实际渲染会由 ratatui 处理
                    &crate::ui::theme::DarkTheme,
                );
                lines.extend(diff_lines);
            }

            lines
        }
```

Note: `message_render.rs` uses `crate::ui::theme` module (not `peri_widgets::Theme`), which provides color constants. For `render_diff` we need a `Theme` implementor — `DarkTheme` from `peri_widgets` is the concrete type. If the TUI's theme system is different, adjust to use a theme instance that's accessible.

- [ ] **Step 2: Verify build and run**

Run: `cargo build -p peri-tui`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/message_render.rs
git commit -m "feat(tui): render inline diff view in ToolBlock for Write/Edit results"
```

---

### Task 7: Integration test — ToolStart/ToolEnd → diff display

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

- [ ] **Step 1: Write integration test**

Add a new test to `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`:

```rust
#[test]
fn test_edit_tool_end_produces_diff_view() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // 模拟 AI 开始工具调用
    let _ = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "I'll edit the file".to_string(),
        source_agent_id: None,
    });
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_edit_1".to_string(),
        name: "Edit".to_string(),
        display: "Edit".to_string(),
        args: "file_path: /tmp/main.rs".to_string(),
        input: serde_json::json!({
            "file_path": "/tmp/main.rs",
            "old_string": "fn main() {\n    println!(\"old\");\n}",
            "new_string": "fn main() {\n    println!(\"new\");\n}"
        }),
        source_agent_id: None,
    });
    // 模拟工具执行完成（成功）
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_edit_1".to_string(),
        name: "Edit".to_string(),
        output: "Replaced text to main.rs".to_string(),
        is_error: false,
        source_agent_id: None,
    });
    // 模拟 StateSnapshot 包含结果
    pipeline.handle_state_snapshot(vec![
        BaseMessage::human("edit the file"),
        BaseMessage::ai_with_tool_calls("tc_edit_1", "Edit", serde_json::json!({
            "file_path": "/tmp/main.rs",
            "old_string": "fn main() {\n    println!(\"old\");\n}",
            "new_string": "fn main() {\n    println!(\"new\");\n}"
        })),
        BaseMessage::tool_result("tc_edit_1", "Replaced text to main.rs"),
    ]);
    // 构建 tail_vms 并验证 diff_view
    let tail_vms = pipeline.build_tail_vms(0);
    let tool_block = tail_vms.iter().find(|vm| {
        matches!(vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "Edit")
    });
    assert!(tool_block.is_some(), "should find Edit ToolBlock in tail_vms");
    if let MessageViewModel::ToolBlock { diff_view, .. } = tool_block.unwrap() {
        assert!(diff_view.is_some(), "Edit ToolBlock should have diff_view after successful ToolEnd");
        let di = diff_view.as_ref().unwrap();
        assert_eq!(di.file_path, "/tmp/main.rs");
        assert_eq!(di.old_content, "fn main() {\n    println!(\"old\");\n}");
        assert_eq!(di.new_content, "fn main() {\n    println!(\"new\");\n}");
        assert!(!di.is_new_file);
    }
}

#[test]
fn test_non_edit_tool_no_diff_view() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_bash_1".to_string(),
        name: "Bash".to_string(),
        display: "Bash".to_string(),
        args: "ls -la".to_string(),
        input: serde_json::json!({"command": "ls -la"}),
        source_agent_id: None,
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_bash_1".to_string(),
        name: "Bash".to_string(),
        output: "file1.txt\nfile2.txt".to_string(),
        is_error: false,
        source_agent_id: None,
    });
    pipeline.handle_state_snapshot(vec![
        BaseMessage::human("list files"),
        BaseMessage::ai_with_tool_calls("tc_bash_1", "Bash", serde_json::json!({"command": "ls -la"})),
        BaseMessage::tool_result("tc_bash_1", "file1.txt\nfile2.txt"),
    ]);
    let tail_vms = pipeline.build_tail_vms(0);
    let tool_block = tail_vms.iter().find(|vm| {
        matches!(vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "Bash")
    });
    if let Some(MessageViewModel::ToolBlock { diff_view, .. }) = tool_block {
        assert!(diff_view.is_none(), "Bash tool should NOT have diff_view");
    }
}

#[test]
fn test_error_tool_end_no_diff_view() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let _ = pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc_edit_err".to_string(),
        name: "Edit".to_string(),
        display: "Edit".to_string(),
        args: "".to_string(),
        input: serde_json::json!({
            "file_path": "/tmp/nonexist.rs",
            "old_string": "not found",
            "new_string": "replacement"
        }),
        source_agent_id: None,
    });
    let _ = pipeline.handle_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_edit_err".to_string(),
        name: "Edit".to_string(),
        output: "Error: old_string not found".to_string(),
        is_error: true,
        source_agent_id: None,
    });
    pipeline.handle_state_snapshot(vec![
        BaseMessage::human("edit"),
        BaseMessage::ai_with_tool_calls("tc_edit_err", "Edit", serde_json::json!({
            "file_path": "/tmp/nonexist.rs",
            "old_string": "not found",
            "new_string": "replacement"
        })),
        BaseMessage::tool_error("tc_edit_err", "Error: old_string not found"),
    ]);
    let tail_vms = pipeline.build_tail_vms(0);
    let tool_block = tail_vms.iter().find(|vm| {
        matches!(vm, MessageViewModel::ToolBlock { tool_name, .. } if tool_name == "Edit")
    });
    if let Some(MessageViewModel::ToolBlock { diff_view, .. }) = tool_block {
        assert!(diff_view.is_none(), "error ToolEnd should NOT have diff_view");
    }
}
```

Note: These tests may require adapting to the actual API of `handle_state_snapshot` and `build_tail_vms`. Check the existing test helpers in `message_pipeline_test.rs` for the exact method signatures and adjust accordingly. The `handle_state_snapshot` method name may be `restore_completed` or similar — look at the existing tests for the pattern.

- [ ] **Step 2: Run tests**

Run: `cargo test -p peri-tui --lib -- message_pipeline::message_pipeline_test`
Expected: All existing + 3 new tests PASS

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/message_pipeline/message_pipeline_test.rs
git commit -m "test(tui): add integration tests for diff_view in Edit/Write ToolBlock"
```

---

### Task 8: Full build verification

**Files:** None (verification only)

- [ ] **Step 1: Full workspace build**

Run: `cargo build`
Expected: All crates build successfully

- [ ] **Step 2: Full test suite**

Run: `cargo test -p peri-widgets -p peri-tui`
Expected: All tests pass

- [ ] **Step 3: Clippy check**

Run: `cargo clippy -p peri-widgets -p peri-tui -- -D warnings 2>&1 | head -30`
Expected: No warnings (or only pre-existing ones)

- [ ] **Step 4: Manual TUI test**

Run: `cargo run -p peri-tui` then ask the agent to edit a file. Verify:
- Edit tool result shows inline diff with green/red highlighting
- Write tool result shows all-green "new file" diff
- Other tools (Bash, Read) show no diff
- Collapsed/expanded toggle works for ToolBlock with diff
