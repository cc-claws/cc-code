use agent_client_protocol::role::acp::Client;
use agent_client_protocol::schema::{
    Content, ContentBlock, PermissionOption, PermissionOptionKind, RequestId,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, TextContent, ToolCallContent, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields,
};
use agent_client_protocol::ConnectionTo;
use async_trait::async_trait;
use peri_agent::interaction::{
    ApprovalDecision, InteractionContext, InteractionResponse, UserInteractionBroker,
};
use tokio::sync::{mpsc, oneshot};

use super::session::{PendingRequestEntry, SessionManager};

pub struct PendingPermission {
    pub context: InteractionContext,
    pub response_tx: oneshot::Sender<InteractionResponse>,
}

pub struct AcpInteractionBroker {
    permission_tx: mpsc::Sender<PendingPermission>,
}

impl AcpInteractionBroker {
    pub fn new(permission_tx: mpsc::Sender<PendingPermission>) -> Self {
        Self { permission_tx }
    }
}

#[async_trait]
impl UserInteractionBroker for AcpInteractionBroker {
    async fn request(&self, context: InteractionContext) -> InteractionResponse {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .permission_tx
            .send(PendingPermission {
                context,
                response_tx,
            })
            .await
            .is_err()
        {
            return InteractionResponse::Decisions(vec![ApprovalDecision::Reject {
                reason: "ACP connection closed".into(),
            }]);
        }

        response_rx
            .await
            .unwrap_or(InteractionResponse::Decisions(vec![
                ApprovalDecision::Reject {
                    reason: "Permission timeout".into(),
                },
            ]))
    }
}

/// 权限转发循环：将 HITL 权限请求通过 ACP request_permission 转发给 Client
pub async fn permission_forwarding_loop(
    mut rx: mpsc::Receiver<PendingPermission>,
    conn: ConnectionTo<Client>,
    session_id: SessionId,
    mgr: SessionManager,
) {
    while let Some(pending) = rx.recv().await {
        let response = handle_pending_permission(pending.context, &conn, &session_id, &mgr).await;
        let _ = pending.response_tx.send(response);
    }
}

async fn handle_pending_permission(
    ctx: InteractionContext,
    conn: &ConnectionTo<Client>,
    session_id: &SessionId,
    mgr: &SessionManager,
) -> InteractionResponse {
    match ctx {
        InteractionContext::Approval { items } => {
            let mut decisions = Vec::with_capacity(items.len());
            for item in &items {
                // 构建 ToolCallUpdate 描述待审批的工具调用
                let tool_update = ToolCallUpdate::new(
                    item.tool_call_id.clone(),
                    ToolCallUpdateFields::new()
                        .status(ToolCallStatus::Pending)
                        .content(vec![ToolCallContent::Content(Content::new(
                            ContentBlock::Text(TextContent::new(truncate_str(
                                &item.tool_input.to_string(),
                                500,
                            ))),
                        ))]),
                );

                let options = vec![
                    PermissionOption::new(
                        "allow_once",
                        "Allow once",
                        PermissionOptionKind::AllowOnce,
                    ),
                    PermissionOption::new(
                        "reject_once",
                        "Reject",
                        PermissionOptionKind::RejectOnce,
                    ),
                ];

                let request =
                    RequestPermissionRequest::new(session_id.clone(), tool_update, options);

                // Send request and get its JSON-RPC ID for cancellation tracking
                let sent = conn.send_request(request);
                let request_id = sent.id();

                // Create a cancel channel for this request
                let (cancel_tx, cancel_rx) = oneshot::channel::<()>();

                // Register in session's pending requests table with a unique generation
                let rid = serde_json::from_value::<RequestId>(request_id.clone())
                    .unwrap_or(RequestId::Null);
                if let Some(session) = mgr.get_session(session_id.0.as_ref()) {
                    let gen = session
                        .pending_gen
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    session.pending_requests.insert(
                        rid.clone(),
                        PendingRequestEntry {
                            cancel_tx,
                            generation: gen,
                        },
                    );
                } else {
                    // Session gone, reject immediately
                    decisions.push(ApprovalDecision::Reject {
                        reason: "Session not found".into(),
                    });
                    continue;
                }

                // Convert SentRequest to a receivable future via a oneshot channel
                let (tx, rx) = oneshot::channel();
                let sid_for_cleanup = session_id.clone();
                let rid_for_cleanup = rid.clone();
                if let Err(e) = sent.on_receiving_result(move |result| {
                    let _ = tx.send(result);
                    async { Ok(()) }
                }) {
                    tracing::warn!(error = %e, "Failed to register response handler for permission");
                    // Cleanup pending entry
                    if let Some(session) = mgr.get_session(sid_for_cleanup.0.as_ref()) {
                        session.pending_requests.remove(&rid_for_cleanup);
                    }
                    decisions.push(ApprovalDecision::Reject {
                        reason: format!("Permission request failed: {e}"),
                    });
                    continue;
                }

                // Race between client response and cancellation signal
                let decision = tokio::select! {
                    result = rx => {
                        match result {
                            Ok(Ok(resp)) => map_permission_response(resp),
                            Err(_) | Ok(Err(_)) => {
                                tracing::warn!("Permission request channel error");
                                ApprovalDecision::Reject {
                                    reason: "Permission request failed".into(),
                                }
                            }
                        }
                    }
                    _ = cancel_rx => {
                        tracing::info!(%session_id, ?request_id, "Permission request cancelled via $/cancel_request");
                        ApprovalDecision::Reject {
                            reason: "Cancelled by client".into(),
                        }
                    }
                };

                // Cleanup pending entry after resolution
                if let Some(session) = mgr.get_session(session_id.0.as_ref()) {
                    session.pending_requests.remove(&rid);
                }

                decisions.push(decision);
            }
            InteractionResponse::Decisions(decisions)
        }
        InteractionContext::Questions { requests } => {
            // ACP 没有 AskUser 等价机制，返回空答案
            tracing::warn!(
                count = requests.len(),
                "AskUser questions not supported in ACP mode, returning empty answers"
            );
            InteractionResponse::Answers(
                requests
                    .into_iter()
                    .map(|q| peri_agent::interaction::QuestionAnswer {
                        id: q.id,
                        selected: vec![],
                        text: Some(String::new()),
                    })
                    .collect(),
            )
        }
    }
}

fn map_permission_response(resp: RequestPermissionResponse) -> ApprovalDecision {
    match resp.outcome {
        RequestPermissionOutcome::Selected(selected) => {
            let SelectedPermissionOutcome { option_id, .. } = selected;
            match option_id.0.as_ref() {
                "allow_once" | "allow_always" => ApprovalDecision::Approve,
                _ => ApprovalDecision::Reject {
                    reason: format!("User selected {option_id}"),
                },
            }
        }
        RequestPermissionOutcome::Cancelled => ApprovalDecision::Reject {
            reason: "Cancelled by user".into(),
        },
        _ => ApprovalDecision::Reject {
            reason: "Unknown response".into(),
        },
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s.floor_char_boundary(max_len);
        format!("{}...", &s[..boundary])
    }
}
