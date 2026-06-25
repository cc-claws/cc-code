//! 文本编辑器核心类型与 TextEditor 结构体。
//!
//! 提供基于 [`Rope`](ropey::Rope) 的文件编辑能力，包括光标定位、选区管理、
//! undo/redo 栈、滚动偏移等。不涉及渲染逻辑，仅负责数据层。

pub mod input;
pub mod render;

use anyhow::{Context, Result};
use ratatui::style::Style;
use ropey::Rope;
use std::fs;
use std::path::PathBuf;
use unicode_width::UnicodeWidthChar;

/// 单行最大显示列数（防止超长行拖慢渲染）
pub(crate) const MAX_DISPLAY_COLS: usize = 2000;

/// Tab 显示宽度
const TAB_WIDTH: usize = 4;

/// 统一的字符显示宽度计算。
/// Tab 按 TAB_WIDTH 计算，其余按 unicode-width 库。
pub fn char_width(ch: char) -> usize {
    if ch == '\t' {
        TAB_WIDTH
    } else {
        UnicodeWidthChar::width(ch).unwrap_or(0)
    }
}

// ── CursorPos ──

/// 光标位置（行号和列号，均为字符索引，0-based）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub line: usize,
    pub col: usize,
}

impl CursorPos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for CursorPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CursorPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line.cmp(&other.line).then(self.col.cmp(&other.col))
    }
}

// ── EditAction ──

/// 编辑操作记录（用于 undo/redo）。
#[derive(Debug, Clone)]
pub enum EditAction {
    /// 在 `pos` 位置插入了 `text`。
    Insert { pos: CursorPos, text: String },
    /// 在 `pos` 位置删除了 `text`。
    Delete { pos: CursorPos, text: String },
    /// 交换两相邻行内容（line1 < line2），用于 move_line_up/down。
    SwapLines { line1: usize, line2: usize },
    /// 多步操作的原子组合，undo/redo 作为一个整体。
    Group { actions: Vec<EditAction> },
}

// ── TextEditor ──

/// 基于 Rope 的文本编辑器数据结构。
///
/// 管理 [ropey::Rope] 文本缓冲区、光标、选区、undo/redo 栈和滚动状态。
/// 不持有任何终端/渲染状态。
pub struct TextEditor {
    rope: Rope,
    path: PathBuf,
    cursor: CursorPos,
    selection_anchor: Option<CursorPos>,
    scroll_y: usize,
    scroll_x: usize,
    modified: bool,
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,

    // 语法高亮
    /// 每行的高亮结果。打开/编辑后增量重建，滚动时只读不写。
    highlight_cache: Vec<Option<Vec<(Style, String)>>>,
    /// 编辑后标记缓存需要重建
    highlight_dirty: bool,
    /// 防抖计时器：编辑后等 200ms 再重建高亮
    highlight_debounce: Option<std::time::Instant>,
    /// 增量高亮：当前已处理到的行号（0-based）
    highlight_progress: usize,
    /// syntect 状态检查点：(行号, HighlightState, ParseState)
    /// 每 BATCH_SIZE 行保存一次，恢复时从最近检查点开始
    highlight_checkpoints: Vec<(
        usize,
        syntect::highlighting::HighlightState,
        syntect::parsing::ParseState,
    )>,
}

/// 全量高亮的行数上限。超过此大小的文件降级为纯文本。
const HIGHLIGHT_MAX_LINES: usize = 10000;
/// 每次主循环增量高亮的行数上限
const HIGHLIGHT_BATCH_SIZE: usize = 200;

#[allow(dead_code)]
impl TextEditor {
    // ── 文件 I/O ──

    /// 从磁盘加载文件到 Rope 缓冲区。
    ///
    /// 文件不存在时创建空缓冲区。读取失败返回错误。
    pub fn open(path: PathBuf) -> Result<Self> {
        let rope = if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("无法读取文件: {}", path.display()))?;
            Rope::from(content)
        } else {
            Rope::new()
        };
        Ok(Self {
            rope,
            path: path.clone(),
            cursor: CursorPos::new(0, 0),
            selection_anchor: None,
            scroll_y: 0,
            scroll_x: 0,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            highlight_cache: Vec::new(),
            highlight_dirty: true,
            // 首次打开立即高亮（防抖已过期）
            highlight_debounce: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
            highlight_progress: 0,
            highlight_checkpoints: Vec::new(),
        })
    }

    /// 增量高亮一批行（从 highlight_progress 开始，最多 HIGHLIGHT_BATCH_SIZE 行）。
    /// 利用 highlight_checkpoints 避免每帧从头跑 syntect 状态。
    /// 返回 true 表示有更新（需要重绘）。
    fn rehighlight_batch(&mut self) -> bool {
        let total = self.rope.len_lines();
        if total == 0 || !self.highlight_dirty {
            return false;
        }

        // 超大文件降级为纯文本
        if total > HIGHLIGHT_MAX_LINES {
            self.highlight_cache.resize(total, None);
            self.highlight_dirty = false;
            self.highlight_progress = total;
            return false;
        }

        let ext = crate::ui::syntax::extension_from_path(self.path.to_str().unwrap_or(""));
        let syntax = match crate::ui::syntax::find_syntax(ext) {
            Some(s) => s,
            None => {
                self.highlight_cache.resize(total, None);
                self.highlight_dirty = false;
                self.highlight_progress = total;
                return false;
            }
        };

        // 确保 cache 容量
        if self.highlight_cache.len() != total {
            self.highlight_cache.resize(total, None);
        }

        let theme = crate::ui::syntax::get_theme();
        let ss = crate::ui::syntax::get_syntax_set();

        // 从最近的 checkpoint 恢复 syntect 状态
        let mut h = if let Some((cp_line, hs, ps)) = self
            .highlight_checkpoints
            .iter()
            .rev()
            .find(|(l, _, _)| *l <= self.highlight_progress)
        {
            // 从检查点恢复后，跑到 highlight_progress
            let mut hl = syntect::easy::HighlightLines::from_state(theme, hs.clone(), ps.clone());
            for i in *cp_line..self.highlight_progress {
                let _ = hl.highlight_line(&self.line_text(i), ss);
            }
            hl
        } else {
            syntect::easy::HighlightLines::new(syntax, theme)
        };

        let start = self.highlight_progress;
        let end = (start + HIGHLIGHT_BATCH_SIZE).min(total);

        for i in start..end {
            let line = self.line_text(i);
            let spans = match h.highlight_line(&self.line_text(i), ss) {
                Ok(segments) => segments
                    .into_iter()
                    .map(|(s, t)| (crate::ui::syntax::to_ratatui_style(s), t.to_string()))
                    .collect(),
                Err(_) => vec![(Style::default(), line)],
            };
            self.highlight_cache[i] = Some(spans);
        }

        // 批次结束后保存检查点（通过 state(self) 取出状态）
        let (hs, ps) = h.state();
        self.highlight_checkpoints.push((end, hs, ps));

        self.highlight_progress = end;
        if end >= total {
            self.highlight_dirty = false;
        }

        true
    }

    /// 主循环调用：增量高亮。
    ///
    /// 策略：脏标记 + 防抖 → 每帧增量处理 200 行 → 完成后清除脏标记。
    pub fn sync_highlight_visible(&mut self, _scroll_y: usize, _viewport_height: usize) -> bool {
        if !self.highlight_dirty {
            return false;
        }
        // 防抖：编辑后 200ms 内不重建（显示旧缓存）
        if let Some(t) = self.highlight_debounce {
            if t.elapsed() < std::time::Duration::from_millis(200) {
                return false;
            }
        }
        self.rehighlight_batch()
    }

    /// 兼容旧调用
    pub fn sync_highlight(&mut self) -> bool {
        if !self.highlight_dirty {
            return false;
        }
        self.rehighlight_batch()
    }

    /// 获取指定行的高亮 spans（如果已缓存）
    pub fn line_highlights(&self, line: usize) -> Option<&[(Style, String)]> {
        self.highlight_cache
            .get(line)
            .and_then(|opt| opt.as_deref())
    }

    /// 编辑后标记缓存失效
    fn invalidate_highlight(&mut self) {
        self.highlight_dirty = true;
        self.highlight_debounce = Some(std::time::Instant::now());
        self.highlight_progress = 0;
        self.highlight_checkpoints.clear();
        // 编辑后需要重跑，但不清缓存——防抖期间显示旧高亮
    }

    /// 将缓冲区内容写入磁盘。成功后清除 modified 标记。
    pub fn save(&mut self) -> Result<()> {
        let content = self.rope.to_string();
        fs::write(&self.path, &content)
            .with_context(|| format!("无法写入文件: {}", self.path.display()))?;
        self.modified = false;
        Ok(())
    }

    // ── 只读访问器 ──

    /// 文件是否被修改过（相对于打开时）。
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// 关联的文件路径。
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// 缓冲区总行数。
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// 当前光标位置。
    pub fn cursor(&self) -> CursorPos {
        self.cursor
    }

    /// 选区锚点（如果存在）。
    pub fn selection(&self) -> Option<CursorPos> {
        self.selection_anchor
    }

    /// 是否存在选区。
    pub fn has_selection(&self) -> bool {
        self.selection_anchor.is_some()
    }

    /// 返回有序选区范围 `(start, end)`，保证 start <= end。
    /// 无选区时返回 None。
    pub fn selection_range(&self) -> Option<(CursorPos, CursorPos)> {
        self.selection_anchor.map(|anchor| {
            let start = anchor.min(self.cursor);
            let end = anchor.max(self.cursor);
            (start, end)
        })
    }

    /// 获取选区文本。无选区时返回空字符串。
    pub fn selected_text(&self) -> String {
        match self.selection_anchor {
            Some(anchor) => {
                let start = anchor.min(self.cursor);
                let end = anchor.max(self.cursor);
                let start_idx = self.pos_to_char(start);
                let end_idx = self.pos_to_char(end);
                self.rope.slice(start_idx..end_idx).to_string()
            }
            None => String::new(),
        }
    }

    /// 清除选区。
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// 全选（锚点放在文件起始，光标放在文件末尾）。
    pub fn select_all(&mut self) {
        self.selection_anchor = Some(CursorPos::new(0, 0));
        let last_line = self.line_count().saturating_sub(1);
        let last_col = self.line_content_len(last_line);
        self.cursor = CursorPos::new(last_line, last_col);
    }

    // ── 行/位置工具 ──

    /// 指定行的内容长度（不含换行符）。
    pub fn line_content_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let rope_line = self.rope.line(line);
        // ropey 行末含 \n，len_chars() 包含它；最后非空行可能不含 \n
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    /// 获取指定行文本（不含换行符）。
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        let rope_line = self.rope.line(line);
        let len = rope_line.len_chars();
        if len > 0 && rope_line.char(len - 1) == '\n' {
            rope_line.slice(0..len - 1).to_string()
        } else {
            rope_line.to_string()
        }
    }

    /// 计算指定行的显示宽度（所有字符的 display width 之和）。
    pub fn line_display_width(&self, line: usize) -> usize {
        let text = self.line_text(line);
        text.chars().map(char_width).sum()
    }

    /// [`CursorPos`] 转换为 Rope 绝对字符索引。
    pub fn pos_to_char(&self, pos: CursorPos) -> usize {
        if pos.line == 0 {
            pos.col
        } else {
            // ropey line(i) 从行起始开始；行索引 + 列偏移
            let line_start = self.rope.line_to_char(pos.line);
            line_start + pos.col
        }
    }

    /// 将位置钳位到有效范围。
    pub fn clamp_pos(&self, pos: CursorPos) -> CursorPos {
        let line = pos.line.min(self.line_count().saturating_sub(1));
        let col = pos.col.min(self.line_content_len(line));
        CursorPos::new(line, col)
    }

    /// 将当前光标钳位到有效范围。
    pub fn clamp_cursor(&mut self) {
        self.cursor = self.clamp_pos(self.cursor);
    }

    // ── 编辑操作 ──

    /// 在光标位置插入单个字符。若存在选区则先删除。
    pub fn insert_char(&mut self, ch: char) {
        self.delete_selection_if_any();
        let pos = self.cursor;
        let char_idx = self.pos_to_char(pos);
        self.rope.insert_char(char_idx, ch);
        // 更新光标
        if ch == '\n' {
            self.cursor = CursorPos::new(pos.line + 1, 0);
        } else {
            self.cursor = CursorPos::new(pos.line, pos.col + 1);
        }
        self.push_undo(EditAction::Insert {
            pos,
            text: ch.to_string(),
        });
        self.modified = true;
    }

    /// 在光标位置插入字符串（用于粘贴）。处理多行文本。
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection_if_any();
        let pos = self.cursor;
        let char_idx = self.pos_to_char(pos);
        self.rope.insert(char_idx, text);
        // 计算插入后光标位置
        let newline_count = text.chars().filter(|&c| c == '\n').count();
        if newline_count == 0 {
            self.cursor = CursorPos::new(pos.line, pos.col + text.chars().count());
        } else {
            let last_newline_offset = text.rfind('\n').unwrap();
            let last_line_col = text[last_newline_offset + 1..].chars().count();
            self.cursor = CursorPos::new(pos.line + newline_count, last_line_col);
        }
        self.push_undo(EditAction::Insert {
            pos,
            text: text.to_string(),
        });
        self.modified = true;
    }

    /// 退格删除：选区存在时删除选区，否则删除光标前一个字符或合并行。
    pub fn delete_backward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        if pos.col > 0 {
            // 删除光标前一个字符
            let char_idx = self.pos_to_char(pos);
            let removed = self.rope.char(char_idx - 1);
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor = CursorPos::new(pos.line, pos.col - 1);
            self.push_undo(EditAction::Delete {
                pos: CursorPos::new(pos.line, pos.col - 1),
                text: removed.to_string(),
            });
            self.modified = true;
        } else if pos.line > 0 {
            // 合并到上一行
            let prev_line_len = self.line_content_len(pos.line - 1);
            let char_idx = self.pos_to_char(pos);
            // 删除换行符（前一行末尾的 \n）
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor = CursorPos::new(pos.line - 1, prev_line_len);
            self.push_undo(EditAction::Delete {
                pos: CursorPos::new(pos.line - 1, prev_line_len),
                text: "\n".to_string(),
            });
            self.modified = true;
        }
    }

    /// 正向删除：选区存在时删除选区，否则删除光标处字符或合并下一行。
    pub fn delete_forward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        let line_len = self.line_content_len(pos.line);
        if pos.col < line_len {
            // 删除光标处字符
            let char_idx = self.pos_to_char(pos);
            let removed = self.rope.char(char_idx);
            self.rope.remove(char_idx..char_idx + 1);
            self.push_undo(EditAction::Delete {
                pos,
                text: removed.to_string(),
            });
            self.modified = true;
        } else if pos.line + 1 < self.line_count() {
            // 合并下一行（删除行末换行符）
            let char_idx = self.pos_to_char(pos);
            self.rope.remove(char_idx..char_idx + 1);
            self.push_undo(EditAction::Delete {
                pos,
                text: "\n".to_string(),
            });
            self.modified = true;
        }
    }

    /// 删除选区文本并返回被删除的内容。无选区时返回 None。
    pub fn delete_selection(&mut self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let start_idx = self.pos_to_char(start);
        let end_idx = self.pos_to_char(end);
        let text = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        self.cursor = start;
        self.selection_anchor = None;
        self.push_undo(EditAction::Delete {
            pos: start,
            text: text.clone(),
        });
        self.modified = true;
        Some(text)
    }

    /// 内部辅助：如果存在选区则删除之。
    pub fn delete_selection_if_any(&mut self) {
        self.delete_selection();
    }

    // ── 单词级编辑 ──

    /// 向左删除一个单词（Ctrl+Backspace）。
    pub fn delete_word_backward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        if pos.col == 0 {
            // 行首 → 合并到上一行
            self.delete_backward();
            return;
        }
        let new_col = self.find_word_boundary_left(pos.line, pos.col);
        let start_col = if new_col < pos.col { new_col } else { 0 };
        if start_col == pos.col {
            return;
        }
        let start = CursorPos::new(pos.line, start_col);
        let start_idx = self.pos_to_char(start);
        let end_idx = self.pos_to_char(pos);
        let text = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        self.cursor = start;
        self.push_undo(EditAction::Delete { pos: start, text });
        self.modified = true;
    }

    /// 向右删除一个单词（Ctrl+Delete）。
    pub fn delete_word_forward(&mut self) {
        if self.selection_anchor.is_some() {
            self.delete_selection();
            return;
        }
        let pos = self.cursor;
        let line_len = self.line_content_len(pos.line);
        if pos.col >= line_len {
            // 行末 → 合并下一行
            self.delete_forward();
            return;
        }
        let new_col = self.find_word_boundary_right(pos.line, pos.col);
        let end_col = if new_col > pos.col { new_col } else { line_len };
        if end_col == pos.col {
            return;
        }
        let end = CursorPos::new(pos.line, end_col);
        let start_idx = self.pos_to_char(pos);
        let end_idx = self.pos_to_char(end);
        let text = self.rope.slice(start_idx..end_idx).to_string();
        self.rope.remove(start_idx..end_idx);
        self.push_undo(EditAction::Delete { pos, text });
        self.modified = true;
    }

    // ── 行级操作 ──

    /// 复制当前行并插入到下方（Ctrl+D）。
    pub fn duplicate_line(&mut self) {
        let line = self.cursor.line;
        let line_text = self.line_text(line);
        let insert_text = format!("{}\n", line_text);
        let char_idx = self.pos_to_char(CursorPos::new(line + 1, 0));
        self.rope.insert(char_idx, &insert_text);
        self.cursor.line += 1;
        // 保持 col 不变
        self.push_undo(EditAction::Insert {
            pos: CursorPos::new(line + 1, 0),
            text: insert_text,
        });
        self.modified = true;
    }

    /// 将当前行上移一行（Alt+Up）。
    pub fn move_line_up(&mut self) {
        let line = self.cursor.line;
        if line == 0 {
            return;
        }
        let target = line - 1;
        self.do_swap_lines(target, line);
        self.cursor.line = target;
        self.push_undo(EditAction::SwapLines {
            line1: target,
            line2: line,
        });
        self.modified = true;
    }

    /// 将当前行下移一行（Alt+Down）。
    pub fn move_line_down(&mut self) {
        let line = self.cursor.line;
        if line + 1 >= self.line_count() {
            return;
        }
        let target = line + 1;
        self.do_swap_lines(line, target);
        self.cursor.line = target;
        self.push_undo(EditAction::SwapLines {
            line1: line,
            line2: target,
        });
        self.modified = true;
    }

    /// 交换两相邻行的内容（line1 < line2）。
    fn do_swap_lines(&mut self, line1: usize, line2: usize) {
        let l1_content = self.line_text(line1);
        let l2_content = self.line_text(line2);
        let l1_raw_len = self.rope.line(line1).len_chars();
        let l2_raw_len = self.rope.line(line2).len_chars();
        // l2 可能是最后一行（无尾部 \n），需要记录以便重建
        let l2_had_nl = l2_raw_len > 0 && self.rope.line(line2).char(l2_raw_len - 1) == '\n';
        let start = self.rope.line_to_char(line1);
        let end = start + l1_raw_len + l2_raw_len;
        self.rope.remove(start..end);
        // l1 一定有尾部 \n（因为 l2 = l1+1），重建时 l2 在前、l1 在后
        let new_text = format!(
            "{}\n{}{}",
            l2_content,
            l1_content,
            if l2_had_nl { "\n" } else { "" }
        );
        self.rope.insert(start, &new_text);
        self.invalidate_highlight();
    }

    /// 选中当前行（Ctrl+L）。重复按可扩展到下一行。
    pub fn select_line(&mut self) {
        if let Some(anchor) = self.selection_anchor {
            // 已有选区 → 扩展到下一行
            let end_line = anchor.max(self.cursor).line;
            let next = (end_line + 1).min(self.line_count().saturating_sub(1));
            self.cursor = CursorPos::new(next, self.line_content_len(next));
            // anchor 不变
        } else {
            // 无选区 → 选中当前行
            let line = self.cursor.line;
            self.selection_anchor = Some(CursorPos::new(line, 0));
            self.cursor = CursorPos::new(line, self.line_content_len(line));
        }
    }

    /// 删除当前行（Ctrl+Shift+K）。
    pub fn delete_current_line(&mut self) {
        let line = self.cursor.line;
        if line >= self.line_count() {
            return;
        }
        let line_chars = self.rope.line(line).len_chars();
        let char_idx = self.rope.line_to_char(line);
        let text = self.rope.line(line).to_string();
        self.rope.remove(char_idx..char_idx + line_chars);
        // 光标移到下一行（现在是当前行）的相同行号
        let total = self.line_count();
        if total == 0 {
            self.cursor = CursorPos::new(0, 0);
        } else {
            let new_line = line.min(total.saturating_sub(1));
            self.cursor = CursorPos::new(
                new_line,
                self.cursor.col.min(self.line_content_len(new_line)),
            );
        }
        self.selection_anchor = None;
        self.push_undo(EditAction::Delete {
            pos: CursorPos::new(line, 0),
            text,
        });
        self.modified = true;
    }

    // ── 缩进 ──

    /// 插入 Tab 或缩进选区（Tab 键）。
    pub fn insert_tab(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            // 多行选区 → 缩进所有行
            let mut actions = Vec::new();
            for line in start.line..=end.line.min(self.line_count().saturating_sub(1)) {
                let pos = CursorPos::new(line, 0);
                let char_idx = self.pos_to_char(pos);
                self.rope.insert_char(char_idx, '\t');
                actions.push(EditAction::Insert {
                    pos,
                    text: "\t".to_string(),
                });
            }
            // 调整选区：col += 1
            if let Some(anchor) = self.selection_anchor {
                self.selection_anchor = Some(CursorPos::new(anchor.line, anchor.col + 1));
            }
            self.cursor.col += 1;
            self.push_undo(EditAction::Group { actions });
            self.modified = true;
        } else {
            // 无选区 → 插入 Tab 字符
            self.insert_char('\t');
        }
    }

    /// 反缩进当前行或选区（Shift+Tab）。
    pub fn outdent_lines(&mut self) {
        let (start_line, end_line) = if let Some((start, end)) = self.selection_range() {
            (start.line, end.line)
        } else {
            (self.cursor.line, self.cursor.line)
        };
        let mut actions = Vec::new();
        let mut any_removed = false;
        for line in start_line..=end_line.min(self.line_count().saturating_sub(1)) {
            let text = self.line_text(line);
            let (remove_col, remove_len) = if text.starts_with('\t') {
                (0, 1)
            } else {
                // 最多移除 TAB_WIDTH 个前导空格
                let spaces = text.chars().take_while(|&c| c == ' ').count();
                (0, spaces.min(TAB_WIDTH))
            };
            if remove_len > 0 {
                let pos = CursorPos::new(line, remove_col);
                let char_idx = self.pos_to_char(pos);
                let removed: String = text.chars().skip(remove_col).take(remove_len).collect();
                self.rope.remove(char_idx..char_idx + remove_len);
                actions.push(EditAction::Delete { pos, text: removed });
                any_removed = true;
                // 调整选区/光标
                if line == self.cursor.line && self.cursor.col > 0 {
                    self.cursor.col = self.cursor.col.saturating_sub(remove_len);
                }
                if let Some(anchor) = self.selection_anchor {
                    if line == anchor.line && anchor.col > 0 {
                        self.selection_anchor = Some(CursorPos::new(
                            anchor.line,
                            anchor.col.saturating_sub(remove_len),
                        ));
                    }
                }
            }
        }
        if any_removed {
            self.push_undo(EditAction::Group { actions });
            self.modified = true;
        }
    }

    /// 撤销上一步操作。
    pub fn undo(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            return;
        };
        self.apply_inverse(&action);
        self.redo_stack.push(action);
    }

    /// 重做上一步操作。
    pub fn redo(&mut self) {
        let Some(action) = self.redo_stack.pop() else {
            return;
        };
        self.apply_forward(&action);
        self.undo_stack.push(action);
    }

    /// 将编辑操作推入 undo 栈。合并同类连续单字符操作：
    /// - Insert：同行相邻位置的单字符插入
    /// - Delete：同行相邻位置的单字符删除（正向删除或退格）
    pub fn push_undo(&mut self, action: EditAction) {
        // SwapLines 和 Group 不参与合并
        if matches!(
            action,
            EditAction::SwapLines { .. } | EditAction::Group { .. }
        ) {
            self.undo_stack.push(action);
            self.redo_stack.clear();
            if self.undo_stack.len() > 10000 {
                self.undo_stack.remove(0);
            }
            self.invalidate_highlight();
            return;
        }
        // 尝试合并连续单字符操作
        if self.try_merge_undo(&action) {
            self.redo_stack.clear();
            return;
        }
        // 未合并，正常压栈
        self.undo_stack.push(action);
        self.redo_stack.clear();
        // 栈容量上限
        if self.undo_stack.len() > 10000 {
            self.undo_stack.remove(0);
        }
        // 编辑后使高亮失效
        self.invalidate_highlight();
    }

    /// 尝试将 action 合并到 undo 栈顶。返回 true 表示合并成功。
    fn try_merge_undo(&mut self, action: &EditAction) -> bool {
        match action {
            EditAction::Insert { pos: new_pos, text } => {
                if text.chars().count() != 1 || text.contains('\n') {
                    return false;
                }
                let Some(EditAction::Insert {
                    pos: prev_pos,
                    text: ref mut prev_text,
                }) = self.undo_stack.last_mut()
                else {
                    return false;
                };
                if prev_pos.line == new_pos.line
                    && prev_pos.col + prev_text.chars().count() == new_pos.col
                {
                    prev_text.push_str(text);
                    return true;
                }
                false
            }
            EditAction::Delete { pos: new_pos, text } => {
                if text.chars().count() != 1 || text.contains('\n') {
                    return false;
                }
                let Some(EditAction::Delete {
                    pos: ref mut prev_pos,
                    text: ref mut prev_text,
                }) = self.undo_stack.last_mut()
                else {
                    return false;
                };
                if prev_pos.line != new_pos.line {
                    return false;
                }
                // 正向删除（Delete 键）：pos 不变，追加到末尾
                if new_pos == prev_pos {
                    prev_text.push_str(text);
                    return true;
                }
                // 退格（Backspace）：pos 递减，前置插入
                if new_pos.col + 1 == prev_pos.col {
                    prev_text.insert_str(0, text);
                    *prev_pos = *new_pos;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// 反向应用编辑操作（用于 undo）。
    fn apply_inverse(&mut self, action: &EditAction) {
        match action {
            EditAction::Insert { pos, text } => {
                let start_idx = self.pos_to_char(*pos);
                let end_idx = start_idx + text.chars().count();
                self.rope.remove(start_idx..end_idx);
                self.cursor = *pos;
            }
            EditAction::Delete { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.insert(char_idx, text);
                self.cursor = *pos;
            }
            EditAction::SwapLines { line1, line2 } => {
                // 交换回来（swap 的逆操作就是再 swap 一次）
                self.do_swap_lines(*line1, *line2);
                // 光标回到原来位置（较高行号）
                let col = self.cursor.col.min(self.line_content_len(*line2));
                self.cursor = CursorPos::new(*line2, col);
            }
            EditAction::Group { actions } => {
                for action in actions.iter().rev() {
                    self.apply_inverse(action);
                }
            }
        }
        self.clamp_cursor();
        self.invalidate_highlight();
    }

    /// 正向应用编辑操作（用于 redo）。
    fn apply_forward(&mut self, action: &EditAction) {
        match action {
            EditAction::Insert { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.insert(char_idx, text);
                // 光标移到插入文本末尾
                let newline_count = text.chars().filter(|&c| c == '\n').count();
                if newline_count == 0 {
                    self.cursor = CursorPos::new(pos.line, pos.col + text.chars().count());
                } else {
                    let last_nl = text.rfind('\n').unwrap();
                    let last_col = text[last_nl + 1..].chars().count();
                    self.cursor = CursorPos::new(pos.line + newline_count, last_col);
                }
            }
            EditAction::Delete { pos, text } => {
                let char_idx = self.pos_to_char(*pos);
                self.rope.remove(char_idx..char_idx + text.chars().count());
                self.cursor = *pos;
            }
            EditAction::SwapLines { line1, line2 } => {
                self.do_swap_lines(*line1, *line2);
                // 光标移到较低行号（move up 后的位置）
                let col = self.cursor.col.min(self.line_content_len(*line1));
                self.cursor = CursorPos::new(*line1, col);
            }
            EditAction::Group { actions } => {
                for action in actions {
                    self.apply_forward(action);
                }
            }
        }
        self.clamp_cursor();
        self.invalidate_highlight();
    }

    // ── 光标移动 ──

    /// 光标上移一行，列钳位到目标行内容长度。
    pub fn move_up(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
        }
    }

    /// 光标下移一行，列钳位到目标行内容长度。
    pub fn move_down(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = self.cursor.col.min(self.line_content_len(self.cursor.line));
        }
    }

    /// 光标左移一字符，到行首时跳到上一行末尾。
    pub fn move_left(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_content_len(self.cursor.line);
        }
    }

    /// 光标右移一字符，到行末时跳到下一行行首。
    pub fn move_right(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        let line_len = self.line_content_len(self.cursor.line);
        if self.cursor.col < line_len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
    }

    /// 光标移到当前行行首（col = 0）。
    pub fn move_home(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        self.cursor.col = 0;
    }

    /// 光标移到当前行行末（col = line_content_len）。
    pub fn move_end(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        self.cursor.col = self.line_content_len(self.cursor.line);
    }

    /// 智能 Home：在行首和第一个非空白字符之间切换（VSCode 行为）。
    pub fn move_smart_home(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        let first_non_ws = self.first_non_whitespace_col(self.cursor.line);
        if self.cursor.col == 0 {
            // 在行首 → 跳到第一个非空白
            self.cursor.col = first_non_ws;
        } else if self.cursor.col <= first_non_ws {
            // 在第一个非空白或之前 → 跳到行首
            self.cursor.col = 0;
        } else {
            // 其他位置 → 跳到第一个非空白
            self.cursor.col = first_non_ws;
        }
    }

    /// 按单词左移（Ctrl+Left）。
    pub fn move_word_left(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        let pos = self.cursor;
        if pos.col == 0 {
            if pos.line > 0 {
                self.cursor = CursorPos::new(pos.line - 1, self.line_content_len(pos.line - 1));
            }
            return;
        }
        let new_col = self.find_word_boundary_left(pos.line, pos.col);
        if new_col < pos.col {
            self.cursor.col = new_col;
        } else {
            self.cursor.col = 0;
        }
    }

    /// 按单词右移（Ctrl+Right）。
    pub fn move_word_right(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        let pos = self.cursor;
        let line_len = self.line_content_len(pos.line);
        if pos.col >= line_len {
            if pos.line + 1 < self.line_count() {
                self.cursor = CursorPos::new(pos.line + 1, 0);
            }
            return;
        }
        let new_col = self.find_word_boundary_right(pos.line, pos.col);
        if new_col > pos.col {
            self.cursor.col = new_col;
        } else {
            self.cursor.col = line_len;
        }
    }

    /// 光标移到文件开头（Ctrl+Home）。
    pub fn move_file_start(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        self.cursor = CursorPos::new(0, 0);
    }

    /// 光标移到文件末尾（Ctrl+End）。
    pub fn move_file_end(&mut self, extend: bool) {
        self.begin_selection_if_extending(extend);
        let last_line = self.line_count().saturating_sub(1);
        self.cursor = CursorPos::new(last_line, self.line_content_len(last_line));
    }

    // ── 鼠标交互 ──

    /// 鼠标点击：钳位位置后设置光标，清除选区。
    pub fn click(&mut self, line: usize, col: usize) {
        let pos = self.clamp_pos(CursorPos::new(line, col));
        self.cursor = pos;
        self.clear_selection();
    }

    /// 鼠标拖拽：钳位位置，若无锚点则先设置锚点，然后移动光标。
    pub fn drag(&mut self, line: usize, col: usize) {
        let pos = self.clamp_pos(CursorPos::new(line, col));
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.cursor = pos;
    }

    /// 调整 scroll_y 使光标行在视口内可见。
    pub fn scroll_to_cursor(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        let line = self.cursor.line;
        if line < self.scroll_y {
            self.scroll_y = line;
        } else if line >= self.scroll_y + viewport_height {
            self.scroll_y = line - viewport_height + 1;
        }
    }

    /// 调整 scroll_x 使光标的显示列在视口内可见。
    /// `content_width` 是内容区域的宽度（已减去 gutter、分隔符、滚动条）。
    pub fn scroll_to_cursor_x(&mut self, content_width: usize) {
        if content_width == 0 {
            return;
        }
        let display_col = self.char_idx_to_display_col(self.cursor.line, self.cursor.col);
        // 内容可见范围：[scroll_x, scroll_x + content_width)
        let visible_end = self.scroll_x + content_width;
        if display_col < self.scroll_x {
            // 光标在可见区域左侧，向左滚动（留 2 列边距）
            self.scroll_x = display_col.saturating_sub(2);
        } else if display_col >= visible_end {
            // 光标在可见区域右侧，向右滚动（留 2 列边距）
            self.scroll_x = display_col - content_width + 3;
        }
    }

    // ── 显示列转换 ──

    /// 将字符索引转换为显示列（累加显示宽度）。
    pub fn char_idx_to_display_col(&self, line: usize, char_idx: usize) -> usize {
        let text = self.line_text(line);
        let mut col = 0;
        for (i, ch) in text.chars().enumerate() {
            if i >= char_idx {
                break;
            }
            col += char_width(ch);
        }
        col
    }

    /// 将显示列转换为字符索引（在给定文本中）。
    ///
    /// 从文本开头累加显示宽度，返回不超过 target_display_col 的最大字符索引。
    pub fn display_col_to_char_idx(text: &str, target_display_col: usize) -> usize {
        let mut display_col = 0;
        for (idx, ch) in text.chars().enumerate() {
            if display_col >= target_display_col {
                return idx;
            }
            display_col += char_width(ch);
        }
        text.chars().count()
    }

    /// 将屏幕坐标转换为光标位置（考虑软折行）。
    ///
    /// - `rel_row`：视觉行索引（0 = 视口顶部）
    /// - `rel_col`：内容区内的显示列（0 = 内容区左边界，不含 gutter/sep）
    /// - `content_width`：内容区显示宽度
    pub fn screen_to_cursor(
        &self,
        rel_row: usize,
        rel_col: usize,
        content_width: usize,
    ) -> CursorPos {
        if content_width == 0 || self.rope.len_lines() == 0 {
            return CursorPos::new(0, 0);
        }

        let max_line = self.rope.len_lines().saturating_sub(1);
        let mut visual_row = 0;

        for logical_line in self.scroll_y..=max_line {
            let line_width = self.line_display_width(logical_line);
            let wrap_count = if line_width == 0 {
                1
            } else {
                line_width.div_ceil(content_width)
            };

            for wrap_idx in 0..wrap_count {
                if visual_row == rel_row {
                    // 找到了对应的视觉行
                    let col_offset = wrap_idx * content_width;
                    let actual_display_col = col_offset + rel_col;
                    let text = self.line_text(logical_line);
                    let char_col = Self::display_col_to_char_idx(&text, actual_display_col);
                    return CursorPos::new(logical_line, char_col);
                }
                visual_row += 1;
            }
        }

        // 超出视口 → 文件末尾
        CursorPos::new(max_line, self.line_content_len(max_line))
    }

    // ── 滚动访问器 ──

    /// 垂直滚动偏移（行号）。
    pub fn scroll_y(&self) -> usize {
        self.scroll_y
    }

    /// 水平滚动偏移（列号）。
    pub fn scroll_x(&self) -> usize {
        self.scroll_x
    }

    /// 设置垂直滚动偏移，钳位到文件末尾。
    pub fn set_scroll_y(&mut self, y: usize) {
        let max_y = self.rope.len_lines().saturating_sub(1);
        self.scroll_y = y.min(max_y);
    }

    /// 设置水平滚动偏移。
    pub fn set_scroll_x(&mut self, x: usize) {
        self.scroll_x = x;
    }

    // ── 辅助工具 ──

    /// 扩展选区时设置锚点；非扩展时清除选区。
    fn begin_selection_if_extending(&mut self, extend: bool) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
    }

    /// 判断字符是否为单词字符（字母、数字、下划线）。
    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    /// 向左查找单词边界：跳过空白，再跳过同类字符（单词字符或非空白标点）。
    fn find_word_boundary_left(&self, line: usize, col: usize) -> usize {
        let text = self.line_text(line);
        let chars: Vec<char> = text.chars().collect();
        let mut pos = col;
        // 跳过空白
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        if pos == 0 {
            return 0;
        }
        // 跳过同类字符
        let is_word = Self::is_word_char(chars[pos - 1]);
        while pos > 0
            && Self::is_word_char(chars[pos - 1]) == is_word
            && !chars[pos - 1].is_whitespace()
        {
            pos -= 1;
        }
        pos
    }

    /// 向右查找单词边界：跳过当前同类字符，再跳过空白停在下一个词首。
    fn find_word_boundary_right(&self, line: usize, col: usize) -> usize {
        let text = self.line_text(line);
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut pos = col;
        if pos >= len {
            return len;
        }
        // 跳过空白
        if chars[pos].is_whitespace() {
            while pos < len && chars[pos].is_whitespace() {
                pos += 1;
            }
            return pos;
        }
        // 跳过同类字符
        let is_word = Self::is_word_char(chars[pos]);
        while pos < len && Self::is_word_char(chars[pos]) == is_word && !chars[pos].is_whitespace()
        {
            pos += 1;
        }
        // 跳过后置空白，停在下一个词首
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        pos
    }

    /// 返回指定行第一个非空白字符的列索引。全空白返回 0。
    fn first_non_whitespace_col(&self, line: usize) -> usize {
        let text = self.line_text(line);
        for (i, ch) in text.chars().enumerate() {
            if !ch.is_whitespace() {
                return i;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    /// 测试打开文件并读取内容。
    #[test]
    fn test_open_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"hello\nworld\n").unwrap();
        }

        let editor = TextEditor::open(file_path.clone()).unwrap();
        assert_eq!(editor.line_count(), 3, "应有 3 行（含末尾空行）");
        assert_eq!(editor.line_text(0), "hello", "第一行内容");
        assert_eq!(editor.line_text(1), "world", "第二行内容");
        assert!(!editor.is_modified(), "刚打开不应标记为已修改");
    }

    /// 测试 line_text 处理无换行末尾行和越界行。
    #[test]
    fn test_line_text() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"foo\nbar").unwrap();
        }

        let editor = TextEditor::open(file_path).unwrap();
        assert_eq!(editor.line_text(0), "foo", "第一行应不含换行符");
        assert_eq!(editor.line_text(1), "bar", "最后一行无换行符");
        assert_eq!(editor.line_text(99), "", "越界行应返回空字符串");
    }

    // === 编辑操作测试 ===

    /// 辅助：创建内容为 text 的编辑器，光标在 (0,0)。
    fn make_editor(text: &str) -> TextEditor {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, text).unwrap();
        TextEditor::open(file_path).unwrap()
    }

    #[test]
    fn test_insert_char() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 0);
        ed.insert_char('x');
        assert_eq!(ed.line_text(0), "xab", "插入后应为 xab");
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "光标应在 col 1");
    }

    #[test]
    fn test_insert_char_newline() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 1);
        ed.insert_char('\n');
        assert_eq!(ed.line_text(0), "a", "第一行应为 a");
        assert_eq!(ed.line_text(1), "bc", "第二行应为 bc");
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "光标应在第二行行首");
    }

    #[test]
    fn test_delete_backward() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 2);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "ac", "删除后应为 ac");
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "光标应在 col 1");
    }

    #[test]
    fn test_delete_backward_merge_lines() {
        let mut ed = make_editor("ab\ncd");
        ed.cursor = CursorPos::new(1, 0);
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "abcd", "合并后应为 abcd");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在 col 2");
    }

    #[test]
    fn test_delete_forward() {
        let mut ed = make_editor("abc");
        ed.cursor = CursorPos::new(0, 0);
        ed.delete_forward();
        assert_eq!(ed.line_text(0), "bc", "删除后应为 bc");
        assert_eq!(ed.cursor, CursorPos::new(0, 0), "光标应在 col 0");
    }

    #[test]
    fn test_selection_and_delete() {
        let mut ed = make_editor("hello world");
        ed.cursor = CursorPos::new(0, 2);
        ed.selection_anchor = Some(CursorPos::new(0, 7));
        let deleted = ed.delete_selection();
        assert_eq!(deleted, Some("llo w".to_string()), "应删除 llo w");
        assert_eq!(ed.line_text(0), "heorld", "删除后应为 heorld");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在选区起始位置");
        assert!(!ed.has_selection(), "选区应已清除");
    }

    #[test]
    fn test_undo_redo() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 0);
        ed.insert_char('x');
        assert_eq!(ed.line_text(0), "xab");
        ed.insert_char('y');
        assert_eq!(ed.line_text(0), "xyab");
        // 两个连续单字符插入应合并
        ed.undo();
        assert_eq!(ed.line_text(0), "ab", "undo 合并操作后应恢复为 ab");
        assert_eq!(ed.cursor, CursorPos::new(0, 0), "光标应回到 col 0");
        ed.redo();
        assert_eq!(ed.line_text(0), "xyab", "redo 后应为 xyab");
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "光标应在 col 2");
    }

    #[test]
    fn test_insert_text_multiline() {
        let mut ed = make_editor("ab");
        ed.cursor = CursorPos::new(0, 1);
        ed.insert_text("x\ny");
        assert_eq!(ed.line_text(0), "ax", "第一行应为 ax");
        assert_eq!(ed.line_text(1), "yb", "第二行应为 yb");
        assert_eq!(ed.cursor, CursorPos::new(1, 1), "光标应在 (1,1)");
    }

    #[test]
    fn test_select_all() {
        let mut ed = make_editor("abc\ndef");
        ed.select_all();
        assert_eq!(ed.selected_text(), "abc\ndef", "全选应选中全部文本");
    }

    // === 光标移动与鼠标交互测试 ===

    #[test]
    fn test_cursor_movement() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        ed.move_up(false);
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "上移到第 0 行 col 1");
        ed.move_end(false);
        assert_eq!(ed.cursor, CursorPos::new(0, 3), "行末 col 3");
        ed.move_right(false);
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "行末右移跳到下一行行首");
        ed.move_home(false);
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "行首 col 0");
        ed.move_left(false);
        assert_eq!(ed.cursor, CursorPos::new(0, 3), "行首左移跳到上一行行末");
        ed.move_down(false);
        assert_eq!(ed.cursor, CursorPos::new(1, 3), "下移 col 钳位到行长度");
        assert_eq!(ed.cursor.col, 3, "def 长度 3，col 钳位到 3");
    }

    #[test]
    fn test_click_and_drag() {
        let mut ed = make_editor("hello world");
        ed.click(0, 2);
        assert_eq!(ed.cursor, CursorPos::new(0, 2), "点击设置光标");
        assert!(!ed.has_selection(), "点击后无选区");
        ed.drag(0, 7);
        assert_eq!(ed.cursor, CursorPos::new(0, 7), "拖拽移动光标");
        assert_eq!(
            ed.selection(),
            Some(CursorPos::new(0, 2)),
            "拖拽设置锚点为点击位置"
        );
        assert_eq!(ed.selected_text(), "llo w", "选区文本应为 llo w");
    }

    #[test]
    fn test_display_col_conversion() {
        // ASCII 字符宽度各 1
        assert_eq!(
            TextEditor::display_col_to_char_idx("abc", 2),
            2,
            "ASCII target 2 → char_idx 2"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("abc", 5),
            3,
            "ASCII target 超出 → char_idx 3（文本长度）"
        );
        // CJK 字符宽度各 2
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 2),
            1,
            "CJK target 2 → char_idx 1（display_col=0+2=2, 2>=2, return 1）"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 3),
            2,
            "CJK target 3 → char_idx 2（两个 CJK 共 4 列，超出文本长度）"
        );
        assert_eq!(
            TextEditor::display_col_to_char_idx("你好", 4),
            2,
            "CJK target 4 → char_idx 2（两个 CJK 共 4 显示列，超出）"
        );
        // Tab 测试
        assert_eq!(
            TextEditor::display_col_to_char_idx("a\tb", 4),
            2,
            "Tab target 4 → char_idx 2（a=1, \\t=4, 1+4=5>=4, return 2 即 'b'）"
        );
    }

    /// char_idx_to_display_col 在 CJK 下正确累加宽度
    #[test]
    fn test_char_idx_to_display_col() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "你好abc").unwrap();
        let ed = TextEditor::open(path).unwrap();
        // char_idx=0 → display 0, char_idx=1 → display 2, char_idx=2 → display 4
        assert_eq!(ed.char_idx_to_display_col(0, 0), 0);
        assert_eq!(ed.char_idx_to_display_col(0, 1), 2);
        assert_eq!(ed.char_idx_to_display_col(0, 2), 4);
        assert_eq!(ed.char_idx_to_display_col(0, 3), 5);
    }

    // === 新增功能测试 ===

    #[test]
    fn test_shift_selection() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        // Shift+Right: 扩展选区
        ed.move_right(true);
        assert_eq!(ed.cursor, CursorPos::new(1, 2));
        assert_eq!(
            ed.selection_anchor,
            Some(CursorPos::new(1, 1)),
            "锚点在起点"
        );
        assert_eq!(ed.selected_text(), "e", "选中一个字符");
        // Shift+Down: 扩展到下一行
        ed.move_down(true);
        assert_eq!(ed.selected_text(), "ef\ngh", "跨行选区");
        // 不带 extend 的移动清除选区
        ed.move_left(false);
        assert!(!ed.has_selection(), "普通移动清除选区");
    }

    #[test]
    fn test_word_movement() {
        let mut ed = make_editor("hello world_foo bar");
        ed.cursor = CursorPos::new(0, 0);
        // Ctrl+Right: 跳到 "world_foo" 开头
        ed.move_word_right(false);
        assert_eq!(ed.cursor.col, 6, "跳过 hello 后的空格");
        // Ctrl+Right: 跳到 "bar" 开头（world_foo 是一个单词）
        ed.move_word_right(false);
        assert_eq!(
            ed.cursor.col, 16,
            "world_foo 作为一个单词跳过，停在 bar 开头"
        );
        // Ctrl+Left: 回到 world_foo 开头
        ed.move_word_left(false);
        assert_eq!(ed.cursor.col, 6, "回到 world_foo 开头");
        // Ctrl+Left: 回到行首
        ed.move_word_left(false);
        assert_eq!(ed.cursor.col, 0, "回到行首");
    }

    #[test]
    fn test_word_movement_punctuation() {
        let mut ed = make_editor("foo.bar baz");
        ed.cursor = CursorPos::new(0, 5); // 光标在 'b'
        ed.move_word_left(false);
        assert_eq!(ed.cursor.col, 4, "跳过 . 到 'b' 之前");
        ed.move_word_left(false);
        assert_eq!(ed.cursor.col, 3, "跳过 . 作为一个独立段");
        ed.move_word_left(false);
        assert_eq!(ed.cursor.col, 0, "回到 foo 开头");
    }

    #[test]
    fn test_word_movement_cross_line() {
        let mut ed = make_editor("abc\ndef");
        ed.cursor = CursorPos::new(1, 0);
        // Ctrl+Left at line start → end of previous line
        ed.move_word_left(false);
        assert_eq!(ed.cursor, CursorPos::new(0, 3), "跳到上一行行末");
        ed.cursor = CursorPos::new(0, 3);
        // Ctrl+Right at line end → start of next line
        ed.move_word_right(false);
        assert_eq!(ed.cursor, CursorPos::new(1, 0), "跳到下一行行首");
    }

    #[test]
    fn test_delete_word() {
        let mut ed = make_editor("hello world test");
        ed.cursor = CursorPos::new(0, 12); // 光标在 'test' 的 t
        ed.delete_word_backward();
        assert_eq!(ed.line_text(0), "hello test", "删除 'world '");
        assert_eq!(ed.cursor.col, 6, "光标在删除后位置");
        ed.delete_word_forward();
        assert_eq!(ed.line_text(0), "hello ", "删除 'test'");
    }

    #[test]
    fn test_delete_word_undo() {
        let mut ed = make_editor("hello world");
        ed.cursor = CursorPos::new(0, 6);
        ed.delete_word_backward();
        assert_eq!(ed.line_text(0), "world");
        ed.undo();
        assert_eq!(ed.line_text(0), "hello world", "undo 恢复单词删除");
    }

    #[test]
    fn test_smart_home() {
        let mut ed = make_editor("    hello world");
        ed.cursor = CursorPos::new(0, 7); // 在 'e' 位置
        ed.move_smart_home(false);
        assert_eq!(ed.cursor.col, 4, "跳到第一个非空白字符");
        ed.move_smart_home(false);
        assert_eq!(ed.cursor.col, 0, "已经在非空白 → 跳到行首");
        ed.move_smart_home(false);
        assert_eq!(ed.cursor.col, 4, "在行首 → 跳到非空白");
    }

    #[test]
    fn test_file_start_end() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(2, 1);
        ed.move_file_start(false);
        assert_eq!(ed.cursor, CursorPos::new(0, 0), "文件开头");
        ed.move_file_end(false);
        assert_eq!(ed.cursor, CursorPos::new(2, 3), "文件末尾");
    }

    #[test]
    fn test_duplicate_line() {
        let mut ed = make_editor("abc\ndef");
        ed.cursor = CursorPos::new(0, 2);
        ed.duplicate_line();
        assert_eq!(ed.line_text(0), "abc", "原行不变");
        assert_eq!(ed.line_text(1), "abc", "复制行在下方");
        assert_eq!(ed.line_text(2), "def", "原第二行下移");
        assert_eq!(ed.cursor, CursorPos::new(1, 2), "光标在复制行相同列");
        ed.undo();
        assert_eq!(ed.line_text(0), "abc", "undo 恢复");
        assert_eq!(ed.line_text(1), "def", "undo 恢复第二行");
        assert_eq!(ed.line_count(), 2, "undo 恢复为 2 行");
    }

    #[test]
    fn test_move_line_up_down() {
        let mut ed = make_editor("aaa\nbbb\nccc");
        ed.cursor = CursorPos::new(1, 1);
        // move up: bbb 和 aaa 交换
        ed.move_line_up();
        assert_eq!(ed.line_text(0), "bbb", "bbb 移到第一行");
        assert_eq!(ed.line_text(1), "aaa", "aaa 移到第二行");
        assert_eq!(ed.line_text(2), "ccc", "ccc 不变");
        assert_eq!(ed.cursor, CursorPos::new(0, 1), "光标跟随上移");
        // undo
        ed.undo();
        assert_eq!(ed.line_text(0), "aaa", "undo 恢复");
        assert_eq!(ed.line_text(1), "bbb");
        assert_eq!(ed.line_text(2), "ccc");
        assert_eq!(ed.cursor, CursorPos::new(1, 1), "光标恢复");
        // move down: bbb 和 ccc 交换
        ed.move_line_down();
        assert_eq!(ed.line_text(0), "aaa");
        assert_eq!(ed.line_text(1), "ccc", "ccc 上移");
        assert_eq!(ed.line_text(2), "bbb", "bbb 下移");
        assert_eq!(ed.cursor, CursorPos::new(2, 1), "光标跟随下移");
    }

    #[test]
    fn test_move_line_up_at_first_line() {
        let mut ed = make_editor("abc\ndef");
        ed.cursor = CursorPos::new(0, 1);
        ed.move_line_up();
        assert_eq!(ed.line_text(0), "abc", "首行不动");
        assert_eq!(ed.cursor, CursorPos::new(0, 1));
    }

    #[test]
    fn test_select_line() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        // 第一次 Ctrl+L: 选中当前行
        ed.select_line();
        assert_eq!(ed.selected_text(), "def", "选中整行");
        // 第二次 Ctrl+L: 扩展到下一行
        ed.select_line();
        assert_eq!(ed.selected_text(), "def\nghi", "扩展选区");
    }

    #[test]
    fn test_delete_current_line() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        ed.delete_current_line();
        assert_eq!(ed.line_text(0), "abc");
        assert_eq!(ed.line_text(1), "ghi", "def 被删除");
        assert_eq!(ed.cursor, CursorPos::new(1, 1), "光标在下一行");
        ed.undo();
        assert_eq!(ed.line_text(1), "def", "undo 恢复");
    }

    #[test]
    fn test_indent_outdent() {
        let mut ed = make_editor("abc\ndef");
        ed.cursor = CursorPos::new(0, 1);
        // Tab 插入（无选区 → 在光标位置插入）
        ed.insert_tab();
        assert_eq!(ed.line_text(0), "a\tbc", "在光标位置插入 Tab");
        assert_eq!(ed.cursor.col, 2, "光标在 Tab 后");
        // 反缩进：移除行首 Tab（需要先将光标移到行首，再 outdent）
        // 当前光标在 col 2，outdent 会尝试移除行首的 tab（但 tab 不在行首）
        // outdent 只移除行首的缩进，当前行 "a\tbc" 行首是 'a' 不是 tab，不会移除
        // 先测试行首 Tab 的场景
        ed.move_home(false);
        // 现在光标在 col 0
        ed.insert_tab();
        assert_eq!(ed.line_text(0), "\ta\tbc", "在行首插入 Tab");
        // 反缩进
        ed.outdent_lines();
        assert_eq!(ed.line_text(0), "a\tbc", "移除行首 Tab");
    }

    #[test]
    fn test_indent_outdent_selection() {
        let mut ed = make_editor("abc\ndef\nghi");
        // 选中第 0 和第 1 行
        ed.selection_anchor = Some(CursorPos::new(0, 0));
        ed.cursor = CursorPos::new(1, 3);
        ed.insert_tab();
        assert_eq!(ed.line_text(0), "\tabc", "第 0 行缩进");
        assert_eq!(ed.line_text(1), "\tdef", "第 1 行缩进");
        assert_eq!(ed.line_text(2), "ghi", "第 2 行不变");
        // undo 作为一步
        ed.undo();
        assert_eq!(ed.line_text(0), "abc", "undo 缩进");
        assert_eq!(ed.line_text(1), "def");
    }

    #[test]
    fn test_ctrl_shift_z_redo() {
        // Ctrl+Shift+Z 和 Ctrl+Y 都触发 redo，这里测试 redo 栈行为
        let mut ed = make_editor("ab");
        ed.insert_char('x');
        ed.undo();
        assert_eq!(ed.line_text(0), "ab");
        ed.redo();
        assert_eq!(ed.line_text(0), "xab");
    }

    /// 连续退格应合并为一次 undo
    #[test]
    fn test_undo_merge_backspace() {
        let mut ed = make_editor("abcde");
        ed.cursor = CursorPos::new(0, 5);
        // 连续 3 次退格
        ed.delete_backward(); // 删 e
        ed.delete_backward(); // 删 d
        ed.delete_backward(); // 删 c
        assert_eq!(ed.line_text(0), "ab", "连续退格后应为 ab");
        // undo 一次应恢复全部
        ed.undo();
        assert_eq!(ed.line_text(0), "abcde", "undo 一次恢复所有退格");
    }

    /// 连续正向删除应合并为一次 undo
    #[test]
    fn test_undo_merge_forward_delete() {
        let mut ed = make_editor("abcde");
        ed.cursor = CursorPos::new(0, 0);
        ed.delete_forward(); // 删 a
        ed.delete_forward(); // 删 b
        ed.delete_forward(); // 删 c
        assert_eq!(ed.line_text(0), "de", "连续正向删除后应为 de");
        ed.undo();
        assert_eq!(ed.line_text(0), "abcde", "undo 一次恢复所有正向删除");
    }

    /// 不同类型的操作不合并（插入后删除是两次 undo）
    #[test]
    fn test_undo_no_cross_merge() {
        let mut ed = make_editor("ab");
        ed.insert_char('x');
        ed.delete_backward();
        assert_eq!(ed.line_text(0), "ab");
        // undo 删除 → 恢复 x
        ed.undo();
        assert_eq!(ed.line_text(0), "xab", "undo 删除恢复 x");
        // undo 插入 → 移除 x
        ed.undo();
        assert_eq!(ed.line_text(0), "ab", "undo 插入移除 x");
    }

    #[test]
    fn test_ctrl_home_end_selection() {
        let mut ed = make_editor("abc\ndef\nghi");
        ed.cursor = CursorPos::new(1, 1);
        ed.move_file_start(true);
        assert_eq!(ed.selected_text(), "abc\nd", "选到文件开头");
        ed.move_file_end(true);
        assert_eq!(
            ed.selected_text(),
            "ef\nghi",
            "选到文件末尾（从原光标 d 之后）"
        );
    }

    /// 验证常用语言的语法高亮是否可用
    #[test]
    fn test_syntax_detection_common_languages() {
        let cases = [
            ("py", "Python"),
            ("rs", "Rust"),
            ("js", "JavaScript"),
            ("ts", "TypeScript"),
            ("go", "Go"),
            ("json", "JSON"),
            ("yaml", "YAML"),
        ];
        for (ext, expected_name) in cases {
            let path = format!("test.{}", ext);
            let ext_str = crate::ui::syntax::extension_from_path(&path);
            let syntax = crate::ui::syntax::find_syntax(ext_str);
            match syntax {
                Some(s) => assert!(
                    s.name.contains(expected_name),
                    "ext={}: expected name containing '{}', got '{}'",
                    ext,
                    expected_name,
                    s.name
                ),
                None => panic!("ext={}: 未找到语法定义", ext),
            }
        }
    }

    /// 验证 Python 文件增量高亮能正常产出
    #[test]
    fn test_rehighlight_batch_python() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        {
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"def hello():\n    print('world')\n    return 42\n")
                .unwrap();
        }

        let mut ed = TextEditor::open(file_path).unwrap();
        assert!(ed.highlight_dirty, "打开后应标记为 dirty");
        assert!(ed.highlight_cache.is_empty(), "初始缓存为空");

        let changed = ed.rehighlight_batch();
        assert!(changed, "应有更新");

        assert!(!ed.highlight_dirty, "小文件应一次完成");
        assert_eq!(ed.highlight_cache.len(), 4, "4 行（含末尾空行）");
        for (i, cache) in ed.highlight_cache.iter().enumerate() {
            assert!(cache.is_some(), "第 {} 行应有高亮", i);
        }
    }
}
