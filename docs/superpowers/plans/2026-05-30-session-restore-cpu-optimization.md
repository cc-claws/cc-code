# 会话恢复 CPU 暴涨优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除长上下文会话恢复（`-c`/`-r`）时的 CPU 短暂暴涨，通过三个独立优化减少约 60-70% 的恢复开销。

**Architecture:** 三个 P0/P1 级优化，各自独立可测：① wrap 计算去重（render_thread.rs），② Markdown 延迟解析（message_view/mod.rs + render_thread.rs），③ TUI 侧统一用 load_context（thread_ops.rs）。每个优化消除一个已确认的热点，不引入新抽象。

**Tech Stack:** Rust, ratatui (Text/Line/Paragraph), pulldown-cmark (markdown), sqlx (SQLite), tokio

**Issue:** `spec/issues/2026-05-30-cpu-spike-on-session-restore.md`

---

### Task 1: 消除 wrap 重复计算

`rebuild()` 中 `compute_wrapped_height()` 和 `build_wrap_map()` 对同一批 `lines` 各遍历一次调用 `Paragraph::line_count()`，完全重复。将两者合并为一次遍历。

**Files:**
- Modify: `peri-tui/src/ui/render_thread.rs:73-81`（删除 `compute_wrapped_height`）
- Modify: `peri-tui/src/ui/render_thread.rs:127-163`（`build_wrap_map` 返回元组）
- Modify: `peri-tui/src/ui/render_thread.rs:350-357`（`rebuild` 中拆解元组）
- Test: `peri-tui/src/ui/render_thread_test.rs`

- [ ] **Step 1: 修改 `build_wrap_map` 返回 `(usize, Vec<WrappedLineInfo>)`**

将 `build_wrap_map` 的返回类型改为元组，内部累加 `total_lines`。

修改 `peri-tui/src/ui/render_thread.rs` 第 127 行起的 `build_wrap_map`：

```rust
    fn build_wrap_map(lines: &[Line<'static>], width: u16) -> (usize, Vec<WrappedLineInfo>) {
        if width == 0 || lines.is_empty() {
            return (0, Vec::new());
        }
        let mut wrap_map = Vec::with_capacity(lines.len());
        let mut visual_row: u16 = 0;

        for (idx, line) in lines.iter().enumerate() {
            let plain_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            // 使用 grapheme 级别（与 ratatui WordWrapper 一致）
            let char_widths: Vec<u8> = plain_text
                .graphemes(true)
                .map(|g| unicode_width::UnicodeWidthStr::width(g) as u8)
                .collect();

            // 使用 ratatui 的 Paragraph::line_count 精确计算该行的视觉行数
            let visual_count = if plain_text.is_empty() {
                1
            } else {
                let text = ratatui::text::Text::from(line.clone());
                let count = Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .line_count(width);
                count.max(1) as u16
            };

            wrap_map.push(WrappedLineInfo {
                line_idx: idx,
                visual_row_start: visual_row,
                visual_row_end: visual_row + visual_count,
                plain_text,
                char_widths,
            });
            visual_row += visual_count;
        }

        (visual_row as usize, wrap_map)
    }
```

- [ ] **Step 2: 删除 `compute_wrapped_height` 函数**

删除 `peri-tui/src/ui/render_thread.rs` 第 71-81 行的 `compute_wrapped_height` 函数和注释。

即删除：
```rust
    /// 计算给定 lines 在指定宽度下 wrap 后的真实视觉行数。
    /// 使用 ratatui 的 Paragraph::line_count 与 Wrap{trim:false} 确保与实际渲染一致。
    fn compute_wrapped_height(lines: &[Line<'static>], width: u16) -> usize {
        if width == 0 || lines.is_empty() {
            return 0;
        }
        let text = ratatui::text::Text::from(lines.to_vec());
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(width)
    }
```

- [ ] **Step 3: 修改 `rebuild()` 中的调用**

将 `rebuild()` 函数末尾（第 350-357 行）改为拆解元组：

```rust
        let render_width = self.width;
        let (total_lines, wrap_map) = Self::build_wrap_map(&deduped, self.width);
        let mut cache = self.cache.write();
        cache.lines = deduped;
        cache.message_offsets = offsets;
        cache.total_lines = total_lines;
        cache.wrap_map = wrap_map;
        cache.width = self.width;
        cache.version += 1;
```

注意：`deduped` 在此之前已被消费到 `cache.lines`，所以 `build_wrap_map` 调用必须在 `cache.lines = deduped` 之前。因此实际顺序是先调用 `build_wrap_map(&deduped, ...)` 拿到结果，再赋值 cache。

- [ ] **Step 4: 更新所有测试中的 `build_wrap_map` 调用**

测试文件 `peri-tui/src/ui/render_thread_test.rs` 中 `test_build_wrap_map_*` 系列测试直接调用 `RenderTask::build_wrap_map()`，需要适配元组返回值。

以 `test_build_wrap_map_empty` 为例，所有 `build_wrap_map` 调用处改为解构：

```rust
#[test]
fn test_build_wrap_map_empty() {
    let (total, result) = RenderTask::build_wrap_map(&[], 80);
    assert!(result.is_empty());
    assert_eq!(total, 0);
}
```

同理更新：`test_build_wrap_map_single_short_line`、`test_build_wrap_map_single_long_line_wraps`、`test_build_wrap_map_cjk_char_width`、`test_build_wrap_map_multi_line_visual_rows`、`test_build_wrap_map_empty_line`。

每个测试增加 `let (total, result) = ...` 解构，并可选增加 `assert!(total > 0)` 等断言。

- [ ] **Step 5: 运行测试验证**

```bash
cargo test -p peri-tui --lib -- test_build_wrap_map
cargo test -p peri-tui --lib -- test_rebuild
cargo test -p peri-tui --lib -- test_resize
cargo test -p peri-tui --lib -- test_clear
```

预期：全部 PASS

- [ ] **Step 6: 构建验证**

```bash
cargo build -p peri-tui
```

预期：编译通过，无 warning

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/ui/render_thread.rs peri-tui/src/ui/render_thread_test.rs
git commit -m "perf(render): merge compute_wrapped_height into build_wrap_map to eliminate duplicate line_count traversal

Eliminates redundant Paragraph::line_count() traversal in rebuild().
Previously compute_wrapped_height() and build_wrap_map() each iterated
all lines calling Paragraph::line_count(); now merged into a single pass.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: Markdown 延迟解析（UserBubble + AssistantBubble TextBlock）

`from_base_message_with_cwd()` 在创建 VM 时就调用 `parse_markdown_default()`（宽度80），但 `render_one()` 又会用实际宽度重新解析 UserBubble。改为延迟解析——创建时不解析，渲染时才解析。

**Files:**
- Modify: `peri-tui/src/ui/message_view/mod.rs:442-468`（`from_base_message_with_cwd` 中 UserBubble + TextBlock）
- Modify: `peri-tui/src/ui/message_view/mod.rs:681-684`（`MessageViewModel::user()` helper）
- Modify: `peri-tui/src/ui/render_thread.rs:166-190`（`render_one` 无需改动，已有处理逻辑）
- Test: `peri-tui/src/ui/render_thread_test.rs`（现有测试验证 hash diff 和 rebuild）

- [ ] **Step 1: 修改 `from_base_message_with_cwd` 中 Human 消息路径**

将 `peri-tui/src/ui/message_view/mod.rs` 第 442-448 行从：

```rust
            BaseMessage::Human { content, .. } => {
                let raw = content.text_content();
                let rendered = parse_markdown_default(&raw);
                MessageViewModel::UserBubble {
                    content: raw,
                    rendered,
                }
            }
```

改为：

```rust
            BaseMessage::Human { content, .. } => {
                let raw = content.text_content();
                MessageViewModel::UserBubble {
                    content: raw,
                    rendered: Text::raw(""),
                }
            }
```

这样 UserBubble 的 `rendered` 在创建时为空，由 `render_one()` 第 179-184 行的实际宽度 `parse_markdown()` 填充。

- [ ] **Step 2: 修改 `from_base_message_with_cwd` 中 Ai 消息的 Text ContentBlock**

将第 460-469 行从：

```rust
                        ContentBlock::Text { text } => {
                            let rendered = parse_markdown_default(&text);
                            let rendered_prefix_lines = rendered.lines.len();
                            ContentBlockView::Text {
                                raw: text.clone(),
                                rendered,
                                dirty: false,
                                rendered_prefix_len: text.len(),
                                rendered_prefix_lines,
                            }
                        }
```

改为：

```rust
                        ContentBlock::Text { text } => {
                            ContentBlockView::Text {
                                raw: text.clone(),
                                rendered: Text::raw(""),
                                dirty: true,
                                rendered_prefix_len: 0,
                                rendered_prefix_lines: 0,
                            }
                        }
```

关键变更：`dirty: true` + `rendered_prefix_len: 0`。`render_one()` 第 174-176 行已调用 `ensure_rendered_incremental(block, width)`，它会检测 `dirty=true` 并用实际宽度全量解析（路径 3：`last_stable_boundary == 0` → `*rendered = parse_markdown(raw, max_width)`）。

- [ ] **Step 3: 修改 `MessageViewModel::user()` helper**

将第 681-684 行从：

```rust
    pub fn user(content: String) -> Self {
        let rendered = parse_markdown_default(&content);
        MessageViewModel::UserBubble { content, rendered }
    }
```

改为：

```rust
    pub fn user(content: String) -> Self {
        MessageViewModel::UserBubble {
            content,
            rendered: Text::raw(""),
        }
    }
```

- [ ] **Step 4: 运行测试验证**

现有测试已覆盖：
- `test_rebuild_hash_diff_skips_unchanged`：验证 hash diff 在无 rendered 内容时仍工作
- `test_rebuild_increments_version`：验证基本 rebuild 流程
- `test_build_wrap_map_*`：wrap 计算与 VM 内容无关

```bash
cargo test -p peri-tui --lib -- test_rebuild
cargo test -p peri-tui --lib -- test_build_wrap_map
cargo test -p peri-tui --lib -- render_thread
```

预期：全部 PASS。如果 hash 测试因空 rendered 导致 hash 不同而失败，需确认 `MessageViewModel` 的 `Hash` impl 是否包含 `rendered` 字段——如果不包含则无影响。

- [ ] **Step 5: 构建验证**

```bash
cargo build -p peri-tui
```

预期：编译通过。检查是否有 `parse_markdown_default` 的 unused import warning——如有，从 `message_view/mod.rs` 顶部移除该 import（其他调用点可能仍在使用，需检查）。

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/ui/message_view/mod.rs
git commit -m "perf(markdown): defer markdown parsing to render time in session restore

UserBubble and AssistantBubble TextBlocks no longer parse markdown at
VM creation time (width=80). Instead, render_one() parses with actual
terminal width, and ensure_rendered_incremental() handles TextBlocks
with dirty=true. Eliminates ~50% of redundant markdown parse calls
during session restore.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: TUI 侧统一用 `load_context()` 替代 `load_messages()`

`open_thread()` 调用 `store.load_messages()`（单线程消息），随后 ACP 侧 `load_session()` 再调用 `load_context()`（含祖先链+缓存）。改用 `load_context()` 让 TUI 侧也受益于 cached_context 缓存，且功能是超集。

**Files:**
- Modify: `peri-tui/src/app/thread_ops.rs:155-159`（`open_thread` 中 `load_messages` → `load_context`）
- Test: 手动测试恢复会话功能

- [ ] **Step 1: 修改 `open_thread()` 中的加载调用**

将 `peri-tui/src/app/thread_ops.rs` 第 155-159 行从：

```rust
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_messages(&tid))
                .unwrap_or_default()
        });
```

改为：

```rust
        let base_msgs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.load_context(&tid))
                .unwrap_or_default()
        });
```

- [ ] **Step 2: 验证 `load_context` 返回类型与后续代码兼容**

`load_context()` 和 `load_messages()` 都返回 `Vec<BaseMessage>`，后续代码（`origin_messages`、`messages_to_view_models`、`pipeline.restore_completed`）无需改动。

`load_context()` 的额外功能：
- 包含祖先链消息（通过 `resolve_ancestor_chain`）
- 先查 `cached_context` 缓存列，命中时跳过多条 SQL 查询
- 缓存未命中时走完整路径并更新缓存

**注意**：对于没有祖先链的普通会话（绝大多数情况），`load_context()` 退化为 `load_messages()` + 缓存读写，行为等价。对于有祖先链的会话（SubAgent fork 等），`load_context()` 返回更完整的上下文，这是正确的行为。

- [ ] **Step 3: 构建验证**

```bash
cargo build -p peri-tui
```

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/thread_ops.rs
git commit -m "perf(restore): use load_context instead of load_messages in open_thread

TUI side now benefits from cached_context and ancestor chain resolution.
ACP side load_session() will hit the cache populated by this call,
reducing total SQLite reads during session restore.

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: 全量构建和测试验证

在所有优化合入后执行完整的构建和测试，确保没有回归。

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

- [ ] **Step 3: 运行 agent 相关测试（load_context 变更）**

```bash
cargo test -p peri-agent --lib -- sqlite_store
```

预期：全部 PASS

- [ ] **Step 4: Lint 检查**

```bash
cargo clippy -p peri-tui --lib -- -D warnings
```

预期：无 warning

- [ ] **Step 5: 手动验证恢复功能**

```bash
cargo run -p peri-tui -- -c
```

手动验证：
1. 恢复旧会话后消息正确显示
2. 发送新 prompt 能正常对话
3. `/compact` 命令正常工作

- [ ] **Step 6: Final commit（如有 lint 修复）**

如有 lint 或测试修复：
```bash
git add -A
git commit -m "chore: fix lint warnings from session restore optimization

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

## Self-Review

### Spec coverage

| 需求 | Task |
|------|------|
| 消除 wrap 重复计算 | Task 1 |
| Markdown 延迟解析（UserBubble） | Task 2 Step 1, 3 |
| Markdown 延迟解析（AssistantBubble TextBlock） | Task 2 Step 2 |
| TUI 统一 load_context | Task 3 |
| 全量验证 | Task 4 |

### Placeholder scan

无 TBD/TODO/占位符。所有步骤包含具体代码或命令。

### Type consistency

- `build_wrap_map` 返回 `(usize, Vec<WrappedLineInfo>)`——所有调用点（`rebuild()` 和测试）均已适配
- `ContentBlockView::Text { dirty: true, rendered_prefix_len: 0 }` 与 `ensure_rendered_incremental` 的路径 3（全量重解析）匹配
- `load_context()` 和 `load_messages()` 返回类型均为 `Vec<BaseMessage>`
