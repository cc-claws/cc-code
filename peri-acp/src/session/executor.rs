//! Shared prompt execution logic.
//!
//! Provides [`execute_prompt`] which encapsulates the common agent execution
//! pipeline used by both TUI (via [`TransportEventSink`]) and stdio (via
//! [`StdioEventSink`]) paths.
//!
//! After agent execution completes, the executor checks token thresholds and
//! automatically triggers compact (micro or full) + resubmit as needed.

use std::sync::Arc;

use peri_agent::agent::events::{AgentEvent as ExecutorEvent, AgentEventHandler};
use peri_agent::agent::state::AgentState;
use peri_agent::agent::token::ContextBudget;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::agent::State;
use peri_agent::interaction::UserInteractionBroker;
use peri_agent::messages::BaseMessage;
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use crate::agent::builder::{self, AcpAgentConfig};
use crate::prompt::{build_system_prompt, PromptFeatures};
use crate::provider::LlmProvider;
use crate::session::compact_runner::{self, HookContext};
use crate::session::event_sink::EventSink;

/// Result of prompt execution.
pub struct PromptResult {
    /// Updated message history after execution.
    pub messages: Vec<BaseMessage>,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Whether a compact occurred during execution.
    pub compacted: bool,
}

/// Shared agent execution pipeline with auto-compact support.
///
/// This function encapsulates steps 2-7 of the prompt execution flow:
/// 1. Create event channel + cancel token
/// 2. Build agent via [`build_system_prompt`] + [`builder::build_agent`]
/// 3. Spawn background event pump using the provided [`EventSink`]
/// 4. Execute agent
/// 5. Check token thresholds → compact if needed (micro or full + resubmit)
/// 6. Wait for pump to drain
/// 7. Return updated messages
///
/// The caller is responsible for:
/// - Session management (storing/retrieving cwd, history, cancel_token)
/// - Choosing the broker (HITL/AskUser handler)
/// - Providing the correct `EventSink` implementation
#[allow(clippy::too_many_arguments)]
pub async fn execute_prompt(
    provider: &LlmProvider,
    peri_config: Arc<crate::provider::PeriConfig>,
    cwd: &str,
    content: String,
    history: Vec<BaseMessage>,
    is_empty_history: bool,
    permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    event_sink: Arc<dyn EventSink>,
    cancel: AgentCancellationToken,
    broker: Arc<dyn UserInteractionBroker>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>>,
    session_id: String,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<
        parking_lot::RwLock<
            std::collections::HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>,
        >,
    >,
    lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
) -> PromptResult {
    let agent_input = peri_agent::agent::react::AgentInput::text(content);

    // Compact config and context budget (computed once)
    let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
    compact_config.apply_env_overrides();
    let context_window = provider.context_window();
    let context_1m = peri_config.config.context_1m.unwrap_or(false);
    let effective_context_window = if context_1m {
        1_000_000
    } else {
        context_window
    };
    let budget = ContextBudget::new(effective_context_window)
        .with_auto_compact_threshold(compact_config.auto_compact_threshold)
        .with_warning_threshold(compact_config.micro_compact_threshold);

    let disable_compact = std::env::var("DISABLE_COMPACT").is_ok()
        || std::env::var("DISABLE_AUTO_COMPACT").is_ok()
        || !compact_config.auto_compact_enabled;

    // Event channel (lives for entire execute_prompt lifetime)
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutorEvent>();
    let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));

    // Background event pump
    let sink = event_sink;
    let sid = session_id.clone();
    let (pump_done_tx, pump_done_rx) = oneshot::channel();
    let pump_cw = effective_context_window;
    tokio::spawn(async move {
        while let Some(exec_event) = event_rx.recv().await {
            sink.push_event(&sid, &exec_event, pump_cw).await;
        }
        sink.push_done(&sid).await;
        let _ = pump_done_tx.send(());
    });

    let mut current_history = history;
    let mut total_resubmits: u32 = 0;
    const MAX_RESUBMITS: u32 = 3;
    let mut compacted = false;

    loop {
        // Create event handler for this round
        let event_handler: Arc<dyn AgentEventHandler> =
            Arc::new(peri_agent::agent::events::FnEventHandler({
                let tx = event_tx.clone();
                move |event: ExecutorEvent| {
                    if let Some(tx) = tx.lock().unwrap().as_ref() {
                        let _ = tx.send(event);
                    }
                }
            }));

        let features = PromptFeatures::detect();
        let system_prompt = build_system_prompt(None, cwd, features, &plugin_agent_dirs);

        let agent_output = builder::build_agent(AcpAgentConfig {
            provider: provider.clone(),
            cwd: cwd.to_string(),
            system_prompt,
            event_handler,
            cancel: cancel.clone(),
            permission_mode: permission_mode.clone(),
            peri_config: Arc::new(peri_config.as_ref().clone()),
            cron_scheduler: cron_scheduler.clone(),
            agent_overrides: None,
            preload_skills: Vec::new(),
            session_id: Some(session_id.clone()),
            broker: broker.clone(),
            plugin_skill_dirs: plugin_skill_dirs.clone(),
            plugin_agent_dirs: plugin_agent_dirs.clone(),
            hook_groups: hook_groups.clone(),
            hook_session_start: is_empty_history && total_resubmits == 0,
            mcp_pool: mcp_pool.clone(),
            tool_search_index: tool_search_index.clone(),
            shared_tools: shared_tools.clone(),
            child_handler_factory: None,
            lsp_servers: lsp_servers.clone(),
        });

        // Execute agent
        let mut agent_state = AgentState::with_messages(cwd.to_string(), current_history);
        let result = agent_output
            .executor
            .execute(agent_input.clone(), &mut agent_state, Some(cancel.clone()))
            .await;
        drop(agent_output);

        let ok = result.is_ok();
        if let Err(e) = &result {
            error!(session_id = %session_id, error = %e, "Agent execution failed");
        }

        if !ok || cancel.is_cancelled() {
            close_channel(&event_tx);
            wait_for_pump(pump_done_rx, &session_id).await;
            return PromptResult {
                messages: agent_state.into_messages(),
                ok,
                compacted,
            };
        }

        // Check token budget before consuming agent_state
        let should_auto = !disable_compact
            && budget.should_auto_compact(agent_state.token_tracker())
            && total_resubmits < MAX_RESUBMITS;
        let should_micro =
            !disable_compact && !should_auto && budget.should_warn(agent_state.token_tracker());
        let do_compact_check = should_auto || should_micro;

        let mut messages = agent_state.into_messages();

        // ── Compact check ──
        if do_compact_check && messages.len() > 1 {
            if should_auto {
                // Full compact + resubmit
                info!(session_id = %session_id, "auto-compact: threshold reached, triggering full compact");
                let all_hooks: Vec<_> = hook_groups.iter().flatten().cloned().collect();
                let hook_ctx = HookContext {
                    cwd: cwd.to_string(),
                    session_id: session_id.clone(),
                    transcript_path: String::new(),
                    provider_name: provider.display_name().to_string(),
                    instructions: String::new(),
                };

                match compact_runner::run_full_compact(
                    &messages,
                    provider.clone().into_model().as_ref(),
                    &compact_config,
                    cwd,
                    &event_tx,
                    &cancel,
                    &all_hooks,
                    &hook_ctx,
                )
                .await
                {
                    Ok(output) => {
                        compacted = true;
                        current_history = output.new_messages;
                        total_resubmits += 1;
                        info!(
                            session_id = %session_id,
                            resubmit = total_resubmits,
                            "auto-compact: resubmitting with compacted context"
                        );
                        continue;
                    }
                    Err(e) => {
                        warn!(session_id = %session_id, error = %e, "auto-compact: failed, returning original messages");
                    }
                }
            } else if should_micro {
                // Micro-compact only
                compact_runner::run_micro_compact(&mut messages, &compact_config, &event_tx);
            }
        }

        // Done — close event channel and wait for pump
        close_channel(&event_tx);
        wait_for_pump(pump_done_rx, &session_id).await;
        return PromptResult {
            messages,
            ok: true,
            compacted,
        };
    }
}

fn close_channel(
    event_tx: &Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<ExecutorEvent>>>>,
) {
    let mut tx_guard = event_tx.lock().unwrap();
    *tx_guard = None;
}

async fn wait_for_pump(pump_done_rx: oneshot::Receiver<()>, session_id: &str) {
    match pump_done_rx.await {
        Ok(()) => debug!(session_id, "Event pump done"),
        Err(_) => error!(session_id, "Event pump done channel closed unexpectedly"),
    }
}
