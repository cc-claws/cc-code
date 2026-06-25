use crate::scrollable::ScrollState;
use ratatui::{
    layout::Rect,
    prelude::*,
    style::Style,
    text::{Line, Text},
    widgets::{Paragraph, StatefulWidget, StatefulWidgetRef, Widget},
};

/// 泛型列表状态——管理 items + cursor + scroll offset
///
/// T 不要求 Clone。cursor 使用 clamp 模式（不循环）。
/// 内嵌 ScrollState，滚动与光标联动通过 ensure_visible 自动处理。
pub struct ListState<T> {
    items: Vec<T>,
    cursor: usize,
    pub scroll: ScrollState,
    mouse_pos: Option<(u16, u16)>,
    on_select: Option<Box<dyn Fn(usize)>>,
}

impl<T> ListState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            cursor: 0,
            scroll: ScrollState::new(),
            mouse_pos: None,
            on_select: None,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// 移动光标（clamp 模式，不循环）
    pub fn move_cursor(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() - 1;
        let new = self.cursor as i32 + delta;
        self.cursor = new.clamp(0, max as i32) as usize;
    }

    /// 确保 cursor 不超过 items.len()（外部修改 items 后调用）
    pub fn clamp_cursor(&mut self) {
        if self.items.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(self.items.len() - 1);
        }
    }

    /// 获取当前 cursor 指向的 item 引用
    pub fn selected(&self) -> Option<&T> {
        self.items.get(self.cursor)
    }

    /// 获取当前 cursor 指向的 item 可变引用
    pub fn selected_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.cursor)
    }

    /// 确保 cursor 行在可见视口内（联动 ScrollState）
    pub fn ensure_visible(&mut self, visible: u16) {
        self.scroll.ensure_visible(self.cursor as u16, visible);
    }

    /// 替换 items 列表，自动 clamp cursor
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.clamp_cursor();
    }

    /// 更新鼠标位置（渲染前由 TUI 层调用，传入相对坐标）
    pub fn update_mouse(&mut self, pos: Option<(u16, u16)>) {
        self.mouse_pos = pos;
    }

    /// 根据鼠标位置计算悬停的 item 索引（考虑 scroll offset）
    pub fn hovered(&self) -> Option<usize> {
        let (row, _) = self.mouse_pos?;
        let idx = row as usize + self.scroll.offset() as usize;
        if idx < self.items.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// 设置鼠标位置对应的 item 为 cursor（点击选择）
    pub fn set_cursor_by_mouse(&mut self, row: u16) {
        let idx = row as usize + self.scroll.offset() as usize;
        if idx < self.items.len() {
            self.cursor = idx;
        }
    }

    /// 触发选中回调
    pub fn select(&self) {
        if let Some(ref cb) = self.on_select {
            cb(self.cursor);
        }
    }

    /// 设置选中回调（builder 模式）
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }
}

/// 可选择列表 widget——通过闭包自定义每项渲染
///
/// 实现 ratatui StatefulWidget trait，状态类型为 ListState<T>。
/// render_item 闭包签名为 `Fn(&T, bool, bool) -> Line<'a>`，
/// 第一个 bool 表示当前行是否为 cursor，第二个 bool 表示是否为鼠标悬停。
/// "特殊首项"模式由调用方在闭包中处理（如 items[0] 是 "New Thread"）。
pub struct SelectableList<'a, T> {
    #[allow(clippy::type_complexity)]
    render_item: Box<dyn Fn(&T, bool, bool) -> Line<'a>>,
    cursor_marker: &'a str,
    cursor_style: Style,
    hover_style: Style,
    normal_style: Style,
}

impl<'a, T> SelectableList<'a, T> {
    pub fn new(render_item: impl Fn(&T, bool, bool) -> Line<'a> + 'static) -> Self {
        Self {
            render_item: Box::new(render_item),
            cursor_marker: "▶ ",
            cursor_style: Style::default(),
            hover_style: Style::default(),
            normal_style: Style::default(),
        }
    }

    pub fn cursor_marker(mut self, marker: &'a str) -> Self {
        self.cursor_marker = marker;
        self
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn hover_style(mut self, style: Style) -> Self {
        self.hover_style = style;
        self
    }

    pub fn normal_style(mut self, style: Style) -> Self {
        self.normal_style = style;
        self
    }
}

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

            // 三态：cursor 优先 > hover > normal
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
        Widget::render(paragraph, area, buf);
    }
}

impl<T> StatefulWidget for SelectableList<'_, T> {
    type State = ListState<T>;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        self.render_ref(area, buf, state);
    }
}

#[cfg(test)]
#[path = "list_test.rs"]
mod tests;
