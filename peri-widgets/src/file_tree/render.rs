use crate::file_tree::{FileTreeState, FlatNode};
use ratatui::{
    layout::Rect,
    prelude::*,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Paragraph, StatefulWidget, Widget},
};

/// 文件树渲染 widget
pub struct FileTree {
    cursor_style: Style,
    line_style: Style,
    dir_style: Style,
    file_style: Style,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            cursor_style: Style::default(),
            line_style: Style::default(),
            dir_style: Style::default(),
            file_style: Style::default(),
        }
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn line_style(mut self, style: Style) -> Self {
        self.line_style = style;
        self
    }

    pub fn dir_style(mut self, style: Style) -> Self {
        self.dir_style = style;
        self
    }

    pub fn file_style(mut self, style: Style) -> Self {
        self.file_style = style;
        self
    }
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for FileTree {
    type State = FileTreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
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
            .map(|(i, node)| build_line(node, i == cursor, &self))
            .collect();

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text);
        Widget::render(paragraph, area, buf);
    }
}

fn build_line(node: &FlatNode, is_cursor: bool, tree: &FileTree) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // 缩进竖线: each depth level gets "│ " prefix
    for _ in 0..node.depth {
        spans.push(Span::styled("│ ".to_string(), tree.line_style));
    }

    // 展开/折叠符号
    if node.is_dir {
        let marker = if node.expanded { "▾ " } else { "▸ " };
        spans.push(Span::styled(marker.to_string(), tree.dir_style));
    } else {
        // 文件图标占位（2 字符宽）
        spans.push(Span::styled("  ".to_string(), Style::default()));
    }

    // 名称
    let name = if node.is_dir {
        format!("{}/", node.name)
    } else {
        node.name.clone()
    };
    let name_style = if node.is_dir {
        tree.dir_style
    } else {
        tree.file_style
    };
    spans.push(Span::styled(name, name_style));

    // 选中态：整行覆盖 cursor_style
    if is_cursor {
        for span in &mut spans {
            *span = span.clone().patch_style(tree.cursor_style);
        }
    }

    Line::from(spans)
}

#[path = "render_test.rs"]
#[cfg(test)]
mod tests;
