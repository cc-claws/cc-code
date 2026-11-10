use crate::{
    app::{panel_manager::PanelContext, AgentEvent, App},
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
        if app.session_mgr.current().metadata.kernel_request_pending {
            let notice = app.services.lc.tr("app-kernel-pending");
            app.session_mgr
                .current_mut()
                .ui
                .set_toast(notice, std::time::Duration::from_millis(2500));
            app.render_rebuild();
            return;
        }
        let Some(acp) = app.acp_client.clone() else {
            let message = format!(
                "当前内核: {}（ACP 未连接）\n{USAGE}",
                app.current_kernel_id()
            );
            Self::push_notice(app, message);
            return;
        };
        app.session_mgr
            .current_mut()
            .metadata
            .kernel_request_pending = true;
        let session_id = app
            .session_mgr
            .current()
            .current_thread_id
            .clone()
            .or_else(|| acp.session_id());
        let cwd = app.services.cwd.clone();
        let tx = app.services.bg_event_tx.clone();
        tokio::spawn(async move {
            let status = async {
                if let Some(target) = session_id.as_deref() {
                    if acp.session_id().as_deref() != Some(target) {
                        acp.load_session(target, &cwd, None).await?;
                    }
                }
                acp.kernel_status(session_id.as_deref().unwrap_or(""), &cwd)
                    .await
            }
            .await;
            let message = match status {
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

    /// Shared by the explicit command and the picker; state updates happen in
    /// KernelCommandCompleted only after the backend acknowledges the change.
    pub(crate) fn switch(ctx: &mut PanelContext<'_>, id: &str) {
        if ctx.session_mgr.current().metadata.kernel_request_pending {
            let notice = ctx.services.lc.tr("app-kernel-pending");
            ctx.session_mgr
                .current_mut()
                .ui
                .set_toast(notice, std::time::Duration::from_millis(2500));
            return;
        }
        let Some(acp) = ctx.acp_client.clone() else {
            ctx.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    "ACP 未连接，无法切换内核".to_string(),
                ));
            return;
        };
        ctx.session_mgr
            .current_mut()
            .metadata
            .kernel_request_pending = true;
        let session_id = ctx
            .session_mgr
            .current()
            .current_thread_id
            .clone()
            .or_else(|| acp.session_id());
        let cwd = ctx.services.cwd.clone();
        let tx = ctx.services.bg_event_tx.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            let result = async {
                // A restored thread may not have been loaded into ACP yet.
                // Load it before setting a session option so an empty ID never
                // accidentally changes the default kernel instead.
                if let Some(target) = session_id.as_deref() {
                    if acp.session_id().as_deref() != Some(target) {
                        acp.load_session(target, &cwd, None).await?;
                    }
                }
                acp.set_config_option_for_session(
                    session_id.as_deref().unwrap_or(""),
                    "agent_kernel",
                    &id,
                )
                .await
            }
            .await;
            let (kernel_id, message) = match result {
                Ok(()) => (Some(id.clone()), format!("内核已切换为: {id}")),
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
                Self::switch(
                    &mut PanelContext {
                        services: &mut app.services,
                        session_mgr: &mut app.session_mgr,
                        acp_client: app.acp_client.clone(),
                    },
                    &arg,
                );
                app.render_rebuild();
            }
            "show" => Self::query(app, true),
            "" => app.open_kernel_panel(),
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
                messages + usize::from(!success)
            );
            if success {
                let expected = app
                    .services
                    .lc
                    .tr_args("app-kernel-switched", &[("kernel".into(), "pi".into())]);
                assert_eq!(
                    app.session_mgr
                        .current()
                        .ui
                        .toast
                        .as_ref()
                        .map(|toast| toast.0.as_str()),
                    Some(expected.as_str())
                );
            }
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

    #[tokio::test]
    async fn restored_thread_loads_before_switch_and_load_failure_never_sets_default() {
        for load_succeeds in [false, true] {
            let (mut app, _handle) = App::new_headless(100, 30).await;
            app.session_mgr.current_mut().current_thread_id = Some("restored-thread".to_string());
            app.session_mgr.current_mut().metadata.kernel_id = Some("pi".to_string());
            let (client, server) = mpsc_transport_pair();
            let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
            app.acp_client = Some(client);

            KernelCommand.execute(&mut app, "dsh");
            let request = tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
                .await
                .unwrap()
                .unwrap();
            let IncomingMessage::Request { id, method, params } = request else {
                panic!("expected a load request");
            };
            assert_eq!(method, "session/load");
            assert_eq!(params["sessionId"], "restored-thread");
            assert_eq!(app.current_kernel_id(), "pi");
            server
                .send_response(
                    id,
                    if load_succeeds {
                        Ok(serde_json::json!({}))
                    } else {
                        Err(AcpError::new(-32603, "thread cannot be loaded"))
                    },
                )
                .await
                .unwrap();

            if load_succeeds {
                let request =
                    tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
                        .await
                        .unwrap()
                        .unwrap();
                let IncomingMessage::Request { id, method, params } = request else {
                    panic!("expected a config request after loading");
                };
                assert_eq!(method, "session/set_config_option");
                assert_eq!(params["sessionId"], "restored-thread");
                assert_eq!(params["value"], "dsh");
                server
                    .send_response(id, Ok(serde_json::json!({})))
                    .await
                    .unwrap();
            }
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
            if let AgentEvent::KernelCommandCompleted {
                session_id,
                kernel_id,
                message,
            } = &event
            {
                assert_eq!(session_id.as_deref(), Some("restored-thread"));
                assert_eq!(kernel_id.as_deref(), load_succeeds.then_some("dsh"));
                if !load_succeeds {
                    assert!(message.contains("thread cannot be loaded"));
                    assert!(
                        tokio::time::timeout(std::time::Duration::from_millis(50), server.recv())
                            .await
                            .is_err(),
                        "a failed session load must not issue any config request"
                    );
                }
            }
            app.handle_agent_event(event);
            assert_eq!(
                app.current_kernel_id(),
                if load_succeeds { "dsh" } else { "pi" }
            );
        }
    }

    #[tokio::test]
    async fn show_keeps_querying_full_prompt_without_opening_picker() {
        let (mut app, _handle) = App::new_headless(100, 30).await;
        let (client, server) = mpsc_transport_pair();
        let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
        app.acp_client = Some(client);
        KernelCommand.execute(&mut app, "show");
        assert!(!app.session_mgr.current().session_panels.is_any_open());
        let request = tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
            .await
            .unwrap()
            .unwrap();
        let IncomingMessage::Request { id, method, .. } = request else {
            panic!("expected a kernel status request");
        };
        assert_eq!(method, "session/kernel");
        server
            .send_response(
                id,
                Ok(serde_json::json!({
                    "scope": "session", "kernelId": "pi", "systemPrompt": "test kernel prompt"
                })),
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
        let AgentEvent::KernelCommandCompleted {
            kernel_id, message, ..
        } = event
        else {
            unreachable!();
        };
        assert_eq!(kernel_id, None);
        assert!(message.contains("当前会话内核: pi"));
        assert!(message.contains("test kernel prompt"));
    }

    #[tokio::test]
    async fn pending_request_blocks_duplicates_and_thread_switch_until_success_or_failure() {
        for command in ["pi", "show"] {
            for success in [false, true] {
                let (mut app, _handle) = App::new_headless(100, 30).await;
                let (client, server) = mpsc_transport_pair();
                let (client, _notifications) = crate::acp_client::AcpTuiClient::new(client);
                app.acp_client = Some(client);

                KernelCommand.execute(&mut app, command);
                assert!(app.session_mgr.current().metadata.kernel_request_pending);
                assert!(!app.can_switch_thread_context());
                assert!(!app.open_thread("another-thread".to_string()));
                assert!(app.session_mgr.current().current_thread_id.is_none());
                let request =
                    tokio::time::timeout(std::time::Duration::from_secs(5), server.recv())
                        .await
                        .unwrap()
                        .unwrap();
                let IncomingMessage::Request { id, method, .. } = request else {
                    panic!("expected a kernel request");
                };
                assert_eq!(
                    method,
                    if command == "show" {
                        "session/kernel"
                    } else {
                        "session/set_config_option"
                    }
                );

                KernelCommand.execute(&mut app, "dsh");
                KernelCommand.execute(&mut app, "show");
                let pending_notice = app.services.lc.tr("app-kernel-pending");
                assert_eq!(
                    app.session_mgr
                        .current()
                        .ui
                        .toast
                        .as_ref()
                        .map(|toast| toast.0.as_str()),
                    Some(pending_notice.as_str())
                );
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(50), server.recv())
                        .await
                        .is_err(),
                    "the pending request must prevent concurrent query and switch requests"
                );
                server
                    .send_response(
                        id,
                        if success {
                            Ok(serde_json::json!({
                                "scope": "session", "kernelId": "pi", "systemPrompt": "test prompt"
                            }))
                        } else {
                            Err(AcpError::new(-32603, "backend rejected request"))
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
                assert!(
                    app.session_mgr.current().metadata.kernel_request_pending,
                    "the request stays pending until its completion is handled"
                );
                app.handle_agent_event(event);
                assert!(!app.session_mgr.current().metadata.kernel_request_pending);
                assert!(
                    app.can_switch_thread_context(),
                    "both success and failure must release the gate"
                );
            }
        }
    }
}
