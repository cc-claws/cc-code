use std::collections::HashSet;

pub use peri_middlewares::plugin::InstallScope;
use peri_widgets::InputState;

use super::super::panel_list::PanelList;

/// Discover 视图中展示的可用插件
#[derive(Debug, Clone)]
pub struct DiscoverPlugin {
    pub name: String,
    pub description: String,
    pub marketplace: String,
    pub version: String,
    pub author: Option<String>,
    pub installed: bool,
    pub plugin_id: String,
    pub install_count: Option<u64>,
}

/// Discover 详情页操作菜单
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverDetailAction {
    InstallUser,
    InstallProject,
    BackToList,
}

impl DiscoverDetailAction {
    pub const ALL: [DiscoverDetailAction; 3] = [
        DiscoverDetailAction::InstallUser,
        DiscoverDetailAction::InstallProject,
        DiscoverDetailAction::BackToList,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::InstallUser => "Install (User scope)",
            Self::InstallProject => "Install (Project scope)",
            Self::BackToList => "Back to list",
        }
    }
}

/// Marketplace 条目（Marketplaces 视图用）
#[derive(Debug, Clone)]
pub struct MarketplaceViewEntry {
    pub name: String,
    pub source: peri_middlewares::plugin::MarketplaceSource,
    pub source_label: String,
    pub plugin_count: usize,
    pub installed_count: usize,
    pub status: MarketplaceViewStatus,
    pub last_updated: Option<String>,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketplaceViewStatus {
    Fresh,
    Cached,
    Fetching,
    Stale,
    Failed,
}

/// Marketplace 列表项（用于 PanelList 游标管理）
///
/// 第一个项为虚拟的 "Add Marketplace"，其余项对应实际的 marketplace 条目。
/// 这样 PanelList cursor 范围 (0..=N) 与渲染器期望 (0=Add, 1..=N=entries) 对齐。
#[derive(Debug, Clone)]
pub enum MarketplaceListItem {
    /// 虚拟的 "Add Marketplace" 入口
    AddPlaceholder,
    /// 实际的 marketplace 条目
    Entry(MarketplaceViewEntry),
}

/// 插件条目类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginItemType {
    Plugin,
    Mcp,
}

/// 面板中展示的插件条目
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub plugin_type: PluginItemType,
    pub marketplace: String,
    pub enabled: bool,
    pub scope: InstallScope,
    pub version: String,
    pub install_path: std::path::PathBuf,
    pub project_path: Option<String>,
    pub load_error: Option<String>,
    pub description: String,
    pub author: Option<String>,
    pub commands: Vec<String>,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub mcp_servers: Vec<String>,
}

/// 详情页操作菜单
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailAction {
    ToggleEnabled,
    Uninstall,
    BackToList,
}

impl DetailAction {
    pub const ALL: [DetailAction; 3] = [
        DetailAction::ToggleEnabled,
        DetailAction::Uninstall,
        DetailAction::BackToList,
    ];

    pub fn label(&self, enabled: bool) -> &'static str {
        match self {
            Self::ToggleEnabled => {
                if enabled {
                    "Disable plugin"
                } else {
                    "Enable plugin"
                }
            }
            Self::Uninstall => "Uninstall",
            Self::BackToList => "Back to plugin list",
        }
    }
}

/// 插件面板视图
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginPanelView {
    Installed,
    Discover,
    Marketplaces,
    Errors,
}

impl PluginPanelView {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Discover => "Discover",
            Self::Marketplaces => "Marketplaces",
            Self::Errors => "Errors",
        }
    }

    pub const ALL: [PluginPanelView; 4] = [
        PluginPanelView::Installed,
        PluginPanelView::Discover,
        PluginPanelView::Marketplaces,
        PluginPanelView::Errors,
    ];

    pub fn next(&mut self) {
        *self = match self {
            Self::Installed => Self::Discover,
            Self::Discover => Self::Marketplaces,
            Self::Marketplaces => Self::Errors,
            Self::Errors => Self::Installed,
        };
    }

    pub fn prev(&mut self) {
        *self = match self {
            Self::Installed => Self::Errors,
            Self::Discover => Self::Installed,
            Self::Marketplaces => Self::Discover,
            Self::Errors => Self::Marketplaces,
        };
    }
}

/// /plugin 面板状态
#[derive(Debug, Clone)]
pub struct PluginPanel {
    pub view: PluginPanelView,
    pub entries: Vec<PluginEntry>,
    pub installed_list: PanelList<PluginEntry>,
    pub confirm_delete: Option<String>,
    /// 详情视图：已进入时为 Some(entry_index)
    pub detail_index: Option<usize>,
    /// 详情页操作菜单光标
    pub detail_cursor: usize,

    // --- Discover 视图状态 ---
    pub discover_plugins: Vec<DiscoverPlugin>,
    pub discover_search: InputState,
    pub discover_searching: bool,
    pub discover_list: PanelList<DiscoverPlugin>,
    pub discover_loading: bool,
    pub discover_selected: HashSet<String>,
    pub discover_detail_index: Option<usize>,
    pub discover_detail_cursor: usize,

    // --- Marketplaces 视图状态 ---
    pub marketplace_entries: Vec<MarketplaceViewEntry>,
    pub marketplace_list: PanelList<MarketplaceListItem>,
    pub marketplace_confirm_delete: Option<usize>,
    pub marketplace_updating: HashSet<String>,
    /// 添加 marketplace 输入框
    pub add_marketplace_input: InputState,
    /// 是否处于添加 marketplace 模式
    pub add_marketplace_active: bool,

    // --- 安装/卸载进度 ---
    pub installing: HashSet<String>,
    pub uninstalling: HashSet<String>,
}
