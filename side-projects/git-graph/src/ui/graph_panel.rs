use crate::app::App;
use crate::graph::render::render_graph_row;
use peri_widgets::Theme as _;
use ratatui::{
    layout::{Margin, Rect},
    style::Style,
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(app.theme.border()));
    f.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));
    app.graph_area = area;
    app.graph_inner_y = inner.y;
    app.viewport_height = inner.height as usize;

    // 为 scrollbar 预留最右 1 列
    let graph_inner = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    let visible_rows = graph_inner.height as usize;
    let start = app.scroll_offset;
    let end = (start + visible_rows).min(app.layout.rows.len());

    for (i, row_idx) in (start..end).enumerate() {
        let row = &app.layout.rows[row_idx];
        let is_selected = row_idx == app.selected_idx;
        let line = render_graph_row(
            row,
            app.graph_width.saturating_sub(2),
            is_selected,
            Some(app.head_oid),
            &app.colors,
            &app.theme,
        );
        f.render_widget(
            Paragraph::new(line),
            Rect::new(
                graph_inner.x,
                graph_inner.y + i as u16,
                graph_inner.width,
                1,
            ),
        );
    }

    // 渲染 scrollbar
    let total = app.layout.rows.len();
    if total > visible_rows {
        let max_scroll = total.saturating_sub(visible_rows);
        let scrollbar_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y,
            1,
            inner.height,
        );
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll).position(app.scroll_offset.min(max_scroll));
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}
