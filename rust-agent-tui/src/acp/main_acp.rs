use std::sync::Arc;

use agent_client_protocol::{on_receive_dispatch, on_receive_request, Agent};
use agent_client_protocol_tokio::Stdio;
use anyhow::Result;
use rust_agent_middlewares::prelude::{PermissionMode, SharedPermissionMode};
use rust_create_agent::thread::{SqliteThreadStore, ThreadStore};

use super::dispatch;
use super::request_handler;
use super::session::SessionManager;
use crate::app::agent::LlmProvider;
use crate::config;

pub async fn run_acp_mode(_cwd: String, model_override: Option<String>) -> Result<()> {
    let _telemetry = rust_create_agent::telemetry::init_tracing("peri-acp");

    let zen_config = Arc::new(config::load().unwrap_or_default());
    let provider = resolve_provider(&zen_config, model_override.as_deref());

    let thread_store: Arc<dyn ThreadStore> =
        Arc::new(SqliteThreadStore::default_path().unwrap_or_else(|_| {
            SqliteThreadStore::new(std::env::temp_dir().join("peri-acp-threads.db"))
                .expect("无法创建临时数据库")
        }));

    let permission_mode = SharedPermissionMode::new(PermissionMode::AutoMode);

    let session_mgr = SessionManager::new(thread_store, provider, zen_config, permission_mode);

    dispatch::init_session_manager(session_mgr);

    Agent::builder(Agent)
        .name("perihelion")
        .on_receive_request(request_handler::handle_initialize, on_receive_request!())
        .on_receive_request(dispatch::handle_new_session, on_receive_request!())
        .on_receive_request(dispatch::handle_close_session, on_receive_request!())
        .on_receive_request(dispatch::handle_list_sessions, on_receive_request!())
        .on_receive_request(dispatch::handle_prompt, on_receive_request!())
        .on_receive_request(dispatch::handle_load_session, on_receive_request!())
        .on_receive_request(dispatch::handle_resume_session, on_receive_request!())
        .on_receive_request(dispatch::handle_set_mode, on_receive_request!())
        .on_receive_request(dispatch::handle_set_config_option, on_receive_request!())
        .on_receive_request(dispatch::handle_set_model, on_receive_request!())
        .on_receive_request(dispatch::handle_fork_session, on_receive_request!())
        .on_receive_dispatch(dispatch::handle_dispatch, on_receive_dispatch!())
        .connect_to(Stdio::new())
        .await
        .map_err(Into::into)
}

fn resolve_provider(zen_config: &config::ZenConfig, model_override: Option<&str>) -> LlmProvider {
    if let Some(model) = model_override {
        LlmProvider::from_config_for_alias(zen_config, model)
            .or_else(LlmProvider::from_env)
            .unwrap_or_else(|| {
                LlmProvider::from_config(zen_config)
                    .or_else(LlmProvider::from_env)
                    .expect("未配置任何 LLM Provider，请运行 peri tui 完成初始设置")
            })
    } else {
        LlmProvider::from_config(zen_config)
            .or_else(LlmProvider::from_env)
            .expect("未配置任何 LLM Provider，请运行 peri tui 完成初始设置")
    }
}
