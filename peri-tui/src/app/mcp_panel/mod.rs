use peri_middlewares::mcp::{ConfigSource, Resource, ServerInfo, Tool};

use super::{panel_list::PanelList, AgentEvent, App};

mod component;
mod ops;

/// 详情视图中的操作菜单项
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailAction {
    /// 查看工具列表
    ViewTools,
    /// 重新进行 OAuth 授权
    ReAuthenticate,
    /// 清除 OAuth 凭证
    ClearAuth,
    /// 重新连接
    Reconnect,
    /// 禁用（已连接的服务器）
    Disable,
    /// 启用（已禁用的服务器）
    Enable,
}

/// MCP 管理面板
#[derive(Clone)]
pub struct McpPanel {
    /// 服务器列表信息
    pub servers: Vec<ServerInfo>,
    /// ServerList 视图的列表状态（cursor + scroll_offset）
    pub(crate) server_list: PanelList<ServerInfo>,
    /// 当前视图层级
    pub view: McpPanelView,
    /// 确认删除弹窗（server name），None 表示非确认状态
    pub confirm_delete: Option<String>,
    /// ServerDetail 视图的光标
    pub(crate) detail_cursor: usize,
    /// ServerDetail 视图的滚动偏移
    pub(crate) detail_scroll_offset: u16,
}

/// 面板视图层级
#[derive(Clone)]
pub enum McpPanelView {
    /// 服务器列表
    ServerList,
    /// 服务器详情（元信息 + 操作菜单）
    ServerDetail {
        server_name: String,
        tools: Vec<Tool>,
        resources: Vec<Resource>,
        /// 可用的操作菜单
        actions: Vec<DetailAction>,
        /// 是否展开显示工具列表
        show_tools: bool,
    },
}

impl McpPanelView {
    pub fn is_server_list(&self) -> bool {
        matches!(self, McpPanelView::ServerList)
    }

    /// 获取详情视图操作列表长度
    pub(crate) fn action_count(&self) -> usize {
        match self {
            McpPanelView::ServerList => 0,
            McpPanelView::ServerDetail { actions, .. } => actions.len(),
        }
    }
}

impl McpPanel {
    pub fn new(mut servers: Vec<ServerInfo>) -> Self {
        servers.sort_by(|a, b| {
            let a_is_project = matches!(a.source, Some(ConfigSource::Project(_)));
            let b_is_project = matches!(b.source, Some(ConfigSource::Project(_)));
            b_is_project
                .cmp(&a_is_project)
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut server_list = PanelList::new();
        server_list.set_items(servers.clone());
        Self {
            servers,
            server_list,
            view: McpPanelView::ServerList,
            confirm_delete: None,
            detail_cursor: 0,
            detail_scroll_offset: 0,
        }
    }

    pub fn cursor(&self) -> usize {
        match &self.view {
            McpPanelView::ServerList => self.server_list.cursor(),
            McpPanelView::ServerDetail { .. } => self.detail_cursor,
        }
    }

    pub fn scroll_offset(&self) -> u16 {
        match &self.view {
            McpPanelView::ServerList => self.server_list.scroll_offset(),
            McpPanelView::ServerDetail { .. } => self.detail_scroll_offset,
        }
    }

    pub fn set_scroll_offset(&mut self, offset: u16) {
        match &mut self.view {
            McpPanelView::ServerList => self.server_list.set_scroll_offset(offset),
            McpPanelView::ServerDetail { .. } => self.detail_scroll_offset = offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peri_middlewares::mcp::ClientStatus;
    include!("mcp_panel_test.rs");
}
