use crate::app::{App, Overlay};
use crate::ui::overlay;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    app.frame_area = size;

    // 全局工具栏占顶部 1 行
    let (toolbar_area, body_area) = if size.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(size);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, size)
    };

    // 搜索栏占底部 1 行
    let (content_area, search_area) = if app.overlay == Overlay::SearchBar && body_area.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(body_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (body_area, None)
    };

    // 左右分栏：sidebar(25%) + 右侧(75%)
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(content_area);
    app.sidebar_layout = crate::ui::sidebar::draw(f, h_chunks[0], app);

    // 右侧上下分栏：graph(65%) + detail(35%)
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(h_chunks[1]);
    app.detail_area = v_chunks[1];
    crate::ui::graph_panel::draw(f, v_chunks[0], app);
    crate::ui::detail_panel::draw(f, v_chunks[1], app);

    // 全局工具栏
    if let Some(tb_area) = toolbar_area {
        crate::ui::toolbar::draw_global_toolbar(f, tb_area, app);
    }

    // 搜索栏
    if let Some(sb_area) = search_area {
        crate::ui::search_bar::draw_search_bar(f, sb_area, app);
    }

    // 确认弹窗
    crate::ui::confirm::draw_confirm(f, size, app);
    // 列表 overlay（覆盖在确认弹窗之上）
    overlay::draw_overlay(f, size, app);
}
