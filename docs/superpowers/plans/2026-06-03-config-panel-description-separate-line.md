# Config Panel Description Separate Line Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move config panel field descriptions from inline (same line as value) to a separate line below the field label.

**Architecture:** Each editable field currently renders as 1 line (label + value + description). Change to 2 lines: first line has label + value, second line has description in MUTED style. Add a `screen_to_logical_row()` mapping function so mouse clicks still map to the correct logical row index. Increase `desired_height` to fit the extra lines.

**Tech Stack:** Rust, ratatui

---

### Task 1: Update render — description on separate line

**Files:**
- Modify: `peri-tui/src/ui/main_ui/panels/config.rs`

- [ ] **Step 1: 修改所有字段渲染，description 从同行移到独立行**

每个字段类型的改动模式相同：把 `desc_style` 的 Span 从值行末尾移除，改为 `lines.push` 一个独立的新行。

以 `ROW_AUTOCOMPACT`（第 87-118 行）为例，当前代码：

```rust
lines.push(Line::from(vec![
    Span::styled("  ", Style::default()),
    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
    on_span,
    Span::styled("  ", Style::default()),
    off_span,
    Span::styled(
        format!("  {}", lc.tr("config-desc-autocompact")),
        desc_style,
    ),
]));
```

改为：

```rust
lines.push(Line::from(vec![
    Span::styled("  ", Style::default()),
    Span::styled(format!("{:<14}", lc.tr(field_label_key(row))), label_style),
    on_span,
    Span::styled("  ", Style::default()),
    off_span,
]));
lines.push(Line::from(Span::styled(
    format!("  {}", lc.tr("config-desc-autocompact")),
    desc_style,
)));
```

对以下所有字段重复同样模式（移除行内 description Span，追加独立 Line）：

| 字段 | 行号范围 | description i18n key |
|------|---------|---------------------|
| `ROW_AUTOCOMPACT` | 87-118 | `config-desc-autocompact` |
| `ROW_LANGUAGE` | 119-154 | `config-desc-language` |
| `ROW_DIFF` | 155-183 | `config-desc-diff` |
| `ROW_STREAMING` | 184-216 | `config-desc-streaming` |
| `ROW_PROACTIVENESS` | 217-249 | `config-desc-proactiveness` |
| `ROW_THRESHOLD` | 250-289 | `config-desc-threshold` |
| `ROW_PERSONA` | 250-289 | `config-desc-persona` |
| `ROW_TONE` | 250-289 | `config-desc-tone` |

对于 `ROW_LANGUAGE`、`ROW_STREAMING`、`ROW_PROACTIVENESS`（使用 `value_spans` vector 的字段），需要从 `value_spans` 中移除最后的 description span，改为独立行 push。

对于 `ROW_THRESHOLD | ROW_PERSONA | ROW_TONE`，移除 `Span::styled(format!("  {}", lc.tr(desc_key)), desc_style)` 追加独立行。

- [ ] **Step 2: Run `cargo check -p peri-tui`**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/main_ui/panels/config.rs
git commit -m "refactor(tui): config panel description on separate line"
```

---

### Task 2: Update mouse click mapping + desired_height

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

改动后屏幕布局不再是 1:1 的 row index → screen line，需要映射。

- [ ] **Step 1: 添加 `screen_to_logical_row()` 函数**

在 `config_panel.rs` 中 `is_text_row` 之后添加：

```rust
/// 屏幕行号 → 逻辑行号。
/// 渲染时每个可编辑字段占 2 行（值行 + 描述行），非编辑行占 1 行。
const SCREEN_LAYOUT: &[usize] = &[
    ROW_GENERAL_HEADER,   // screen 0: General 标题
    ROW_AUTOCOMPACT,      // screen 1: 值
    ROW_AUTOCOMPACT,      // screen 2: 描述
    ROW_THRESHOLD,        // screen 3: 值
    ROW_THRESHOLD,        // screen 4: 描述
    ROW_LANGUAGE,         // screen 5: 值
    ROW_LANGUAGE,         // screen 6: 描述
    ROW_DIFF,             // screen 7: 值
    ROW_DIFF,             // screen 8: 描述
    ROW_STREAMING,        // screen 9: 值
    ROW_STREAMING,        // screen 10: 描述
    ROW_PROACTIVENESS,    // screen 11: 值
    ROW_PROACTIVENESS,    // screen 12: 描述
    ROW_SEPARATOR,        // screen 13: 分隔线
    ROW_OVERRIDES_HEADER, // screen 14: Overrides 标题
    ROW_PERSONA,          // screen 15: 值
    ROW_PERSONA,          // screen 16: 描述
    ROW_TONE,             // screen 17: 值
    ROW_TONE,             // screen 18: 描述
];

fn screen_to_logical_row(screen_line: usize) -> Option<usize> {
    SCREEN_LAYOUT.get(screen_line).copied()
}
```

- [ ] **Step 2: 更新 `handle_mouse` 使用映射**

将 `handle_mouse` 方法（第 480-511 行）中的直接 `clicked = (relative_y - 1) as usize` 逻辑改为使用映射：

```rust
fn handle_mouse(
    &mut self,
    mouse: ratatui::crossterm::event::MouseEvent,
    area: Rect,
    _ctx: &mut PanelContext<'_>,
) -> EventResult {
    use ratatui::crossterm::event::{MouseButton, MouseEventKind};
    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
        let relative_y = mouse.row.saturating_sub(area.y);
        if relative_y >= 1 {
            let screen_line = (relative_y - 1) as usize;
            if let Some(clicked) = screen_to_logical_row(screen_line) {
                if matches!(
                    clicked,
                    ROW_AUTOCOMPACT
                        | ROW_THRESHOLD
                        | ROW_LANGUAGE
                        | ROW_DIFF
                        | ROW_STREAMING
                        | ROW_PROACTIVENESS
                        | ROW_PERSONA
                        | ROW_TONE
                ) {
                    if is_text_row(self.cursor) && self.cursor != clicked {
                        save_config_now(self, _ctx);
                    }
                    self.cursor = clicked;
                    return EventResult::Consumed;
                }
            }
        }
    }
    EventResult::NotConsumed
}
```

- [ ] **Step 3: 更新 `desired_height`**

当前值 `18`。新布局有 19 行（SCREEN_LAYOUT 长度）+ 2（上下边框）= 21。改为：

```rust
fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
    (SCREEN_LAYOUT.len() + 2) as u16
}
```

- [ ] **Step 4: Run `cargo check -p peri-tui`**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 5: Run tests**

Run: `cargo test -p peri-tui --lib config_panel_test 2>&1 | tail -5`
Expected: 全部 PASS

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/config_panel.rs
git commit -m "fix(tui): config panel mouse mapping + height for description lines"
```
