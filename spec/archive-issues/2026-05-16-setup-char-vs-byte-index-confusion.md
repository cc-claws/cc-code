> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-char-vs-byte-index-confusion.md
# 多处字符索引与字节长度混用比较

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

多个函数中字符索引（`cursor`）与字节长度（`buf.len()` / `mp.api_key.len()`）混用比较或操作。在纯 ASCII 文本中无害，但在 CJK 文本下存在逻辑歧义（当前恰巧不触发 panic，但语义不正确）。

## 具体位置

1. **`insert_at_cursor`**: `peri-tui/src/app/setup_wizard.rs:452`
   ```rust
   if *cursor > buf.len() { ... }  // 字符索引 vs 字节长度
   ```
   正确应为 `buf.chars().count()`

2. **`handle_edit_key` Backspace**: `peri-tui/src/app/mod.rs:645`
   ```rust
   if *cursor > 0 && *cursor <= buf.len() { ... }
   ```
   其他分支（Left/Right/End/Ctrl+K/U）均使用 `buf.chars().count()`

3. **`handle_edit_key` Delete**: `peri-tui/src/app/mod.rs:663`
   ```rust
   if *cursor < buf.len() { ... }
   ```
   同上

## 症状

当前不触发 panic（因为 `char_indices().nth()` 在越界时返回 None，有 `if let Some` 防护），但比较条件在 CJK 文本下过于宽松——cursor 可能超过合法字符数但仍 < 字节长度，导致无效操作进入分支。目前靠 `nth()` 返回 None 兜底，但逻辑上不干净。

## 期望

统一所有光标边界检查使用 `buf.chars().count()`。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `insert_at_cursor()` (line 451-462)
- `peri-tui/src/app/mod.rs` —— `handle_edit_key()` (line 617-751, 具体 645, 663)
