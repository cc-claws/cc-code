# peri-widgets WidgetRef 迁移 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `peri-widgets` 中的 Widget 组件从 `Widget` trait（消费所有权）迁移到 `WidgetRef` trait（引用渲染），减少流式输出场景下的不必要克隆开销。

**Architecture:** 在 `peri-widgets` 和 `peri-tui` 的 `Cargo.toml` 中启用 `unstable-widget-ref` feature；为每个自定义 Widget 同时实现 `WidgetRef` 和原有 `Widget`（`Widget` 委托给 `WidgetRef`）；为 `StatefulWidget` 实现 `StatefulWidgetRef`；渲染调用处从 `render_widget()` 迁移到 `render_widget_ref()`（需 `use ratatui::widgets::FrameExt`）。

**Tech Stack:** Rust, ratatui 0.30+ `unstable-widget-ref` feature, `WidgetRef`/`StatefulWidgetRef` trait, `FrameExt` trait

**Key Design Decisions:**

1. **渐进式迁移**：不需要一次性改完所有组件。优先迁移高频渲染的热路径组件，低频组件保持原状。
2. **双向兼容**：保留原有 `Widget` impl（委托给 `WidgetRef::render_ref`），确保未迁移的调用处不会 break。
3. **`Widget` for `&T` blanket impl**：ratatui 0.30 已为 `&W where W: WidgetRef` 提供 blanket `Widget` impl，因此在 `WidgetRef` impl 中直接用 `&self` 调内部方法即可。
4. **Feature gate 统一**：`peri-widgets` 和 `peri-tui` 都需要启用 `unstable-widget-ref`，确保 trait 可见。
5. **不迁移 `ScrollableArea`/`BorderedPanel`/`ListOverlay`**：这些 widget 内部直接调用 `f.render_widget()`（Frame 级别），需要 `FrameExt` 的 `render_widget_ref()` 方法，迁移收益不高且改动分散，留作后续。

**Widget 分类与优先级：**

| 优先级 | Widget | 当前 trait | 迁移理由 |
|--------|--------|-----------|---------|
| P0 | `ToolCallWidget` | `Widget` | 流式输出中频繁重建，含 `String::clone()` |
| P0 | `SpinnerWidget` | `Widget` | 每 100ms tick 重建一次 |
| P0 | `MessageBlockWidget` | `Widget` | 代码块渲染，含 `Vec<Line>` 收集 |
| P1 | `FileTree` | `StatefulWidget` | 大仓库场景下节点多 |
| P1 | `SelectableList` | `StatefulWidget` | 通用列表组件 |
| P1 | `TabBar` | `StatefulWidget` | 低频，但结构简单，适合练手 |
| P2 | `CheckboxGroup` | `StatefulWidget` | 低频，仅在 popup 中使用 |
| P2 | `RadioGroup` | `StatefulWidget` | 低频 |
| P2 | `InputField` | `StatefulWidget` | 低频 |

**不做迁移的组件：**
- `ScrollableArea` — 内部使用 `f.render_widget()`，需要 Frame 级 API 变更
- `BorderedPanel` — 同上，且语义是「容器」而非「组件」
- `ListOverlay` — 同上
- `diff/renderer.rs` — 纯函数 `render_diff_impl()` 返回 `Vec<Line>`，不是 Widget
- `form.rs` — 纯状态管理，无 Widget impl

---

## Phase 1: 启用 Feature Flag + 基础设施

### Task 1: 在 `peri-widgets/Cargo.toml` 中启用 `unstable-widget-ref`

**Files:**
- Modify: `peri-widgets/Cargo.toml`

- [ ] **Step 1: 添加 `unstable-widget-ref` feature**

```toml
# peri-widgets/Cargo.toml
ratatui = { version = ">=0.30", features = ["unstable-rendered-line-info", "unstable-widget-ref"] }
```

- [ ] **Step 2: 验证编译通过**

```bash
cargo build -p peri-widgets
```

- [ ] **Step 3: 运行现有测试**

```bash
cargo test -p peri-widgets
```

预期：全部 PASS（仅添加 feature flag，不改代码）

---

### Task 2: 在 `peri-tui/Cargo.toml` 中启用 `unstable-widget-ref`

**Files:**
- Modify: `peri-tui/Cargo.toml`

- [ ] **Step 1: 添加 `unstable-widget-ref` feature**

```toml
# peri-tui/Cargo.toml
ratatatui = { version = ">=0.30", features = ["unstable-rendered-line-info", "unstable-widget-ref"] }
```

- [ ] **Step 2: 验证全量编译**

```bash
cargo build -p peri-tui
```

- [ ] **Step 3: Commit feature flags**

```bash
git add peri-widgets/Cargo.toml peri-tui/Cargo.toml
git commit -m "chore: enable unstable-widget-ref feature for ratatui

Enable the WidgetRef/StatefulWidgetRef traits in both peri-widgets
and peri-tui to prepare for incremental WidgetRef migration.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 2: P0 Widget 迁移（热路径组件）

### Task 3: 为 `ToolCallWidget` 实现 `WidgetRef`

**Files:**
- Modify: `peri-widgets/src/tool_call/mod.rs`
- Modify: `peri-widgets/src/tool_call/mod_test.rs`

**背景：** `ToolCallWidget<'a>` 当前实现 `Widget`（`fn render(self, ...)`），每次渲染消费所有权。流式场景下每条消息的 ToolCallWidget 独立构建，内部有 `format_args_summary` 和 `String::clone()` 调用。迁移后通过引用渲染，避免所有权转移。

- [ ] **Step 1: 添加 `WidgetRef` import**

在 `tool_call/mod.rs` 顶部 imports 中添加：

```rust
use ratatui::widgets::WidgetRef;
```

- [ ] **Step 2: 实现 `WidgetRef for ToolCallWidget`**

在现有 `impl<'a> Widget for ToolCallWidget<'a>` 之前（或之后），添加：

```rust
impl WidgetRef for ToolCallWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let indicator = display::format_indicator(self.state.status.clone(), self.state.tick);
        let arrow = if self.state.collapsed { "▸" } else { "▾" };

        let mut header_spans: Vec<Span<'_>> = vec![
            Span::styled(
                format!("{} ", indicator),
                Style::default().fg(self.state.color),
            ),
            Span::styled(format!("{} ", arrow), Style::default().fg(self.state.color)),
            Span::styled(
                self.state.tool_name.clone(),
                Style::default()
                    .fg(self.state.color)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if !self.state.args_summary.is_empty() {
            let summary = display::format_args_summary(&self.state.args_summary, 400);
            header_spans.push(Span::styled(
                format!("({})", summary),
                Style::default().fg(ratatui::style::Color::DarkGray),
            ));
        }

        let mut lines: Vec<Line<'_>> = vec![Line::from(header_spans)];

        if !self.state.collapsed && !self.state.result_lines.is_empty() {
            for line in &self.state.result_lines {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(ratatui::style::Color::DarkGray)),
                    Span::raw(line.clone()),
                ]));
            }
            if let Some(omitted) = self.state.omitted_lines {
                lines.push(Line::from(vec![Span::styled(
                    format!("  … ({} more lines)", omitted),
                    Style::default().fg(ratatui::style::Color::DarkGray),
                )]));
            }
        }

        Paragraph::new(lines).render(area, buf);
    }
}
```

- [ ] **Step 3: 修改 `Widget` impl 委托给 `WidgetRef`**

```rust
impl Widget for ToolCallWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_ref(area, buf);
    }
}
```

注意：`Widget` impl 的签名从 `impl<'a> Widget for ToolCallWidget<'a>` 改为 `impl Widget for ToolCallWidget<'_>`，因为 `render_ref` 不再需要显式生命周期参数（生命周期由 `&self` 推导）。

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- tool_call
```

预期：所有现有测试 PASS。

- [ ] **Step 5: 添加 WidgetRef 测试**

在 `mod_test.rs` 中添加：

```rust
#[test]
fn test_tool_call_widget_ref_render() {
    // Arrange
    let state = ToolCallState::new("Read".to_string(), Color::Cyan);
    let widget = ToolCallWidget::new(&state);
    let backend = TestBackend::new(40, 5);
    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));

    // Act — 通过引用渲染
    WidgetRef::render_ref(&widget, Rect::new(0, 0, 40, 3), &mut buf);

    // Assert — buffer 不为空（至少有工具名）
    let content = buffer_to_string(&buf, 40, 3);
    assert!(content.contains("Read"), "WidgetRef 渲染结果应包含工具名");
}
```

- [ ] **Step 6: Commit**

```bash
git add peri-widgets/src/tool_call/
git commit -m "refactor(widgets): implement WidgetRef for ToolCallWidget

Render by reference instead of consuming ownership. The existing
Widget impl now delegates to WidgetRef::render_ref().

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: 为 `SpinnerWidget` 实现 `WidgetRef`

**Files:**
- Modify: `peri-widgets/src/spinner/mod.rs`

**背景：** `SpinnerWidget` 每 100ms tick 重建一次，是最高频渲染的组件。迁移后避免每 tick 消费所有权。

- [ ] **Step 1: 添加 `WidgetRef` import**

```rust
use ratatui::widgets::WidgetRef;
```

- [ ] **Step 2: 实现 `WidgetRef for SpinnerWidget`**

```rust
impl WidgetRef for SpinnerWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'_>> = vec![];

        let frame = animation::tick_to_frame(self.state.tick());
        let orange = Style::default().fg(self.primary_color);
        let gray = Style::default().fg(self.secondary_color);

        spans.push(Span::styled(format!("{} ", frame), orange));
        spans.push(Span::styled(self.state.verb().to_string(), orange));

        let elapsed = self.state.elapsed_ms();
        let displayed_tokens = self.state.displayed_tokens();

        let mut suffix_parts = Vec::new();

        if self.show_elapsed {
            suffix_parts.push(animation::format_elapsed(elapsed));
        }

        if self.show_tokens && displayed_tokens > 0 {
            suffix_parts.push(format!(
                "↓ {} tokens",
                animation::format_tokens(displayed_tokens)
            ));
        }

        if !suffix_parts.is_empty() {
            spans.push(Span::styled(
                format!(" ({}", suffix_parts.join(" · ")),
                gray,
            ));
            spans.push(Span::styled(")", gray));
        }

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
```

- [ ] **Step 3: 修改 `Widget` impl 委托**

```rust
impl Widget for SpinnerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_ref(area, buf);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- spinner
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/spinner/
git commit -m "refactor(widgets): implement WidgetRef for SpinnerWidget

Spinner is rendered every 100ms tick during active agent sessions.
Rendering by reference avoids per-tick ownership transfer.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: 为 `MessageBlockWidget` 实现 `WidgetRef`

**Files:**
- Modify: `peri-widgets/src/message_block/mod.rs`

**背景：** `MessageBlockWidget` 渲染代码块和消息块，内部收集 `Vec<Line>`。在流式输出中频繁调用。

- [ ] **Step 1: 添加 `WidgetRef` import**

```rust
use ratatui::widgets::WidgetRef;
```

- [ ] **Step 2: 实现 `WidgetRef for MessageBlockWidget`**

```rust
impl WidgetRef for MessageBlockWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let theme = DarkTheme;
        let mut all_lines: Vec<Line<'_>> = Vec::new();
        for block in &self.state.blocks {
            let lines = blocks::render_block(block, self.width, &theme);
            all_lines.extend(lines);
        }
        Paragraph::new(all_lines).render(area, buf);
    }
}
```

- [ ] **Step 3: 修改 `Widget` impl 委托**

```rust
impl Widget for MessageBlockWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_ref(area, buf);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- message_block
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/message_block/
git commit -m "refactor(widgets): implement WidgetRef for MessageBlockWidget

Code blocks and message blocks are rendered frequently during
streaming. WidgetRef avoids ownership transfer on each render.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 3: P1 StatefulWidget 迁移

### Task 6: 为 `FileTree` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/file_tree/render.rs`

**背景：** `FileTree` 是 `StatefulWidget`，需要同时实现 `StatefulWidgetRef`。ratatui 0.30 已提供 `&W where W: StatefulWidget` 的 blanket `StatefulWidgetRef` impl，但显式实现可以控制渲染逻辑并确保引用语义。

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for FileTree`**

```rust
impl StatefulWidgetRef for FileTree {
    type State = FileTreeState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.height == 0 {
            return;
        }
        let visible_height = area.height;
        state
            .scroll
            .ensure_visible(state.cursor as u16, visible_height);
        let offset = state.scroll.offset() as usize;
        let cursor = state.cursor;

        let flat = state.flat();
        let lines: Vec<Line<'_>> = flat
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_height as usize)
            .map(|(i, node)| build_line(node, i == cursor, self))
            .collect();

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text);
        paragraph.render(area, buf);
    }
}
```

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl StatefulWidget for FileTree {
    type State = FileTreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- file_tree
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/file_tree/
git commit -m "refactor(widgets): implement StatefulWidgetRef for FileTree

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: 为 `SelectableList` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/list.rs`

**背景：** `SelectableList<'a, T>` 是泛型 `StatefulWidget`，需要泛型 `StatefulWidgetRef` impl。

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for SelectableList`**

```rust
impl<T> StatefulWidgetRef for SelectableList<'_, T> {
    type State = ListState<T>;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let cursor = state.cursor;
        let hovered_idx = state.hovered();

        let mut lines: Vec<Line<'_>> = Vec::with_capacity(state.items.len());
        for (i, item) in state.items.iter().enumerate() {
            let is_cursor = i == cursor;
            let is_hovered = hovered_idx == Some(i);

            let line = (self.render_item)(item, is_cursor, is_hovered);

            let style = if is_cursor {
                self.cursor_style
            } else if is_hovered {
                self.hover_style
            } else {
                self.normal_style
            };
            let marker = if is_cursor {
                Span::styled(self.cursor_marker.to_string(), style)
            } else {
                Span::styled(" ".repeat(self.cursor_marker.chars().count()), style)
            };
            let mut spans = vec![marker];
            spans.extend(line.spans.iter().cloned().map(|s| s.patch_style(style)));
            lines.push(Line::from(spans));
        }

        let text = Text::from(lines);
        let visible = area.height;
        state.scroll.ensure_visible(cursor as u16, visible);

        let paragraph = Paragraph::new(text).scroll((state.scroll.offset(), 0));
        paragraph.render(area, buf);
    }
}
```

注意：`render_item` 是 `Box<dyn Fn(&T, bool, bool) -> Line<'a>>`，通过 `&self` 可以调用（闭包不消费 self）。这是安全的，因为 `render_ref` 只借用 self。

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl<T> StatefulWidget for SelectableList<'_, T> {
    type State = ListState<T>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- list
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/list.rs peri-widgets/src/list_test.rs
git commit -m "refactor(widgets): implement StatefulWidgetRef for SelectableList

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 8: 为 `TabBar` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/tab_bar.rs`

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for TabBar`**

```rust
impl StatefulWidgetRef for TabBar {
    type State = TabState;

    fn render_ref(&self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if state.labels.is_empty() || area.width < 3 {
            return;
        }
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (i, label) in state.labels.iter().enumerate() {
            let indicator = state.indicators.get(i).copied().flatten();
            let indicator_str = indicator
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".to_string());
            let style = if i == state.active {
                self.style.active
            } else if indicator.is_some() {
                self.style.completed
            } else {
                self.style.incomplete
            };
            spans.push(Span::styled(
                format!(" {} {} ", indicator_str, label),
                style,
            ));
            if i < state.labels.len() - 1 {
                spans.push(Span::styled(
                    self.style.separator.to_string(),
                    self.style.incomplete,
                ));
            }
        }
        let line = Line::from(spans);
        let _ = buf.set_line(area.x, area.y, &line, area.width);
    }
}
```

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl StatefulWidget for TabBar {
    type State = TabState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- tab_bar
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/tab_bar.rs
git commit -m "refactor(widgets): implement StatefulWidgetRef for TabBar

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 4: P2 低频组件迁移

### Task 9: 为 `CheckboxGroup` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/checkbox_group.rs`

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for CheckboxGroup`**

```rust
impl StatefulWidgetRef for CheckboxGroup<'_> {
    type State = CheckboxState;

    fn render_ref(&self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if self.labels.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, label) in self.labels.iter().enumerate() {
            let is_cursor = i == state.cursor;
            let checked = state.is_checked(i);
            let icon = if checked {
                self.checked_char
            } else {
                self.unchecked_char
            };
            let style = if is_cursor {
                self.cursor_style
            } else {
                self.normal_style
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(label.to_string(), style),
            ]));
        }
        let text = ratatui::text::Text::from(lines);
        for (i, line) in text.lines.iter().enumerate() {
            if area.y as usize + i < buf.area.height as usize {
                let _ = buf.set_line(area.x, area.y + i as u16, line, area.width);
            }
        }
    }
}
```

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl StatefulWidget for CheckboxGroup<'_> {
    type State = CheckboxState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- checkbox_group
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/checkbox_group.rs
git commit -m "refactor(widgets): implement StatefulWidgetRef for CheckboxGroup

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 10: 为 `RadioGroup` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/radio_group.rs`

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for RadioGroup`**

```rust
impl StatefulWidgetRef for RadioGroup<'_> {
    type State = RadioState;

    fn render_ref(&self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        if self.options.is_empty() {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, opt) in self.options.iter().enumerate() {
            let is_cursor = i == state.cursor;
            let is_selected = state.selected == Some(i);
            let marker = if is_selected {
                self.marker_char.to_string()
            } else {
                "○".to_string()
            };
            let style = if is_cursor {
                self.cursor_style
            } else {
                self.normal_style
            };
            let mut spans = vec![
                Span::styled(format!("{} ", marker), style),
                Span::styled(opt.label.to_string(), style),
            ];
            if let Some(desc) = opt.description {
                spans.push(Span::styled(format!(" — {}", desc), style));
            }
            lines.push(Line::from(spans));
        }
        let text = ratatui::text::Text::from(lines);
        for (i, line) in text.lines.iter().enumerate() {
            if area.y as usize + i < buf.area.height as usize {
                let _ = buf.set_line(area.x, area.y + i as u16, line, area.width);
            }
        }
    }
}
```

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl StatefulWidget for RadioGroup<'_> {
    type State = RadioState;

    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- radio_group
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/radio_group.rs
git commit -m "refactor(widgets): implement StatefulWidgetRef for RadioGroup

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 11: 为 `InputField` 实现 `StatefulWidgetRef`

**Files:**
- Modify: `peri-widgets/src/input_field.rs`

- [ ] **Step 1: 添加 `StatefulWidgetRef` import**

```rust
use ratatui::widgets::StatefulWidgetRef;
```

- [ ] **Step 2: 实现 `StatefulWidgetRef for InputField`**

```rust
impl StatefulWidgetRef for InputField<'_> {
    type State = InputState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let line = self.to_line(state);
        line.render(area, buf);
    }
}
```

- [ ] **Step 3: 修改 `StatefulWidget` impl 委托**

```rust
impl StatefulWidget for InputField<'_> {
    type State = InputState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test -p peri-widgets -- input_field
```

- [ ] **Step 5: Commit**

```bash
git add peri-widgets/src/input_field.rs
git commit -m "refactor(widgets): implement StatefulWidgetRef for InputField

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 5: TUI 渲染调用处迁移（可选，按需）

### Task 12: TUI 侧 `render_widget` → `render_widget_ref` 迁移示例

**Files:**
- Modify: `peri-tui/src/ui/main_ui/message_area.rs`（示例迁移点）

**注意：** 这一阶段是可选的。由于 ratatui 0.30 提供了 `&W where W: WidgetRef` 的 blanket `Widget` impl，现有 `f.render_widget(widget, area)` 调用在很多场景下已经可以隐式享受 `WidgetRef` 的好处（编译器会自动选择正确的 impl）。只有在以下场景需要显式迁移：

1. 需要 `Box<dyn WidgetRef>` 动态分发的场景
2. 同一 widget 实例需要多次渲染的场景
3. 想要显式标注引用语义的场景

对于 `peri-tui` 中直接构造 `Paragraph`/`Text` 等 ratatui 内置 widget 的场景（如 `message_area.rs`、`status_bar.rs`），这些 widget 已由 ratatui 自身实现了 `WidgetRef`，无需手动实现。只需在调用处使用 `render_widget_ref()` 即可。

- [ ] **Step 1: 在需要迁移的文件中添加 import**

```rust
use ratatui::widgets::FrameExt;
```

- [ ] **Step 2: 逐个替换热路径调用**

以 `message_area.rs` 为例：

```rust
// 之前
f.render_widget(paragraph, text_area);

// 之后（需要 use ratatui::widgets::FrameExt;）
f.render_widget_ref(&paragraph, text_area);
```

注意：`render_widget_ref` 接受 `&W`（引用），而 `render_widget` 接受 `W`（所有权）。迁移时需要调整参数为引用。

- [ ] **Step 3: 验证全量编译 + 测试**

```bash
cargo build -p peri-tui
cargo test -p peri-tui
```

- [ ] **Step 4: Commit（仅在实际迁移了调用处时）**

```bash
git commit -m "refactor(tui): migrate hot-path render calls to render_widget_ref

Use FrameExt::render_widget_ref() for message area rendering to
avoid unnecessary ownership transfer on each frame.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 6: 验证与收尾

### Task 13: 全量编译 + 测试验证

- [ ] **Step 1: 全量编译**

```bash
cargo build
```

- [ ] **Step 2: 全量测试**

```bash
cargo test
```

- [ ] **Step 3: Clippy 检查**

```bash
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 4: 格式检查**

```bash
cargo fmt --check
```

### Task 14: 更新 CLAUDE.md（可选）

如果 `WidgetRef` 模式成为项目的标准做法，可在 CLAUDE.md 的编码规范部分添加一条规则：

```
- **Widget 实现规范**：新增 Widget 组件时，必须同时实现 `WidgetRef`（或 `StatefulWidgetRef`），`Widget` impl 委托给 `WidgetRef::render_ref()`。这确保组件可以通过引用渲染，避免所有权转移。仅当组件内部需要消费所有权（如拆解 builder 字段）时才允许直接实现 `Widget`。
```

---

## 风险评估

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| `unstable-widget-ref` API 在未来 ratatui 版本中变更 | 低 | Widget impl 委托给 WidgetRef，API 变更时只需修改委托层 |
| 迁移引入渲染 bug（像素级差异） | 低 | 每个组件迁移后运行现有测试；视觉验证通过 `cargo run -p peri-tui` |
| `Box<dyn Fn>` 闭包在 `&self` 上下文中可用性 | 无 | 已验证：`Fn` trait 不消费 self，`&self` 可正常调用 |
| 泛型组件 `SelectableList<T>` 的 WidgetRef impl 复杂性 | 低 | `StatefulWidgetRef` 是泛型 impl，与 `StatefulWidget` 对称 |

## 预期收益

- **减少流式场景 clone 开销**：`ToolCallWidget`/`SpinnerWidget` 不再每帧消费所有权
- **为动态 widget 集合铺路**：`Box<dyn WidgetRef>` 可用于消息列表等动态渲染场景
- **对齐 ratatui 最佳实践**：WidgetRef 是 ratatui 社区推荐的新模式，预计将在未来版本稳定
