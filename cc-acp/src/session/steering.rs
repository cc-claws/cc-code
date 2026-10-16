//! TUI ACP 扩展：补充信息等待安全边界确认，而非仅确认入队。

use cc_agent::{agent::steering::SteeringQueue, messages::MessageContent};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::transport::types::AcpError;

/// 仅用于 peri/session/steer：未进入运行中队列，调用方可保留并重试。
pub const STEERING_NOT_ENQUEUED: i64 = -32001;
/// 仅用于 peri/session/steer：已入队但退出前未写入历史，调用方可保留并重试。
pub const STEERING_NOT_CONSUMED: i64 = -32002;

pub fn enqueue(
    queue: Option<&SteeringQueue>,
    params: &Value,
) -> Result<oneshot::Receiver<()>, AcpError> {
    let value = params
        .get("message")
        .and_then(|message| message.get("content"))
        .ok_or_else(|| AcpError::new(-32602, "missing message.content"))?;
    let content: MessageContent = serde_json::from_value(value.clone())
        .map_err(|error| AcpError::new(-32602, format!("invalid content: {error}")))?;
    if content.content_blocks().is_empty() {
        return Err(AcpError::new(-32602, "empty steering content"));
    }
    queue
        .and_then(|queue| queue.enqueue(content))
        .ok_or_else(|| AcpError::new(STEERING_NOT_ENQUEUED, "当前执行已结束，消息仍保留在队列中"))
}

pub async fn confirm(receipt: Result<oneshot::Receiver<()>, AcpError>) -> Result<Value, AcpError> {
    receipt?.await.map_err(|_| {
        AcpError::new(
            STEERING_NOT_CONSUMED,
            "本轮未接收补充信息，消息仍保留在队列中",
        )
    })?;
    Ok(json!({ "consumed": true }))
}

#[cfg(test)]
#[path = "steering_test.rs"]
mod tests;
