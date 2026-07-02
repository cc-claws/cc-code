use crate::{
    app::{App, MessageViewModel},
    command::Command,
};

pub struct ModelCommand;

impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-model-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let alias = args.trim().to_lowercase();
        // 从当前 provider 动态收集非空 alias，替代硬编码三选一
        let is_valid_alias = if let Some(cfg) = app.services.peri_config.as_ref() {
            let active_provider_id = cfg.config.active_provider_id.as_str();
            cfg.config
                .providers
                .iter()
                .find(|p| p.id == active_provider_id)
                .map(|p| p.models.get_model(&alias).filter(|m| !m.is_empty()).is_some())
                .unwrap_or(false)
        } else {
            false
        };
        if !alias.is_empty() && is_valid_alias {
            if let Some(cfg) = app.services.peri_config.as_mut() {
                cfg.config.active_alias = alias.clone();
                if let Err(e) =
                    App::save_config(cfg, app.services.config_path_override.as_deref())
                {
                    app.session_mgr
                        .current_mut()
                        .messages
                        .view_messages
                        .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                }
                if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                    app.services.provider_name = p.display_name().to_string();
                    app.services.model_name = p.model_name().to_string();
                }
                if let Some(ref acp_client) = app.acp_client {
                    let acp = acp_client.clone();
                    let alias_val = alias.clone();
                    tokio::spawn(async move {
                        let _ = acp.set_config_option("model", &alias_val).await;
                    });
                }
            }
        } else {
            app.open_model_panel();
        }
    }
}
