# Popup Input Unified Component Implementation Plan
# Popup Input Unified Component Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 统一所有弹窗/面板输入框的编辑能力，添加跳词（Ctrl+Left/Right）和删词（Ctrl+W、Alt+Backspace）到 `handle_edit_key()` 和 `InputState`。

**Architecture:** 两个独立路径，互不交叉：(1) `handle_edit_key()` 增强——所有使用 `String` + `usize` 字符索引光标的弹窗（AskUser、Config、Login、Setup）受益；(2) `InputState` 增强——PluginPanel 的 discover_search 和 add_marketplace_input 受益。两条路径各自维护与光标模型一致的实现。

**Tech Stack:** Rust, tui_textarea::Input/Key, unicode-segmentation（已有依赖）

**Issue:** `spec/issues/2026-05-24-popup-input-unified-component.md`

**现状**:

| 输入框 | 光标模型 | 编辑函数 | 缺失能力 |
|--------|----------|----------|----------|
| AskUser/Config/Login/Setup | `String` + `usize`（字符索引） | `handle_edit_key()` | Ctrl+Left/Right 跳词、Ctrl+W/Alt+Backspace 删词 |
| PluginPanel discover_search | `InputState`（字节偏移） | 裸 `insert(c)` / `backspace()` | 所有光标操作 |
| PluginPanel add_marketplace | `InputState`（字节偏移） | 裸 `insert(c)` / `backspace()` | 所有光标操作 |

---

## File Structure

| 文件 | 改动 |
|------|------|
| `peri-tui/src/app/edit_utils.rs` | **Modify**: `handle_edit_key()` 添加 4 个新分支；添加 `find_word_start()` / `find_word_end()` 辅助函数 |
| `peri-widgets/src/input_field.rs` | **Modify**: `InputState` 添加 `cursor_word_left()` / `cursor_word_right()` / `delete_word_backward()` |
| `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/discover_search.rs` | **Modify**: `handle_discover_searching()` 使用 `InputState` 新方法替代裸 insert/backspace |
| `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/marketplace.rs` | **Modify**: `handle_marketplace_add()` 使用 `InputState` 新方法替代裸 insert/backspace |
| `peri-tui/src/app/plugin_panel/mod.rs` | **Modify**: 调用 `InputState` 新方法的辅助函数 |
| `peri-tui/src/app/edit_utils_test.rs` | **Create**: `handle_edit_key` 及辅助函数的测试 |

---

### Task 1: Add word boundary helpers to `edit_utils.rs`

**Files:**
- Modify: `peri-tui/src/app/edit_utils.rs`（在 `handle_edit_key` 函数之前插入）

- [ ] **Step 1: Add `find_word_start` and `find_word_end` helper functions**

```rust
/// 找到 word 左边界的字符索引。cjk 字符视为独立 word。
/// cursor: 当前字符索引（不含），从 cursor-1 向前扫描。
/// 规则：
///   - 跳过 whitespace（除非紧跟的是非空白字符，则停在空白后）
///   - 同类字符连续作为同一个 word（alphanumeric 一类，其他符号各自独立但同类合并）
fn find_word_start(chars: &[char], cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let len = chars.len();
    if cursor > len {
        return len;
    }
    // 从 cursor-1 向前扫描
    let mut pos = cursor;
    // 跳过空白
    while pos > 0 && chars[pos - 1].is_whitespace() {
        pos -= 1;
    }
    if pos == 0 {
        return 0;
    }
    // 确定当前字符类别
    let cat = char_category(chars[pos - 1]);
    // 向前扫描同类字符
    while pos > 0 {
        let prev = chars[pos - 1];
        if prev.is_whitespace() {
            // 空白前面的字符是不同类别，停在空白后
            return pos;
        }
        if char_category(prev) != cat {
            return pos;
        }
        pos -= 1;
    }
    pos
}

/// 找到 word 右边界的字符索引。
/// cursor: 当前字符索引（含），从 cursor 向后扫描。
fn find_word_end(chars: &[char], cursor: usize) -> usize {
    let len = chars.len();
    if cursor >= len {
        return len;
    }
    // 跳过空白
    let mut pos = cursor;
    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }
    if pos >= len {
        return len;
    }
    // 确定当前字符类别
    let cat = char_category(chars[pos]);
    // 向后扫描同类字符
    while pos < len {
        if chars[pos].is_whitespace() {
            return pos;
        }
        if char_category(chars[pos]) != cat {
            return pos;
        }
        pos += 1;
    }
    pos
}

/// 字符类别：alphanumeric / other。用于 word 边界判断。
fn char_category(c: char) -> u8 {
    if c.is_alphanumeric() || c == '_' {
        0
    } else {
        1
    }
}
```

- [ ] **Step 2: Run `cargo build -p peri-tui` to verify no syntax errors**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/edit_utils.rs
git commit -m "feat(edit_utils): add word boundary helpers for word-jump support

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: Add word jump/delete to `handle_edit_key()`

**Files:**
- Modify: `peri-tui/src/app/edit_utils.rs`（`handle_edit_key` 函数内）

- [ ] **Step 1: Add Ctrl+Left, Ctrl+Right, Ctrl+W, Alt+Backspace arms**

在 `handle_edit_key` 的 match 中，在 `Ctrl+U` arm 之后、`_ => false` 之前插入：

```rust
        // ── Ctrl+Left：跳词到左边界 ──────────────────────────────────────
        tui_textarea::Input {
            key: Key::Left,
            ctrl: true,
            ..
        } => {
            let chars: Vec<char> = buf.chars().collect();
            *cursor = find_word_start(&chars, *cursor);
            true
        }
        // ── Ctrl+Right：跳词到右边界 ─────────────────────────────────────
        tui_textarea::Input {
            key: Key::Right,
            ctrl: true,
            ..
        } => {
            let chars: Vec<char> = buf.chars().collect();
            *cursor = find_word_end(&chars, *cursor);
            true
        }
        // ── Ctrl+W：删除光标前一个 word ──────────────────────────────────
        tui_textarea::Input {
            key: Key::Char('w'),
            ctrl: true,
            ..
        } => {
            let char_count = buf.chars().count();
            if *cursor > char_count {
                *cursor = char_count;
            }
            if *cursor > 0 {
                let chars: Vec<char> = buf.chars().collect();
                let start = find_word_start(&chars, *cursor);
                let byte_start = buf
                    .char_indices()
                    .nth(start)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let byte_end = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.drain(byte_start..byte_end);
                *cursor = start;
            }
            true
        }
        // ── Alt+Backspace：删除光标前一个 word ──────────────────────────
        tui_textarea::Input {
            key: Key::Backspace,
            alt: true,
            ..
        } => {
            let char_count = buf.chars().count();
            if *cursor > char_count {
                *cursor = char_count;
            }
            if *cursor > 0 {
                let chars: Vec<char> = buf.chars().collect();
                let start = find_word_start(&chars, *cursor);
                let byte_start = buf
                    .char_indices()
                    .nth(start)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let byte_end = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.drain(byte_start..byte_end);
                *cursor = start;
            }
            true
        }
```

- [ ] **Step 2: Update doc comment for `handle_edit_key`**

```rust
/// 支持的按键：Char、Backspace、Delete、Left、Right、Home、End、
/// Ctrl+A(Home)、Ctrl+E(End)、Ctrl+K(kill to end)、Ctrl+U(kill to start)、
/// Ctrl+Left(word left)、Ctrl+Right(word right)、Ctrl+W(delete word backward)、
/// Alt+Backspace(delete word backward)
```

- [ ] **Step 3: Run `cargo build -p peri-tui` to verify compilation**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/edit_utils.rs
git commit -m "feat(edit_utils): add word-jump and word-delete to handle_edit_key

Add Ctrl+Left/Right for word navigation and Ctrl+W/Alt+Backspace for word
deletion. All String+cursor popup inputs (AskUser, Config, Login, Setup)
now get consistent word-level editing.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: Write tests for `handle_edit_key` helpers

**Files:**
- Create: `peri-tui/src/app/edit_utils_test.rs`

- [ ] **Step 1: Write tests for `find_word_start` and `find_word_end`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ─── find_word_start ──────────────────────────────────────────────────

    #[test]
    fn test_find_word_start_at_start() {
        let chars: Vec<char> = "hello world".chars().collect();
        // cursor=0，已经在开头
        assert_eq!(find_word_start(&chars, 0), 0);
    }

    #[test]
    fn test_find_word_start_inside_word() {
        let chars: Vec<char> = "hello world".chars().collect();
        // cursor=4（'o'），word 起始位置是 0
        assert_eq!(find_word_start(&chars, 4), 0);
    }

    #[test]
    fn test_find_word_start_after_space() {
        let chars: Vec<char> = "hello world".chars().collect();
        // cursor=6（'w'），向前找到空格，跳过空格到达 word 起始 6
        assert_eq!(find_word_start(&chars, 6), 6);
    }

    #[test]
    fn test_find_word_start_trailing_spaces() {
        let chars: Vec<char> = "foo   ".chars().collect();
        // cursor=6（末尾空格后），向前跳过空格→ foo 末尾=3，word 起始=0
        assert_eq!(find_word_start(&chars, 6), 0);
    }

    #[test]
    fn test_find_word_start_single_char_word() {
        let chars: Vec<char> = "a b c".chars().collect();
        // cursor=3（'b'），向前：blank→跳过，'a'→不同类别→停在 2
        assert_eq!(find_word_start(&chars, 3), 2);
    }

    #[test]
    fn test_find_word_start_cjk() {
        let chars: Vec<char> = "你好 世界".chars().collect();
        // cursor=3（'世'，空白之后），向前跳过空格→回到 2→word 起始=2
        assert_eq!(find_word_start(&chars, 3), 2);
    }

    // ─── find_word_end ────────────────────────────────────────────────────

    #[test]
    fn test_find_word_end_at_end() {
        let chars: Vec<char> = "hello".chars().collect();
        assert_eq!(find_word_end(&chars, 5), 5);
    }

    #[test]
    fn test_find_word_end_inside_word() {
        let chars: Vec<char> = "hello world".chars().collect();
        // cursor=0，向后：h/e/l/l/o 同类→空格→停在 5
        assert_eq!(find_word_end(&chars, 0), 5);
    }

    #[test]
    fn test_find_word_end_skipping_spaces() {
        let chars: Vec<char> = "hello   world".chars().collect();
        // cursor=7（在空格区域），向后跳过空格→w=10→word end=15
        assert_eq!(find_word_end(&chars, 7), 15);
    }

    // ─── handle_edit_key word jumps ───────────────────────────────────────

    use tui_textarea::{Input, Key};

    fn make_input(key: Key, ctrl: bool, alt: bool) -> Input {
        Input {
            key,
            ctrl,
            alt,
            shift: false,
        }
    }

    #[test]
    fn test_handle_edit_key_ctrl_left_word_jump() {
        let mut buf = "hello world foo".to_string();
        let mut cursor = 13; // 末尾
        // Ctrl+Left 一次：跳到 'f'（第三个 word 起始=12）
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 12);
        // Ctrl+Left 一次：跳到 'w'（第二个 word 起始=6）
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 6);
        // Ctrl+Left 一次：跳到 'h'（第一个 word 起始=0）
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 0);
        // 再按不动
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_handle_edit_key_ctrl_right_word_jump() {
        let mut buf = "hello world foo".to_string();
        let mut cursor = 0;
        // Ctrl+Right 一次：跳到 'h' 所在 word 结束 = 5
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Right, true, false)));
        assert_eq!(cursor, 5);
        // Ctrl+Right 一次：跳过空格→ 'w' word 结束 = 11
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Right, true, false)));
        assert_eq!(cursor, 11);
        // Ctrl+Right 一次：跳过空格→ 'f' word 结束 = 15
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Right, true, false)));
        assert_eq!(cursor, 15);
    }

    #[test]
    fn test_handle_edit_key_ctrl_w_delete_word() {
        let mut buf = "hello world".to_string();
        let mut cursor = 11; // 末尾
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Char('w'), true, false)));
        // 删除 " world"，剩下 "hello"
        assert_eq!(buf, "hello");
        assert_eq!(cursor, 5);

        // 再删一次
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Char('w'), true, false)));
        assert_eq!(buf, "");
        assert_eq!(cursor, 0);

        // 空字符串：不 panic
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Char('w'), true, false)));
        assert_eq!(buf, "");
    }

    #[test]
    fn test_handle_edit_key_ctrl_w_middle_of_word() {
        let mut buf = "hello world".to_string();
        let mut cursor = 8; // 'r'（"world" 中第三个字符）
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Char('w'), true, false)));
        // 删除 "wo"，剩下 "hello rld"
        assert_eq!(buf, "hello rld");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn test_handle_edit_key_alt_backspace() {
        let mut buf = "hello world".to_string();
        let mut cursor = 11; // 末尾
        assert!(handle_edit_key(
            &mut buf,
            &mut cursor,
            make_input(Key::Backspace, false, true)
        ));
        // 删除 " world"
        assert_eq!(buf, "hello");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn test_handle_edit_key_ctrl_left_cjk() {
        let mut buf = "你好 世界 foo".to_string();
        let mut cursor = buf.chars().count(); // 末尾
        // Ctrl+Left 一次：跳到 'f'
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 6); // "你好 世界 " 之后
        // Ctrl+Left 一次：跳到 '世'
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 3); // "你好 " 之后
        // Ctrl+Left 一次：跳到 '你'
        assert!(handle_edit_key(&mut buf, &mut cursor, make_input(Key::Left, true, false)));
        assert_eq!(cursor, 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p peri-tui -- edit_utils_test --nocapture 2>&1 | tail -30`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/edit_utils_test.rs
git commit -m "test(edit_utils): add tests for word boundary helpers and word jump/delete

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: Add word operations to `InputState`

**Files:**
- Modify: `peri-widgets/src/input_field.rs`（在 `cursor_end()` 之后，`paste()` 之前插入）

- [ ] **Step 1: Add `cursor_word_left`, `cursor_word_right`, `delete_word_backward` to `InputState`**

```rust
    /// cursor 跳到前一个 word 的开头
    pub fn cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.buffer.chars().collect();
        // 将 byte cursor 转换为 char index
        let char_idx = self.buffer[..self.cursor].chars().count();
        if char_idx == 0 {
            return;
        }
        // 向前跳过空白
        let mut pos = char_idx;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        if pos == 0 {
            self.cursor = 0;
            return;
        }
        // 确定当前字符类别
        let cat = if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' {
            0u8
        } else {
            1u8
        };
        // 向前扫描同类字符
        while pos > 0 {
            let prev = chars[pos - 1];
            if prev.is_whitespace() {
                break;
            }
            let prev_cat = if prev.is_alphanumeric() || prev == '_' {
                0u8
            } else {
                1u8
            };
            if prev_cat != cat {
                break;
            }
            pos -= 1;
        }
        // 将 char index 转回 byte offset
        self.cursor = self.byte_offset_at(&chars, pos);
    }

    /// cursor 跳到下一个 word 的开头（跳过当前 word）
    pub fn cursor_word_right(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let char_idx = self.buffer[..self.cursor].chars().count();
        let len = chars.len();
        if char_idx >= len {
            return;
        }
        // 跳过空白
        let mut pos = char_idx;
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= len {
            self.cursor = self.buffer.len();
            return;
        }
        // 确定当前字符类别
        let cat = if chars[pos].is_alphanumeric() || chars[pos] == '_' {
            0u8
        } else {
            1u8
        };
        // 向后扫描同类字符
        while pos < len {
            if chars[pos].is_whitespace() {
                break;
            }
            let cur_cat = if chars[pos].is_alphanumeric() || chars[pos] == '_' {
                0u8
            } else {
                1u8
            };
            if cur_cat != cat {
                break;
            }
            pos += 1;
        }
        // 将 char index 转回 byte offset
        self.cursor = self.byte_offset_at(&chars, pos);
    }

    /// 删除光标前一个 word
    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.buffer.chars().collect();
        let char_idx = self.buffer[..self.cursor].chars().count();
        if char_idx == 0 {
            return;
        }
        // 向前跳过空白
        let mut pos = char_idx;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        if pos == 0 {
            // 只有空白，全删
            let byte_end = self.cursor;
            self.buffer.drain(..byte_end);
            self.cursor = 0;
            return;
        }
        // 确定当前字符类别
        let cat = if chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_' {
            0u8
        } else {
            1u8
        };
        // 向前扫描同类字符
        while pos > 0 {
            let prev = chars[pos - 1];
            if prev.is_whitespace() {
                break;
            }
            let prev_cat = if prev.is_alphanumeric() || prev == '_' {
                0u8
            } else {
                1u8
            };
            if prev_cat != cat {
                break;
            }
            pos -= 1;
        }
        let byte_start = self.byte_offset_at(&chars, pos);
        let byte_end = self.cursor;
        self.buffer.drain(byte_start..byte_end);
        self.cursor = byte_start;
    }

    /// 辅助：char index → byte offset
    fn byte_offset_at(&self, chars: &[char], idx: usize) -> usize {
        if idx >= chars.len() {
            return self.buffer.len();
        }
        chars.iter().take(idx).map(|c| c.len_utf8()).sum()
    }
```

- [ ] **Step 2: Run `cargo build -p peri-widgets` to verify compilation**

Run: `cargo build -p peri-widgets 2>&1 | head -20`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/input_field.rs
git commit -m "feat(input_field): add word navigation and deletion to InputState

Add cursor_word_left, cursor_word_right, and delete_word_backward methods
to InputState, enabling word-level editing in plugin search/input fields.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 5: Write tests for `InputState` word operations

**Files:**
- Modify: `peri-widgets/src/input_field_test.rs`

- [ ] **Step 1: Read existing test file to understand test conventions**

```bash
Read peri-widgets/src/input_field_test.rs first
```

- [ ] **Step 2: Add tests for word operations at end of test file**

```rust
    #[test]
    fn test_cursor_word_left_basic() {
        let mut s = InputState::with_value("hello world".to_string());
        s.cursor_end(); // cursor at 11 (byte)
        s.cursor_word_left();
        assert_eq!(s.value(), "hello world");
        assert_eq!(s.cursor(), 6); // start of "world"
        s.cursor_word_left();
        assert_eq!(s.cursor(), 0); // start of "hello"
        s.cursor_word_left(); // at 0, no-op
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn test_cursor_word_right_basic() {
        let mut s = InputState::with_value("hello world foo".to_string());
        s.cursor_home(); // cursor at 0
        s.cursor_word_right();
        assert_eq!(s.cursor(), 5); // end of "hello"
        s.cursor_word_right();
        assert_eq!(s.cursor(), 11); // end of "world"
        s.cursor_word_right();
        assert_eq!(s.cursor(), 15); // end of "foo"
        s.cursor_word_right(); // at end, no-op
        assert_eq!(s.cursor(), 15);
    }

    #[test]
    fn test_delete_word_backward_basic() {
        let mut s = InputState::with_value("hello world".to_string());
        s.cursor_end();
        s.delete_word_backward();
        assert_eq!(s.value(), "hello");
        assert_eq!(s.cursor(), 5);
        s.delete_word_backward();
        assert_eq!(s.value(), "");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn test_delete_word_backward_mid_word() {
        let mut s = InputState::with_value("hello world".to_string());
        // cursor after "hel" = byte 3
        s.cursor = 3;
        s.delete_word_backward();
        assert_eq!(s.value(), "lo world");
        assert_eq!(s.cursor(), 0);
    }

    #[test]
    fn test_word_ops_cjk() {
        let mut s = InputState::with_value("你好 世界".to_string());
        s.cursor_end(); // at byte end (3 + 1 + 3 = 7 in UTF-8? Actually 你=3, 好=3, 空格=1, 世=3, 界=3 = 13)
        s.cursor_word_left();
        assert_eq!(s.value(), "你好 世界");
        // cursor should be at "世" = byte 3+3+1=7
        assert_eq!(s.cursor(), 7);
        s.cursor_word_left();
        assert_eq!(s.cursor(), 0);
    }
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p peri-widgets -- input_field_test --nocapture 2>&1 | tail -30`
Expected: All tests PASS (including new ones)

- [ ] **Step 4: Commit**

```bash
git add peri-widgets/src/input_field_test.rs
git commit -m "test(input_field): add tests for word navigation and deletion

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: Update PluginPanel discover_search to use InputState methods

**Files:**
- Modify: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/discover_search.rs`

- [ ] **Step 1: Rewrite `handle_discover_searching` to use InputState methods for all editing keys**

Replace the `handle_discover_searching` function with:

```rust
use tui_textarea::{Input, Key};

use crate::app::panel_manager::{EventResult, PanelContext};
use crate::app::plugin_panel::PluginPanel;

impl PluginPanel {
    pub(crate) fn handle_discover_searching(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            // ── 字符输入 ────────────────────────────────────────────────
            Input {
                key: Key::Char(c),
                ctrl: false,
                alt: false,
                ..
            } => {
                self.discover_search.insert(c);
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            // ── 光标移动 ────────────────────────────────────────────────
            Input {
                key: Key::Left,
                ctrl: false,
                ..
            } => {
                self.discover_search.cursor_left();
                EventResult::Consumed
            }
            Input {
                key: Key::Right,
                ctrl: false,
                shift: false,
                ..
            } => {
                self.discover_search.cursor_right();
                EventResult::Consumed
            }
            Input {
                key: Key::Home, ..
            } => {
                self.discover_search.cursor_home();
                EventResult::Consumed
            }
            Input { key: Key::End, .. } => {
                self.discover_search.cursor_end();
                EventResult::Consumed
            }
            // ── 跳词 ────────────────────────────────────────────────────
            Input {
                key: Key::Left,
                ctrl: true,
                ..
            } => {
                self.discover_search.cursor_word_left();
                EventResult::Consumed
            }
            Input {
                key: Key::Right,
                ctrl: true,
                ..
            } => {
                self.discover_search.cursor_word_right();
                EventResult::Consumed
            }
            // ── 删除 ────────────────────────────────────────────────────
            Input {
                key: Key::Backspace,
                alt: false,
                ..
            } => {
                self.discover_search.backspace();
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                alt: true,
                ..
            }
            | Input {
                key: Key::Char('w'),
                ctrl: true,
                ..
            } => {
                self.discover_search.delete_word_backward();
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            Input {
                key: Key::Delete, ..
            } => {
                self.discover_search.delete();
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                EventResult::Consumed
            }
            // ── 退出搜索 ────────────────────────────────────────────────
            Input { key: Key::Up, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(-1);
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.discover_searching = false;
                self.discover_list.move_cursor(1);
                EventResult::Consumed
            }
            Input { key: Key::Esc, .. } => {
                self.discover_searching = false;
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                self.discover_searching = false;
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
                self.spawn_install_current(ctx);
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
}
```

- [ ] **Step 2: Also update `discover_search_input` and `discover_search_backspace` helpers in `plugin_panel/mod.rs`**

在 `plugin_panel/mod.rs` 中定位到第 239-241 行（paste 事件中对 `discover_search.insert` 的裸调用）和第 572-588 行（`discover_search_input` / `discover_search_backspace` 辅助函数），这些函数如果只是转发到 `discover_search.insert/backspace`，不需要改动（它们只是粘贴和过滤场景的便捷方法）。

- [ ] **Step 3: Run `cargo build -p peri-tui` to verify compilation**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/app/plugin_panel/handlers/plugin_handlers/discover_search.rs
git commit -m "feat(plugin_panel): use InputState editing methods in discover search

Replace raw insert/backspace with full cursor operations (Left/Right/Home/End/
word-jump/word-delete) from InputState, giving plugin search the same editing
capabilities as other input fields.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6.5: Update marketplace_add handler to use InputState methods

**Files:**
- Modify: `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/marketplace.rs`

- [ ] **Step 1: Rewrite `handle_marketplace_add` to use InputState cursor operations**

Replace `handle_marketplace_add` function with:

```rust
    pub(super) fn handle_marketplace_add(
        &mut self,
        input: Input,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        match input {
            Input { key: Key::Esc, .. } => {
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let input_str = self.add_marketplace_input.value().trim().to_string();
                self.add_marketplace_active = false;
                self.add_marketplace_input = InputState::new();
                if !input_str.is_empty() {
                    if let Err(e) = self.persist_marketplace_add(&input_str, ctx) {
                        ctx.session_mgr.sessions[ctx.session_mgr.active]
                            .messages
                            .push_system_note(ctx.services.lc.tr_args(
                                "app-plugin-add-failed",
                                &[("error".into(), e.to_string().into())],
                            ));
                    }
                }
                EventResult::Consumed
            }
            // ── 字符输入 ────────────────────────────────────────────────
            Input {
                key: Key::Char(ch),
                ctrl: false,
                alt: false,
                ..
            } => {
                self.add_marketplace_input.insert(ch);
                EventResult::Consumed
            }
            // ── 光标移动 ────────────────────────────────────────────────
            Input {
                key: Key::Left,
                ctrl: false,
                ..
            } => {
                self.add_marketplace_input.cursor_left();
                EventResult::Consumed
            }
            Input {
                key: Key::Right,
                ctrl: false,
                shift: false,
                ..
            } => {
                self.add_marketplace_input.cursor_right();
                EventResult::Consumed
            }
            Input {
                key: Key::Home, ..
            } => {
                self.add_marketplace_input.cursor_home();
                EventResult::Consumed
            }
            Input { key: Key::End, .. } => {
                self.add_marketplace_input.cursor_end();
                EventResult::Consumed
            }
            // ── 跳词 ────────────────────────────────────────────────────
            Input {
                key: Key::Left,
                ctrl: true,
                ..
            } => {
                self.add_marketplace_input.cursor_word_left();
                EventResult::Consumed
            }
            Input {
                key: Key::Right,
                ctrl: true,
                ..
            } => {
                self.add_marketplace_input.cursor_word_right();
                EventResult::Consumed
            }
            // ── 删除 ────────────────────────────────────────────────────
            Input {
                key: Key::Backspace,
                alt: false,
                ..
            } => {
                self.add_marketplace_input.backspace();
                EventResult::Consumed
            }
            Input {
                key: Key::Backspace,
                alt: true,
                ..
            }
            | Input {
                key: Key::Char('w'),
                ctrl: true,
                ..
            } => {
                self.add_marketplace_input.delete_word_backward();
                EventResult::Consumed
            }
            Input {
                key: Key::Delete, ..
            } => {
                self.add_marketplace_input.delete();
                EventResult::Consumed
            }
            _ => EventResult::Consumed,
        }
    }
```

- [ ] **Step 2: Run `cargo build -p peri-tui` to verify compilation**

Run: `cargo build -p peri-tui 2>&1 | head -20`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/plugin_panel/handlers/plugin_handlers/marketplace.rs
git commit -m "feat(plugin_panel): use InputState editing methods in marketplace add

Replace raw insert/backspace with full cursor operations from InputState,
giving marketplace URL input the same editing capabilities.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 7: Integration verification

- [ ] **Step 1: Run full test suite**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -20
cargo test -p peri-widgets --lib 2>&1 | tail -20
```

Expected: All tests PASS

- [ ] **Step 2: Run `cargo clippy` on affected crates**

```bash
cargo clippy -p peri-tui -- -D warnings 2>&1 | tail -20
cargo clippy -p peri-widgets -- -D warnings 2>&1 | tail -20
```

Expected: No warnings

- [ ] **Step 3: Run `cargo fmt --check`**

```bash
cargo fmt --check
```

Expected: All files formatted correctly

- [ ] **Step 4: Final commit if any formatting changes needed**

```bash
git add -A && git commit -m "chore: format after popup input unification changes

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
