//! Session-serialized stdio turns and background completion continuation.

use super::*;
use agent_client_protocol::schema::{SessionInfoUpdate, StopReason};
use cc_acp::session::{event_sink::StdioEventSink, executor};
use cc_agent::agent::{events::BackgroundTaskResult, AgentCancellationToken};

fn completion_sender(
    ctx: &Arc<StdioContext>,
    cx: &ConnectionTo<Client>,
    sid: &str,
) -> Result<tokio::sync::mpsc::UnboundedSender<BackgroundTaskResult>, String> {
    let mut sessions = ctx.sessions.write();
    let session = sessions.get_mut(sid).ok_or("session not found")?;
    if let Some(sender) = &session.background_results {
        return Ok(sender.clone());
    }
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<BackgroundTaskResult>();
    session.background_results = Some(sender.clone());
    let weak_context = Arc::downgrade(ctx);
    let cx = cx.clone();
    let sid = sid.to_string();
    tokio::spawn(async move {
        while let Some(result) = receiver.recv().await {
            let Some(ctx) = weak_context.upgrade() else {
                break;
            };
            let mut results = vec![result];
            while let Ok(result) = receiver.try_recv() {
                results.push(result);
            }
            // Deliberately stopped tasks must not start another model turn.
            {
                let sessions = ctx.sessions.read();
                let Some(session) = sessions.get(&sid) else {
                    break;
                };
                results.retain(|result| !session.background_registry.status(&result.task_id)
                    .is_ok_and(|snapshot| snapshot.status == cc_middlewares::subagent::BackgroundTaskStatus::Stopped));
            }
            if results.is_empty() {
                continue;
            }
            if let Err(error) = execute_turn(
                &ctx,
                cx.clone(),
                &sid,
                MessageContent::text("Background agents completed. Please review the results."),
                results,
            )
            .await
            {
                tracing::warn!(%error, session_id = %sid, "Background continuation failed");
            }
        }
    });
    Ok(sender)
}

pub(super) async fn execute_turn(
    ctx: &Arc<StdioContext>,
    cx: ConnectionTo<Client>,
    sid: &str,
    content: MessageContent,
    background_results: Vec<BackgroundTaskResult>,
) -> Result<StopReason, String> {
    let prompt_lock = ctx
        .sessions
        .read()
        .get(sid)
        .map(|session| Arc::clone(&session.prompt_lock))
        .ok_or("session not found")?;
    // Wait without holding the session map: cancel and approvals remain responsive.
    let _guard = prompt_lock.lock().await;
    let sender = completion_sender(ctx, &cx, sid)?;
    let cancel = AgentCancellationToken::new();
    let (cwd, history, thread_id, frozen, registry, pool) = {
        let mut sessions = ctx.sessions.write();
        let session = sessions.get_mut(sid).ok_or("session not found")?;
        session.cancel_token = Some(cancel.clone());
        (
            session.cwd.clone(),
            session.history.clone(),
            session.thread_id.clone(),
            session.frozen.clone(),
            Arc::clone(&session.background_registry),
            Arc::clone(&session.agent_pool),
        )
    };
    let history_len = history.len();
    let history_ids: Vec<_> = history.iter().map(BaseMessage::id).collect();
    let broker: Arc<dyn cc_agent::interaction::UserInteractionBroker> =
        Arc::new(AcpPermissionBroker::new(cx.clone(), sid.to_string().into()));
    let sink = Arc::new(
        StdioEventSink::new(cx, sid.to_string().into())
            .with_background_results(sender)
            .with_background_registry(Arc::clone(&registry)),
    );
    let provider = ctx.provider.read().clone();
    let config = Arc::new(ctx.peri_config.read().clone());
    let result = executor::execute_prompt(
        &provider,
        config,
        &cwd,
        content,
        frozen,
        history,
        vec![],
        history_len == 0,
        Arc::clone(&ctx.permission_mode),
        cc_middlewares::hitl::ApprovalMemory::new(),
        sink.clone(),
        cancel,
        broker,
        None,
        ctx.plugin_skill_dirs.clone(),
        ctx.plugin_agent_dirs.clone(),
        ctx.hook_groups.clone(),
        Some(Arc::clone(&ctx.cron_scheduler)),
        sid.to_string(),
        ctx.mcp_pool.clone(),
        ctx.channel_state.clone(),
        Arc::clone(&ctx.tool_search_index),
        Arc::clone(&ctx.shared_tools),
        ctx.plugin_lsp_servers.clone(),
        ctx.langfuse_session.clone(),
        pool,
        Some(Arc::clone(&ctx.thread_store)),
        Some(thread_id.clone()),
        None,
        background_results,
        None,
        Some(registry),
    )
    .await;
    // Failed turns may still contain completed tool steps; preserve those messages too.
    let messages: Vec<_> = result
        .messages
        .into_iter()
        .filter(|message| !message.is_system())
        .collect();
    if messages.len() < history_len {
        if let Err(error) = ctx
            .thread_store
            .delete_messages(&thread_id, &history_ids)
            .await
        {
            tracing::warn!(%error, "Failed to replace compacted stdio history");
        }
        if let Err(error) = ctx
            .thread_store
            .append_messages(&thread_id, &messages)
            .await
        {
            tracing::warn!(%error, "Failed to persist compacted stdio history");
        }
    } else if messages.len() > history_len {
        if let Err(error) = ctx
            .thread_store
            .append_messages(&thread_id, &messages[history_len..])
            .await
        {
            tracing::warn!(%error, "Failed to persist stdio turn");
        }
    }
    if let Some(session) = ctx.sessions.write().get_mut(sid) {
        session.history = messages;
        session.cancel_token = None;
    }
    sink.send_update(SessionUpdate::SessionInfoUpdate(
        SessionInfoUpdate::new().updated_at(chrono::Utc::now().to_rfc3339()),
    ));
    Ok(match result.stop_reason {
        executor::PromptStopReason::Cancelled => StopReason::Cancelled,
        executor::PromptStopReason::MaxTurnRequests => StopReason::MaxTurnRequests,
        executor::PromptStopReason::EndTurn => StopReason::EndTurn,
    })
}
