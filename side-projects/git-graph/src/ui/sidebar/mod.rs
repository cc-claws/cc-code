pub mod file_tree_panel;
pub mod status_panel;

use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

/// 记录 sidebar 布局（两个面板的位置信息，用于点击检测）
#[derive(Default)]
pub struct SidebarLayout {
    /// Staged 面板内容区信息
    pub staged_inner: Rect,
    pub staged_layout: Option<status_panel::PanelLayout>,
    /// Changes 面板内容区信息
    pub changes_inner: Rect,
    pub changes_layout: Option<status_panel::PanelLayout>,
}

/// 渲染左侧 sidebar：上 Staged + 下 Changes，各带边框
pub fn draw(f: &mut Frame, area: Rect, app: &mut App) -> SidebarLayout {
    app.sidebar_area = area;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    app.sidebar_split_y = chunks[0].height + chunks[0].y;

    let (staged_inner, staged_layout) = status_panel::draw_staged(f, chunks[0], app);
    let (changes_inner, changes_layout) = status_panel::draw_changes(f, chunks[1], app);

    SidebarLayout {
        staged_inner,
        staged_layout,
        changes_inner,
        changes_layout,
    }
}
