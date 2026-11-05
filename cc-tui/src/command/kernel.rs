use cc_agent::kernel::Kernel;

use crate::{
    app::{AgentEvent, App},
    command::Command,
    ui::message_view::MessageViewModel,
};

/// Switches the active session only after the backend acknowledges it.
pub struct KernelCommand;

const USAGE: &str = "用法：/kernel [cc|dsh|pi] | /kernel show";

impl KernelCommand {
    fn push_notice(app: &mut App, text: String) {
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(MessageViewModel::system(text));
        app.render_rebuild();
    }

    fn query(app: &mut App, show_prompt: bool) {
        let Some(acp) = app.acp_client.clone() else {
            let kernel = Kernel::from_config(
                app.services
                    .peri_config
                    .as_ref()
                    .and_then(|config| config.config.agent_kernel.as_deref()),
            );
            Self::push_notice(
                app,
                format!("默认内核: {}（ACP 未连接）\n{USAGE}", kernel.id()),
            );
            return;
        };
        let session_id = acp.session_id();
        let cwd = app.services.cwd.clone();
        let tx = app.services.bg_event_tx.clone();
        tokio::spawn(async move {
            let message = match acp
                .kernel_status(session_id.as_deref().unwrap_or(""), &cwd)
                .await
            {
                Ok(status) => {
                    let label = if status["scope"] == "session" {
                        "当前会话内核"
                    } else {
                        "默认内核"
                    };
                    let id = status["kernelId"].as_str().unwrap_or("unknown");
                    if show_prompt {
                        format!("{label}: {id}\n\n系统提示词（基础与内核段落；项目指引、Skills 和运行时消息另行注入）：\n\n{}",
                            status["systemPrompt"].as_str().unwrap_or(""))
                    } else {
                        format!("{label}: {id}\n可用内核: cc / dsh / pi\n{USAGE}")
                    }
                }
                Err(error) => format!("读取会话内核失败: {error}"),
            };
            let _ = tx
                .send(AgentEvent::KernelCommandCompleted {
                    session_id,
                    kernel_id: None,
                    message,
                })
                .await;
        });
    }
}

impl Command for KernelCommand {
    fn name(&self) -> &str {
        "kernel"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        "Switch the agent kernel (cc / dsh / pi)".to_string()
    }

    fn execute(&self, app: &mut App, args: &str) {
        let arg = args.trim().to_lowercase();
        match arg.as_str() {
            "cc" | "dsh" | "pi" => {
                let Some(acp) = app.acp_client.clone() else {
                    Self::push_notice(app, "ACP 未连接，无法切换内核".to_string());
                    return;
                };
                let session_id = acp.session_id();
                let tx = app.services.bg_event_tx.clone();
                tokio::spawn(async move {
                    let (kernel_id, message) = match acp.set_config_option_for_session(
                        session_id.as_deref().unwrap_or(""), "agent_kernel", &arg,
                    ).await {
                        Ok(()) => (Some(arg.clone()), format!("已切换到 {arg} 内核。记忆、Skills 和 MCP 继续复用；后续请求使用新内核。")),
                        Err(error) => (None, format!("内核切换失败: {error}")),
                    };
                    let _ = tx
                        .send(AgentEvent::KernelCommandCompleted {
                            session_id,
                            kernel_id,
                            message,
                        })
                        .await;
                });
            }
            "show" => Self::query(app, true),
            "" => Self::query(app, false),
            _ => Self::push_notice(
                app,
                format!("未知内核: {arg}。可用内核: cc / dsh / pi\n{USAGE}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_acp::transport::{
        mpsc::mpsc_transport_pair,
        types::{AcpError, IncomingMessage},
        AcpTransport,
    };

    #[tokio::test]
    async fn switch_waits_for_backend_result_and_surfaces_failure() {
        for success in [false, true] {
            let (mut app, _handle) = App::new_headless(100, 30).await;
            let directory = tempfile::tempdir().unwrap();
            let settings = directory.path().join("settings.json");
            app.services.config_path_override = Some(settings.clone());
            app.services
                .peri_config
                .get_or_insert_with(Default::default)
                .config
                .agent_kernel = Some("cc".to_string());
            let (client, server) = mpsc_transport_pair();
            let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
            app.acp_client = Some(client);
            KernelCommand.execute(&mut app, "pi");
            let request = tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
                .await
                .unwrap()
                .unwrap();
            let IncomingMessage::Request { id, method, params } = request else {
                panic!("expected a config request");
            };
            assert_eq!(method, "session/set_config_option");
            assert_eq!(params["value"], "pi");
            assert_eq!(
                app.services
                    .peri_config
                    .as_ref()
                    .unwrap()
                    .config
                    .agent_kernel
                    .as_deref(),
                Some("cc")
            );
            assert!(
                !settings.exists(),
                "the TUI must not save before the backend acknowledges"
            );
            server
                .send_response(
                    id,
                    if success {
                        Ok(serde_json::json!({}))
                    } else {
                        Err(AcpError::new(-32603, "settings are read-only"))
                    },
                )
                .await
                .unwrap();
            let event = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let event = app
                        .services
                        .bg_event_rx
                        .as_mut()
                        .unwrap()
                        .recv()
                        .await
                        .unwrap();
                    if matches!(event, AgentEvent::KernelCommandCompleted { .. }) {
                        break event;
                    }
                }
            })
            .await
            .unwrap();
            if let AgentEvent::KernelCommandCompleted { message, .. } = &event {
                assert_eq!(message.contains("已切换"), success);
                if !success {
                    assert!(message.contains("settings are read-only"));
                }
            }
            let messages = app.session_mgr.current().messages.view_messages.len();
            app.handle_agent_event(event);
            assert_eq!(
                app.session_mgr.current().messages.view_messages.len(),
                messages + 1
            );
            assert_eq!(
                app.services
                    .peri_config
                    .as_ref()
                    .unwrap()
                    .config
                    .agent_kernel
                    .as_deref(),
                Some(if success { "pi" } else { "cc" })
            );
        }
    }
}
