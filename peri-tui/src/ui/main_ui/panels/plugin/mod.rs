use ratatui::{layout::Rect, Frame};

use crate::app::{plugin_panel::PluginPanel, App};

mod plugin_render;

pub fn render_plugin_panel(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    if panel.add_marketplace_active {
        plugin_render::add_marketplace::render_add_marketplace(f, panel, app, area);
        return;
    }
    if panel.discover_detail_index.is_some() {
        plugin_render::discover_detail::render_discover_detail(f, panel, app, area);
    } else if panel.is_detail() {
        plugin_render::detail::render_detail(f, panel, app, area);
    } else if panel.view == crate::app::plugin_panel::PluginPanelView::Discover {
        plugin_render::discover_list::render_discover_list(f, panel, app, area);
    } else {
        plugin_render::list::render_list(f, panel, app, area);
    }
}
