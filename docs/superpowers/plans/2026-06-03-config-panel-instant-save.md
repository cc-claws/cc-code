# Config Panel Instant-Save Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Config panel changes persist immediately on toggle/blur instead of requiring Enter.

**Architecture:** Extract save logic from Enter handler into a reusable `save_config_now()` free function. Call it from toggle (Space/Left/Right on boolean/select rows), blur (Up/Down leaving text rows), and Esc (close with save). Enter becomes a no-op.

**Tech Stack:** Rust, ratatui, tui_textarea

---

### Task 1: Extract `is_text_row` helper + `save_config_now` free function

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: Add `is_text_row` function**

在 `next_editable_row` 函数之后（第 54 行后）添加：

```rust
fn is_text_row(row: usize) -> bool {
    matches!(row, ROW_THRESHOLD | ROW_PERSONA | ROW_TONE)
}

fn save_config_now(panel: &mut ConfigPanel, ctx: &mut PanelContext<'_>) {
    let Some(cfg) = ctx.services.peri_config.as_mut() else {
        return;
    };
    if panel.apply_edit(cfg, &ctx.services.lc).is_ok() {
        if let Some(ref lang) = cfg.config.language {
            let _ = ctx.services.lc.switch(lang);
        }
        let _ = App::save_config(cfg, ctx.services.config_path_override.as_deref());
    }
}
```

- [ ] **Step 2: Run `cargo check -p peri-tui` 确认编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功（新函数尚未被调用）

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/config_panel.rs
git commit -m "refactor(tui): extract is_text_row + save_config_now helpers"
```

---

### Task 2: Rewrite `handle_key` — toggle rows save immediately

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: 改造 Space 分支 — 布尔/选择字段切换后立即保存**

将 Space 分支（第 399-413 行）改为：

```rust
Input {
    key: Key::Char(' '),
    ctrl: false,
    ..
} => {
    match self.cursor {
        ROW_AUTOCOMPACT | ROW_LANGUAGE | ROW_PROACTIVENESS | ROW_DIFF | ROW_STREAMING => {
            match self.cursor {
                ROW_AUTOCOMPACT => self.cycle_autocompact(),
                ROW_LANGUAGE => self.cycle_language(false),
                ROW_PROACTIVENESS => self.cycle_proactiveness(),
                ROW_DIFF => self.cycle_diff(),
                ROW_STREAMING => self.cycle_streaming(false),
                _ => {}
            }
            save_config_now(self, ctx);
        }
        _ => self.input_char(' '),
    }
    EventResult::Consumed
}
```

- [ ] **Step 2: 改造 Left 分支 — 选择字段切换后立即保存**

将 Left 分支（第 414-430 行）改为：

```rust
Input {
    key: Key::Left,
    ctrl: false,
    ..
} => {
    match self.cursor {
        ROW_AUTOCOMPACT | ROW_LANGUAGE | ROW_PROACTIVENESS | ROW_DIFF | ROW_STREAMING => {
            match self.cursor {
                ROW_AUTOCOMPACT => self.cycle_autocompact(),
                ROW_LANGUAGE => self.cycle_language(true),
                ROW_PROACTIVENESS => self.cycle_proactiveness(),
                ROW_DIFF => self.cycle_diff(),
                ROW_STREAMING => self.cycle_streaming(true),
                _ => {}
            }
            save_config_now(self, ctx);
        }
        _ => {
            self.handle_text_key(input);
        }
    }
    EventResult::Consumed
}
```

- [ ] **Step 3: 改造 Right 分支 — 选择字段切换后立即保存**

将 Right 分支（第 431-447 行）改为：

```rust
Input {
    key: Key::Right,
    ctrl: false,
    ..
} => {
    match self.cursor {
        ROW_AUTOCOMPACT | ROW_LANGUAGE | ROW_PROACTIVENESS | ROW_DIFF | ROW_STREAMING => {
            match self.cursor {
                ROW_AUTOCOMPACT => self.cycle_autocompact(),
                ROW_LANGUAGE => self.cycle_language(false),
                ROW_PROACTIVENESS => self.cycle_proactiveness(),
                ROW_DIFF => self.cycle_diff(),
                ROW_STREAMING => self.cycle_streaming(false),
                _ => {}
            }
            save_config_now(self, ctx);
        }
        _ => {
            self.handle_text_key(input);
        }
    }
    EventResult::Consumed
}
```

- [ ] **Step 4: Run `cargo check -p peri-tui` 确认编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/app/config_panel.rs
git commit -m "feat(tui): config panel toggle fields save immediately"
```

---

### Task 3: Rewrite `handle_key` — Up/Down blur-save, Esc save-and-close, Enter no-op

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: 改造 Up 分支 — 离开文本字段时先保存**

将 Up 分支（第 354-357 行）改为：

```rust
Input { key: Key::Up, .. } => {
    if is_text_row(self.cursor) {
        save_config_now(self, ctx);
    }
    self.cursor_up();
    EventResult::Consumed
}
```

- [ ] **Step 2: 改造 Down 分支 — 离开文本字段时先保存**

将 Down 分支（第 358-361 行）改为：

```rust
Input { key: Key::Down, .. } => {
    if is_text_row(self.cursor) {
        save_config_now(self, ctx);
    }
    self.cursor_down();
    EventResult::Consumed
}
```

- [ ] **Step 3: 改造 Enter 分支 — 变为 no-op**

将整个 Enter 分支（第 362-398 行）替换为：

```rust
Input {
    key: Key::Enter, ..
} => EventResult::Consumed,
```

- [ ] **Step 4: 改造 Esc 分支 — 保存后关闭**

将 Esc 分支（第 353 行）改为：

```rust
Input { key: Key::Esc, .. } => {
    if is_text_row(self.cursor) {
        save_config_now(self, ctx);
    }
    EventResult::ClosePanel
}
```

- [ ] **Step 5: Run `cargo check -p peri-tui` 确认编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/config_panel.rs
git commit -m "feat(tui): config panel blur-save on Up/Down/Esc, Enter is no-op"
```

---

### Task 4: Mouse click blur-save

**Files:**
- Modify: `peri-tui/src/app/config_panel.rs`

- [ ] **Step 1: 在 `handle_mouse` 中，点击新行之前先保存当前文本字段**

将 `handle_mouse` 方法（第 466-494 行）中 `self.cursor = clicked;` 之前插入保存逻辑。修改后的完整方法：

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
            let clicked = (relative_y - 1) as usize;
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
    EventResult::NotConsumed
}
```

注意：`handle_mouse` 的签名中 `ctx` 参数名是 `_ctx`，保持不变。

- [ ] **Step 2: Run `cargo check -p peri-tui` 确认编译通过**

Run: `cargo check -p peri-tui 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/config_panel.rs
git commit -m "feat(tui): config panel mouse click triggers blur-save"
```

---

### Task 5: Run tests + clippy

**Files:**
- 无新改动，验证已有测试通过

- [ ] **Step 1: 运行 config_panel 单元测试**

Run: `cargo test -p peri-tui --lib config_panel_test 2>&1`
Expected: 全部 PASS（现有测试不依赖 handle_key，测试的是 `ConfigPanel` struct 方法）

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy -p peri-tui 2>&1 | tail -10`
Expected: 无新 warning

- [ ] **Step 3: Commit（如有修复）**

仅在有修复时 commit。
