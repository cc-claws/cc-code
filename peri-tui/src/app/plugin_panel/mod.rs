use std::{any::Any, collections::HashSet};

use peri_widgets::InputState;
use ratatui::{layout::Rect, Frame};
use tui_textarea::Input;

use super::{
    panel_component::PanelComponent,
    panel_list::PanelList,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

pub mod handlers;
pub mod types;

pub use types::*;

impl PluginPanel {
    pub fn new(entries: Vec<PluginEntry>) -> Self {
        let mut installed_list = PanelList::new();
        installed_list.set_items(entries.clone());
        Self {
            view: PluginPanelView::Installed,
            entries,
            installed_list,
            confirm_delete: None,
            detail_index: None,
            detail_cursor: 0,
            discover_plugins: Vec::new(),
            discover_search: InputState::new(),
            discover_searching: false,
            discover_list: PanelList::new(),
            discover_loading: false,
            discover_selected: HashSet::new(),
            discover_detail_index: None,
            discover_detail_cursor: 0,
            marketplace_entries: Vec::new(),
            marketplace_list: PanelList::new(),
            marketplace_confirm_delete: None,
            marketplace_updating: HashSet::new(),
            add_marketplace_input: InputState::new(),
            add_marketplace_active: false,
            installing: HashSet::new(),
            uninstalling: HashSet::new(),
        }
    }

    pub fn is_detail(&self) -> bool {
        self.detail_index.is_some()
            || self.discover_detail_index.is_some()
            || self.add_marketplace_active
    }

    /// 按搜索词过滤后的 Discover 插件列表
    pub fn discover_filtered_plugins(&self) -> Vec<&DiscoverPlugin> {
        let search = self.discover_search.value();
        if search.is_empty() {
            self.discover_plugins.iter().collect()
        } else {
            let query = search.to_lowercase();
            self.discover_plugins
                .iter()
                .filter(|p| {
                    p.name.to_lowercase().contains(&query)
                        || p.description.to_lowercase().contains(&query)
                        || p.marketplace.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    /// 获取当前光标处的 Discover 插件
    pub fn discover_current_plugin(&self) -> Option<&DiscoverPlugin> {
        let filtered = self.discover_filtered_plugins();
        filtered.get(self.discover_list.cursor()).copied()
    }

    /// 根据当前视图过滤后的可见条目索引列表
    pub fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| match self.view {
                PluginPanelView::Installed => true,
                PluginPanelView::Errors => e.load_error.is_some(),
                PluginPanelView::Discover | PluginPanelView::Marketplaces => false,
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn current_list_len(&self) -> usize {
        match self.view {
            PluginPanelView::Installed => self.installed_list.len(),
            PluginPanelView::Errors => {
                // Errors 视图过滤有 load_error 的条目
                self.entries
                    .iter()
                    .filter(|e| e.load_error.is_some())
                    .count()
            }
            PluginPanelView::Discover => self.discover_list.len(),
            PluginPanelView::Marketplaces => {
                // marketplace_cursor = 0 是 Add Marketplace，+ marketplace_entries.len()
                self.marketplace_entries.len() + 1
            }
        }
    }

    /// 根据当前视图返回 cursor
    pub fn cursor(&self) -> usize {
        match self.view {
            PluginPanelView::Installed => self.installed_list.cursor(),
            PluginPanelView::Discover => self.discover_list.cursor(),
            PluginPanelView::Marketplaces => self.marketplace_list.cursor(),
            PluginPanelView::Errors => self.installed_list.cursor(),
        }
    }

    /// 根据当前视图返回 scroll_offset
    pub fn scroll_offset(&self) -> u16 {
        match self.view {
            PluginPanelView::Installed => self.installed_list.scroll_offset(),
            PluginPanelView::Discover => self.discover_list.scroll_offset(),
            PluginPanelView::Marketplaces => self.marketplace_list.scroll_offset(),
            PluginPanelView::Errors => self.installed_list.scroll_offset(),
        }
    }

    /// 根据当前视图设置 scroll_offset
    pub fn set_scroll_offset(&mut self, offset: u16) {
        match self.view {
            PluginPanelView::Installed => self.installed_list.set_scroll_offset(offset),
            PluginPanelView::Discover => self.discover_list.set_scroll_offset(offset),
            PluginPanelView::Marketplaces => self.marketplace_list.set_scroll_offset(offset),
            PluginPanelView::Errors => self.installed_list.set_scroll_offset(offset),
        }
    }

    pub fn selected_entry(&self) -> Option<&PluginEntry> {
        let indices = self.visible_indices();
        indices
            .get(self.cursor())
            .and_then(|&i| self.entries.get(i))
    }

    /// 切换视图后同步当前视图的 PanelList items
    fn sync_current_view_items(&mut self) {
        match self.view {
            PluginPanelView::Installed => {
                // installed_list items 已在 new() 时设置，无需同步
            }
            PluginPanelView::Errors => {
                // Errors 视图：只显示有 load_error 的 entries
                let error_entries: Vec<PluginEntry> = self
                    .entries
                    .iter()
                    .filter(|e| e.load_error.is_some())
                    .cloned()
                    .collect();
                self.installed_list.set_items(error_entries);
            }
            PluginPanelView::Discover => {
                self.discover_list.set_items(
                    self.discover_filtered_plugins()
                        .into_iter()
                        .cloned()
                        .collect(),
                );
            }
            PluginPanelView::Marketplaces => {
                self.sync_marketplace_list_items();
            }
        }
    }

    /// 将 marketplace_entries 同步到 marketplace_list（含虚拟 Add 占位项）
    pub(crate) fn sync_marketplace_list_items(&mut self) {
        use types::MarketplaceListItem;
        let mut items: Vec<MarketplaceListItem> = vec![MarketplaceListItem::AddPlaceholder];
        items.extend(
            self.marketplace_entries
                .iter()
                .cloned()
                .map(MarketplaceListItem::Entry),
        );
        self.marketplace_list.set_items(items);
    }
}

// ─── PanelComponent 实现 ──────────────────────────────────────────────────────

impl PanelComponent for PluginPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Plugin
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        // 1. confirm_delete 模式
        if self.confirm_delete.is_some() {
            return self.handle_confirm_delete(input, ctx);
        }

        // 2. discover_searching 模式
        if self.discover_searching {
            return self.handle_discover_searching(input, ctx);
        }

        // 3. discover_detail_index 模式
        if self.discover_detail_index.is_some() {
            return self.handle_discover_detail(input, ctx);
        }

        // 4. detail_index 模式
        if self.detail_index.is_some() {
            return self.handle_installed_detail(input, ctx);
        }

        // 5. 列表视图（按 PluginPanelView 分发）
        match self.view {
            PluginPanelView::Discover => self.handle_discover_list(input, ctx),
            PluginPanelView::Marketplaces => self.handle_marketplaces_list(input, ctx),
            PluginPanelView::Installed | PluginPanelView::Errors => {
                self.handle_installed_list(input, ctx)
            }
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        if self.add_marketplace_active {
            for ch in text.chars() {
                self.add_marketplace_input.insert(ch);
            }
            return EventResult::Consumed;
        }
        if self.discover_searching {
            for ch in text.chars() {
                self.discover_search.insert(ch);
            }
            self.discover_list.set_items(
                self.discover_filtered_plugins()
                    .into_iter()
                    .cloned()
                    .collect(),
            );
            return EventResult::Consumed;
        }
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match self.view {
            PluginPanelView::Installed => self.installed_list.handle_scroll(lines, 10),
            PluginPanelView::Discover => self.discover_list.handle_scroll(lines, 10),
            PluginPanelView::Marketplaces => self.marketplace_list.handle_scroll(lines, 10),
            PluginPanelView::Errors => self.installed_list.handle_scroll(lines, 10),
        }
        EventResult::Consumed
    }

    fn set_scroll_offset(&mut self, offset: u16) {
        self.set_scroll_offset(offset);
    }

    fn desired_height(&self, screen_height: u16, _screen_width: u16) -> u16 {
        screen_height * 70 / 100
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::plugin::render_plugin_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        if self.confirm_delete.is_some() {
            return vec![
                ("Enter".to_string(), _lc.tr("hint-plugin-uninstall")),
                ("\u{5176}\u{4ed6}\u{952e}".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.marketplace_confirm_delete.is_some() {
            return vec![
                ("Enter".to_string(), _lc.tr("hint-plugin-delete")),
                ("Esc".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.add_marketplace_active {
            return vec![
                ("Enter".to_string(), _lc.tr("hint-plugin-add")),
                ("Esc".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.discover_searching {
            return vec![
                (
                    "Esc/\u{2191}\u{2193}".to_string(),
                    _lc.tr("hint-plugin-exit-search"),
                ),
                ("\u{2190}\u{2192}".to_string(), _lc.tr("key-tab")),
                ("Enter".to_string(), _lc.tr("key-install")),
                ("Backspace".to_string(), _lc.tr("key-delete")),
            ];
        }
        if self.discover_detail_index.is_some() {
            return vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
                ("Enter".to_string(), _lc.tr("key-execute")),
                ("Esc".to_string(), _lc.tr("key-back")),
            ];
        }
        if self.detail_index.is_some() {
            return vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
                ("Enter".to_string(), _lc.tr("key-execute")),
                ("Esc".to_string(), _lc.tr("key-back")),
            ];
        }
        match self.view {
            PluginPanelView::Discover => vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-select")),
                ("\u{8f93}\u{5165}".to_string(), _lc.tr("hint-plugin-search")),
                ("Enter".to_string(), _lc.tr("key-install")),
                ("\u{2190}\u{2192}/Tab".to_string(), _lc.tr("key-tab")),
                ("Esc".to_string(), _lc.tr("key-close")),
            ],
            PluginPanelView::Marketplaces => vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-select")),
                ("Enter".to_string(), _lc.tr("hint-plugin-add")),
                ("Backspace".to_string(), _lc.tr("hint-plugin-remove")),
                ("\u{2190}\u{2192}/Tab".to_string(), _lc.tr("key-tab")),
                ("Esc".to_string(), _lc.tr("key-close")),
            ],
            PluginPanelView::Installed | PluginPanelView::Errors => vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
                ("Space".to_string(), _lc.tr("key-switch")),
                ("Enter".to_string(), _lc.tr("key-detail")),
                ("\u{2190}\u{2192}/Tab".to_string(), _lc.tr("key-tab")),
                ("Esc".to_string(), _lc.tr("key-close")),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("plugin_panel_test.rs");
}
