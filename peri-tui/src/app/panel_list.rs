use ratatui::layout::Rect;

/// 统一面板列表状态管理器
///
/// 封装 cursor / scroll_offset / items，提供统一的键盘导航、
/// 鼠标点击和滚轮滚动处理。面板不再直接管理这些字段。
#[derive(Clone, Debug)]
pub struct PanelList<T> {
    items: Vec<T>,
    cursor: usize,
    scroll_offset: u16,
}

impl<T> PanelList<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
        }
    }

    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.clamp_cursor();
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 移动光标（clamp 模式，不循环）
    pub fn move_cursor(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() - 1;
        let new = self.cursor as isize + delta;
        self.cursor = new.clamp(0, max as isize) as usize;
    }

    /// 将光标直接设置到指定位置（clamp 模式）
    pub fn move_cursor_to(&mut self, pos: usize) {
        if self.items.is_empty() {
            return;
        }
        self.cursor = pos.min(self.items.len() - 1);
    }

    /// 处理滚轮滚动（clamp 到合法范围）
    pub fn handle_scroll(&mut self, lines: i16, visible_height: u16) {
        if self.items.is_empty() || visible_height == 0 {
            return;
        }
        let content_height = self.items.len() as u16;
        let max_scroll = content_height.saturating_sub(visible_height);
        let new_offset = self.scroll_offset as i16 + lines;
        self.scroll_offset = (new_offset.clamp(0, max_scroll as i16)) as u16;
    }

    /// 处理鼠标点击，返回是否命中了有效 item
    ///
    /// `border_top` 是面板边框占用的行数（如标题行+边框）
    pub fn handle_mouse_click(
        &mut self,
        mouse_row: u16,
        _mouse_col: u16,
        area: Rect,
        border_top: u16,
    ) -> bool {
        let relative_y = mouse_row.saturating_sub(area.y);
        if relative_y < border_top {
            return false;
        }
        let item_row = relative_y - border_top;
        let idx = item_row as usize + self.scroll_offset as usize;
        if idx < self.items.len() {
            self.cursor = idx;
            true
        } else {
            false
        }
    }

    /// 确保 cursor 行在可见视口内
    pub fn ensure_visible(&mut self, visible_height: u16) {
        if visible_height == 0 || self.items.is_empty() {
            return;
        }
        let cursor = self.cursor as u16;
        if cursor < self.scroll_offset {
            self.scroll_offset = cursor;
        } else if cursor >= self.scroll_offset + visible_height {
            self.scroll_offset = cursor.saturating_sub(visible_height.saturating_sub(1));
        }
    }

    /// 当前可视范围 [start, end)
    pub fn visible_range(&self, visible_height: u16) -> std::ops::Range<usize> {
        let start = self.scroll_offset as usize;
        let end = (self.scroll_offset as usize + visible_height as usize).min(self.items.len());
        start..end
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: u16) {
        self.scroll_offset = offset;
    }

    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.cursor)
    }

    pub fn selected_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.cursor)
    }

    pub fn clamp_cursor(&mut self) {
        if self.items.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.items.len() - 1);
        }
    }
}

impl<T> Default for PanelList<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "panel_list_test.rs"]
mod tests;
