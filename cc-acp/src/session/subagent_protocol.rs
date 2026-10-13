//! Shared child-thread extensions used by the in-process and stdio transports.

use agent_client_protocol::{JsonRpcNotification, JsonRpcRequest};
use cc_agent::agent::events::AgentEvent;
use cc_middlewares::BackgroundTaskRegistry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::transport::types::AcpError;

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "session/update")]
#[serde(transparent)]
pub struct RoutedSessionNotification(pub Value);

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "peri/subagent_lifecycle")]
#[serde(transparent)]
pub struct SubagentLifecycleNotification(pub Value);

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "peri/agent_event")]
#[serde(transparent)]
pub struct PeriAgentNotification(pub Value);

/// Preserve extensions that the SDK's standard structs otherwise discard.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "session/prompt", response = agent_client_protocol::schema::PromptResponse)]
pub struct RoutedPromptRequest {
    #[serde(flatten)]
    pub request: agent_client_protocol::schema::PromptRequest,
    #[serde(rename = "targetThreadId", default)]
    pub target_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "session/cancel")]
pub struct RoutedCancelNotification {
    #[serde(flatten)]
    pub notification: agent_client_protocol::schema::CancelNotification,
    #[serde(rename = "targetThreadId", default)]
    pub target_thread_id: Option<String>,
}

pub fn source_metadata(source_agent_id: &str) -> Value {
    json!({ "sourceAgentId": source_agent_id })
}

pub fn session_update_payload(
    session_id: &str,
    update: &agent_client_protocol::schema::SessionUpdate,
    source_agent_id: Option<&str>,
) -> Result<Value, serde_json::Error> {
    let mut payload = json!({ "sessionId": session_id, "update": serde_json::to_value(update)? });
    if let Some(source) = source_agent_id {
        payload["_peri"] = source_metadata(source);
    }
    Ok(payload)
}

/// Lifecycle notifications have one authoritative route; never also send the raw event.
pub fn lifecycle_payload(session_id: &str, event: &AgentEvent) -> Option<Value> {
    let event = match event {
        AgentEvent::SubagentEvent { event, .. } => return lifecycle_payload(session_id, event),
        event => event,
    };
    let kind = match event {
        AgentEvent::SubagentStarted { .. } => "started",
        AgentEvent::SubagentProgress { .. } => "progress",
        AgentEvent::SubagentStopped { .. } => "stopped",
        _ => return None,
    };
    let serialized = serde_json::to_value(event).ok()?;
    let data = &serialized["value"];
    let mut payload = json!({
        "sessionId": session_id,
        "event": kind,
        "instanceId": data["instance_id"],
    });
    for (input, output) in [
        ("agent_name", "agentType"),
        ("task_preview", "taskPreview"),
        ("is_background", "isBackground"),
        ("timeout_secs", "timeoutSecs"),
        ("model", "model"),
        ("total_steps", "totalSteps"),
        ("elapsed_ms", "elapsedMs"),
        ("is_error", "isError"),
        ("result", "result"),
    ] {
        if let Some(value) = data.get(input) {
            payload[output] = value.clone();
        }
    }
    Some(payload)
}

pub fn target_thread_id(params: &Value) -> Option<&str> {
    params
        .get("targetThreadId")
        .or_else(|| params.pointer("/_meta/targetThreadId"))
        .or_else(|| params.pointer("/_meta/_peri/targetThreadId"))
        .and_then(Value::as_str)
}

pub fn enrich_lifecycle(payload: &mut Value, registry: Option<&BackgroundTaskRegistry>) {
    let Some(registry) = registry else {
        return;
    };
    let Some(id) = payload.get("instanceId").and_then(Value::as_str) else {
        return;
    };
    let Ok(snapshot) = registry.status(id) else {
        return;
    };
    payload["agentType"] = json!(snapshot.agent_name);
    payload["taskPreview"] = json!(snapshot.task_preview);
    payload["currentActivity"] = json!(snapshot.current_activity);
    payload["elapsedMs"] = json!(snapshot.elapsed_ms);
    payload["tokensUsed"] = json!(snapshot.tokens_used);
    payload["model"] = json!(snapshot.model);
    payload["timeoutSecs"] = json!(snapshot.timeout_secs);
    payload["status"] = serde_json::to_value(snapshot.status).unwrap_or_default()["type"].clone();
}

pub fn cancel_target(registry: &BackgroundTaskRegistry, target: &str) -> Result<(), AcpError> {
    if target == "*" {
        registry.cancel_all();
        Ok(())
    } else {
        registry
            .cancel(target)
            .map_err(|e| AcpError::new(-32602, e))
    }
}

pub fn enqueue_message(
    registry: &BackgroundTaskRegistry,
    params: &Value,
) -> Result<Value, AcpError> {
    let target = target_thread_id(params)
        .filter(|target| !target.is_empty() && *target != "*")
        .ok_or_else(|| AcpError::new(-32602, "missing targetThreadId"))?;
    let content = params
        .pointer("/message/content")
        .or_else(|| params.get("prompt"));
    let text = match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => {
            if blocks
                .iter()
                .any(|block| block.get("type").and_then(Value::as_str) != Some("text"))
            {
                return Err(AcpError::new(
                    -32602,
                    "child messages currently require text content",
                ));
            }
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => return Err(AcpError::new(-32602, "missing message content")),
    };
    if text.trim().is_empty() {
        return Err(AcpError::new(-32602, "message must not be empty"));
    }
    registry
        .send_message(target, &text)
        .map_err(|e| AcpError::new(-32602, e))?;
    Ok(json!({ "stopReason": "end_turn", "queued": true, "targetThreadId": target }))
}

#[cfg(test)]
#[path = "subagent_protocol_test.rs"]
mod tests;
