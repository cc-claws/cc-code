//! 键盘与鼠标事件处理：将 crossterm 事件转换为 TextEditor 操作。
//!
//! 提供 [`handle_key`]（键盘）和 [`handle_mouse`]（鼠标）两个公共入口，
//! 返回 `bool` 指示事件是否被编辑器消费。
//!
//! ## 快捷键对照（对标 VSCode）
//!
//! | 快捷键 | 功能 |
//! |--------|------|
//! | `Ctrl+Z` / `Ctrl+Shift+Z` | 撤销 / 重做 |
//! | `Ctrl+Y` | 重做 |
//! | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | 复制 / 剪切 / 粘贴 |
//! | `Ctrl+A` | 全选 |
//! | `Ctrl+S` | 保存 |
//! | `Ctrl+Left/Right` | 按单词移动 |
//! | `Ctrl+Backspace/Delete` | 按单词删除 |
//! | `Ctrl+Home/End` | 文件开头/末尾 |
//! | `Ctrl+D` | 复制当前行 |
//! | `Ctrl+L` | 选中当前行（重复按扩展） |
//! | `Ctrl+Shift+K` | 删除当前行 |
//! | `Ctrl+Shift+Left/Right` | 按单词选区 |
//! | `Ctrl+Shift+Home/End` | 选区到文件开头/末尾 |
//! | `Shift+方向键` | 扩展选区 |
//! | `Shift+Home/End` | 选区到行首/行末 |
//! | `Alt+Up/Down` | 上移/下移当前行 |
//! | `Tab` / `Shift+Tab` | 缩进 / 反缩进 |
//! | `Home` | 智能 Home（行首↔首个非空白） |

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::TextEditor;

// ── 键盘处理 ──

/// 处理键盘事件，返回 `true` 表示编辑器已消费该事件。
///
/// `Ctrl+S` 保存文件，`Esc` 和未知按键返回 `false` 交给父级处理。
pub fn handle_key(editor: &mut TextEditor, code: KeyCode, mods: KeyModifiers) -> bool {
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let shift = mods.contains(KeyModifiers::SHIFT);
    let alt = mods.contains(KeyModifiers::ALT);

    // 优先级：Ctrl+Shift > Ctrl > Alt > Shift > 无修饰
    if ctrl && shift {
        return handle_ctrl_shift(editor, code);
    }
    if ctrl {
        return handle_ctrl(editor, code);
    }
    if alt {
        return handle_alt(editor, code);
    }
    if shift {
        return handle_shift(editor, code);
    }
    handle_plain(editor, code)
}

/// 无修饰键处理。
fn handle_plain(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Char(ch) => {
            editor.insert_char(ch);
            true
        }
        KeyCode::Enter => {
            editor.insert_char('\n');
            true
        }
        KeyCode::Backspace => {
            editor.delete_backward();
            true
        }
        KeyCode::Delete => {
            editor.delete_forward();
            true
        }
        KeyCode::Tab => {
            editor.insert_tab();
            true
        }
        KeyCode::Up => {
            editor.move_up(false);
            true
        }
        KeyCode::Down => {
            editor.move_down(false);
            true
        }
        KeyCode::Left => {
            editor.move_left(false);
            true
        }
        KeyCode::Right => {
            editor.move_right(false);
            true
        }
        KeyCode::Home => {
            editor.move_smart_home(false);
            true
        }
        KeyCode::End => {
            editor.move_end(false);
            true
        }
        KeyCode::PageUp => {
            editor.set_scroll_y(editor.scroll_y().saturating_sub(20));
            true
        }
        KeyCode::PageDown => {
            editor.set_scroll_y(editor.scroll_y() + 20);
            true
        }
        KeyCode::Esc => false,
        _ => false,
    }
}

/// Shift 组合键：选区扩展。
fn handle_shift(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Up => {
            editor.move_up(true);
            true
        }
        KeyCode::Down => {
            editor.move_down(true);
            true
        }
        KeyCode::Left => {
            editor.move_left(true);
            true
        }
        KeyCode::Right => {
            editor.move_right(true);
            true
        }
        KeyCode::Home => {
            editor.move_smart_home(true);
            true
        }
        KeyCode::End => {
            editor.move_end(true);
            true
        }
        KeyCode::Tab => {
            editor.outdent_lines();
            true
        }
        // Shift+字母等不处理
        _ => false,
    }
}

/// Ctrl 组合键。
fn handle_ctrl(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('s') | KeyCode::Char('S') => {
            if let Err(e) = editor.save() {
                eprintln!("保存失败: {e}");
            }
            true
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            editor.undo();
            true
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            editor.redo();
            true
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            editor.select_all();
            true
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let text = editor.selected_text();
            if !text.is_empty() {
                copy_to_clipboard(&text);
            }
            true
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            let text = editor.selected_text();
            if !text.is_empty() {
                copy_to_clipboard(&text);
                editor.delete_selection();
            }
            true
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if let Some(text) = paste_from_clipboard() {
                editor.insert_text(&text);
            }
            true
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            editor.duplicate_line();
            true
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            editor.select_line();
            true
        }
        KeyCode::Left | KeyCode::Char('b') => {
            editor.move_word_left(false);
            true
        }
        KeyCode::Right | KeyCode::Char('f') => {
            editor.move_word_right(false);
            true
        }
        KeyCode::Home => {
            editor.move_file_start(false);
            true
        }
        KeyCode::End => {
            editor.move_file_end(false);
            true
        }
        KeyCode::Backspace => {
            editor.delete_word_backward();
            true
        }
        KeyCode::Delete => {
            editor.delete_word_forward();
            true
        }
        _ => false,
    }
}

/// Ctrl+Shift 组合键。
fn handle_ctrl_shift(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            editor.redo();
            true
        }
        KeyCode::Char('k') | KeyCode::Char('K') => {
            editor.delete_current_line();
            true
        }
        KeyCode::Left => {
            editor.move_word_left(true);
            true
        }
        KeyCode::Right => {
            editor.move_word_right(true);
            true
        }
        KeyCode::Home => {
            editor.move_file_start(true);
            true
        }
        KeyCode::End => {
            editor.move_file_end(true);
            true
        }
        _ => false,
    }
}

/// Alt 组合键。
///
/// macOS 终端 Option+←/→ 发送 `\x1bb`/`\x1bf`，crossterm 解析为 Alt+Char('b'/'f')。
/// 因此同时支持方向键和 readline 字符两种绑定。
fn handle_alt(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Up => {
            editor.move_line_up();
            true
        }
        KeyCode::Down => {
            editor.move_line_down();
            true
        }
        // macOS Option+← → Alt+Char('b') 或 Alt+Left
        KeyCode::Left | KeyCode::Char('b') => {
            editor.move_word_left(false);
            true
        }
        // macOS Option+→ → Alt+Char('f') 或 Alt+Right
        KeyCode::Right | KeyCode::Char('f') => {
            editor.move_word_right(false);
            true
        }
        // Alt+d: 删除光标后一个单词（readline 标准）
        KeyCode::Char('d') => {
            editor.delete_word_forward();
            true
        }
        KeyCode::Backspace => {
            editor.delete_word_backward();
            true
        }
        KeyCode::Delete => {
            editor.delete_word_forward();
            true
        }
        _ => false,
    }
}

// ── 鼠标处理 ──

/// 处理鼠标事件，返回 `true` 表示鼠标在编辑器区域内。
///
/// `area` 为编辑器整体布局区域（含 gutter），`gutter_width` 为行号列宽度。
pub fn handle_mouse(editor: &mut TextEditor, mouse: MouseEvent, area: Rect, gutter_w: u16) -> bool {
    // 点击区域判断
    if mouse.column < area.x
        || mouse.column >= area.x + area.width
        || mouse.row < area.y
        || mouse.row >= area.y + area.height
    {
        return false;
    }

    let rel_row = (mouse.row - area.y) as usize;
    // 内容区起始 x（gutter + 分隔符）
    let content_x = area.x + gutter_w + 1;
    // 内容区宽度
    let content_width = area.width.saturating_sub(gutter_w + 1 + 1) as usize;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.column < content_x {
                // 点击 gutter → 定位到对应视觉行的逻辑行首
                let pos = editor.screen_to_cursor(rel_row, 0, content_width);
                editor.click(pos.line, 0);
            } else {
                let rel_col = mouse.column.saturating_sub(content_x) as usize;
                let pos = editor.screen_to_cursor(rel_row, rel_col, content_width);
                editor.click(pos.line, pos.col);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if mouse.column < content_x {
                let pos = editor.screen_to_cursor(rel_row, 0, content_width);
                editor.drag(pos.line, 0);
            } else {
                let rel_col = mouse.column.saturating_sub(content_x) as usize;
                let pos = editor.screen_to_cursor(rel_row, rel_col, content_width);
                editor.drag(pos.line, pos.col);
            }
        }
        MouseEventKind::ScrollUp => {
            editor.set_scroll_y(editor.scroll_y().saturating_sub(3));
        }
        MouseEventKind::ScrollDown => {
            editor.set_scroll_y(editor.scroll_y() + 3); // set_scroll_y 内部钳位
        }
        _ => {}
    }

    true
}

// ── 剪贴板辅助 ──

/// 将文本复制到系统剪贴板。
fn copy_to_clipboard(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text);
    }
}

/// 从系统剪贴板粘贴文本。
fn paste_from_clipboard() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}
