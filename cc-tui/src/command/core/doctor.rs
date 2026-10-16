use crate::{app::App, command::Command, ui::message_view::MessageViewModel};

pub struct DoctorCommand;

impl Command for DoctorCommand {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-doctor-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 借用作用域限制在本块内：之后还要可变借用 app 写入消息
        let lines: Vec<String> = {
            let lc = &app.services.lc;
            let field =
                |key: &str, value: String| lc.tr_args(key, &[("detail".into(), value.into())]);
            let mut out = vec![lc.tr("doctor-header"), String::new()];

            // 1. Settings 文件（#289：经 app_home 解析，新版 ~/.cc-code 优先）
            let settings_path = Some(cc_agent::app_home::global_settings_path());
            let settings_status = match &settings_path {
                Some(p) if p.is_file() => field("doctor-ok", p.display().to_string()),
                Some(p) => field("doctor-missing-path", p.display().to_string()),
                None => lc.tr("doctor-settings-missing"),
            };
            out.push(lc.tr("doctor-table-header"));
            out.push("|--------|------|------|".to_string());
            out.push(format!("| Settings | {} |", settings_status));

            // 2. API Key
            let has_anthropic = std::env::var("ANTHROPIC_API_KEY")
                .map(|k| !k.is_empty())
                .unwrap_or(false);
            let has_openai = std::env::var("OPENAI_API_KEY")
                .map(|k| !k.is_empty())
                .unwrap_or(false);
            let api_status = if has_anthropic || has_openai {
                let keys: Vec<&str> = [
                    has_anthropic.then_some("ANTHROPIC_API_KEY"),
                    has_openai.then_some("OPENAI_API_KEY"),
                ]
                .into_iter()
                .flatten()
                .collect();
                field("doctor-ok", keys.join(" + "))
            } else {
                lc.tr("doctor-no-api-key")
            };
            out.push(format!("| API Key | {} |", api_status));

            // 3. Provider 配置
            let provider_status = match &app.services.peri_config {
                Some(cfg) if !cfg.config.providers.is_empty() => {
                    let active = &cfg.config.active_provider_id;
                    let provider = cfg.config.providers.iter().find(|p| p.id == *active);
                    match provider {
                        Some(p) => field(
                            "doctor-ok",
                            format!(
                                "{} ({})",
                                p.display_name(),
                                p.models
                                    .get_model(&cfg.config.active_alias)
                                    .unwrap_or("default")
                            ),
                        ),
                        None => lc.tr_args(
                            "doctor-provider-not-found",
                            &[("id".into(), active.clone().into())],
                        ),
                    }
                }
                _ => lc.tr("doctor-no-provider"),
            };
            out.push(format!("| Provider | {} |", provider_status));

            // 4. MCP 配置
            let mcp_status = if app.services.mcp_pool.is_some() {
                lc.tr("doctor-mcp-ok")
            } else {
                let mcp_project = std::path::Path::new(&app.services.cwd).join(".mcp.json");
                if mcp_project.is_file() {
                    lc.tr("doctor-mcp-not-init")
                } else {
                    lc.tr("doctor-no-mcp")
                }
            };
            out.push(format!("| MCP | {} |", mcp_status));

            // 5. Model Alias
            let alias_status = match &app.services.peri_config {
                Some(cfg) => {
                    let p = cfg
                        .config
                        .providers
                        .iter()
                        .find(|p| p.id == cfg.config.active_provider_id);
                    match p {
                        Some(p) => {
                            let aliases: Vec<String> =
                                cc_acp::provider::ProviderModels::ALL_ALIASES
                                    .iter()
                                    .filter(|a| !p.models.get_model(a).unwrap_or("").is_empty())
                                    .map(|a| a.to_string())
                                    .collect();
                            if aliases.is_empty() {
                                lc.tr("doctor-no-alias")
                            } else {
                                field("doctor-ok", aliases.join("/"))
                            }
                        }
                        None => lc.tr("doctor-no-alias-no-provider"),
                    }
                }
                _ => lc.tr("doctor-no-alias-not-configured"),
            };
            out.push(format!("| Model Alias | {} |", alias_status));

            out
        };

        let vm = MessageViewModel::system(lines.join("\n"));
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    async fn headless_app() -> App {
        App::new_headless(80, 24).await.0
    }

    #[tokio::test]
    async fn test_doctor_no_config() {
        let mut app = headless_app().await;
        let cmd = DoctorCommand;
        cmd.execute(&mut app, "");
        let msgs = &app.session_mgr.current_mut().messages.view_messages;
        assert_eq!(msgs.len(), 1);
        let text = format!("{:?}", msgs[0]);
        assert!(
            text.contains("No Provider") || text.contains("Missing"),
            "无配置时应显示 No Provider 或 Missing: {}",
            text
        );
    }
}
