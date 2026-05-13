use super::events::OAuthCallbackResult;
use super::message_pipeline::PipelineAction;
use super::*;

impl App {
    pub(crate) fn handle_oauth_needed(
        &mut self,
        server_name: String,
        authorization_url: String,
        callback_tx: tokio::sync::oneshot::Sender<OAuthCallbackResult>,
    ) -> (bool, bool, bool) {
        // 关闭 MCP 面板，避免与 OAuth 面板渲染冲突
        self.global_panels.close_if(PanelKind::Mcp);
        self.global_ui.oauth_prompt = Some(OAuthPrompt::new(
            server_name,
            authorization_url,
            callback_tx,
        ));
        (true, true, false)
    }

    pub(crate) fn handle_oauth_completed(&mut self, server_name: String) -> (bool, bool, bool) {
        self.global_ui.oauth_prompt = None;
        // 刷新 MCP 面板的服务器列表以反映新的连接状态
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            panel.servers = self
                .services
                .mcp_pool
                .as_ref()
                .map(|p| p.all_server_infos())
                .unwrap_or_default();
        }
        let vm = MessageViewModel::system(format!("[i] OAuth 授权完成: {}", server_name));
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }

    pub(crate) fn handle_oauth_failed(
        &mut self,
        server_name: String,
        error: String,
    ) -> (bool, bool, bool) {
        self.global_ui.oauth_prompt = None;
        // 刷新 MCP 面板的服务器列表（可能仍是 Failed 状态但信息已更新）
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            panel.servers = self
                .services
                .mcp_pool
                .as_ref()
                .map(|p| p.all_server_infos())
                .unwrap_or_default();
        }
        let vm =
            MessageViewModel::system(format!("[i] OAuth 授权失败: {} - {}", server_name, error));
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }

    pub(crate) fn handle_mcp_action_completed(
        &mut self,
        server_name: String,
        action: String,
        success: bool,
    ) -> (bool, bool, bool) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            panel.servers = self
                .services
                .mcp_pool
                .as_ref()
                .map(|p| p.all_server_infos())
                .unwrap_or_default();
        }
        let msg = match (action.as_str(), success) {
            ("clear_auth", true) => {
                format!("[i] OAuth credentials cleared: {}", server_name)
            }
            ("clear_auth", false) => {
                format!("[i] Failed to clear credentials: {}", server_name)
            }
            (_, true) => format!("[i] Action completed: {}", server_name),
            (_, false) => format!("[i] Action failed: {}", server_name),
        };
        let vm = MessageViewModel::system(msg);
        self.apply_pipeline_action(PipelineAction::AddMessage(vm));
        (true, false, false)
    }
}
