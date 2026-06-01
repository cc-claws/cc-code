//! In-memory ACP transport using tokio mpsc channels.
//!
//! `mpsc_transport_pair()` creates a connected pair of transports — one for the
//! ACP server side and one for the client (TUI) side. Messages flow through two
//! pairs of unbounded channels.
//!
//! Each transport spawns a background pump task that continuously reads incoming
//! messages and dispatches responses to the pending request map, so `send_request`
//! can await the oneshot channel without deadlocking.

use async_trait::async_trait;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::{
    types::{AcpError, IncomingMessage, RequestId},
    AcpTransport,
};

// ---------- internal channel message types ----------

#[derive(Debug)]
enum ChannelMessage {
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: RequestId,
        result: Result<Value, AcpError>,
    },
}

// ---------- shared pending map ----------

type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, AcpError>>>>>;

/// Background pump that reads from the incoming channel and dispatches
/// Response messages to the pending request map.
async fn pump_incoming(
    mut rx: mpsc::UnboundedReceiver<ChannelMessage>,
    pending: PendingMap,
    outgoing_tx: mpsc::UnboundedSender<IncomingMessage>,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            ChannelMessage::Response { id, result } => {
                if let RequestId::Number(n) = &id {
                    if let Some(tx) = pending.lock().await.remove(n) {
                        let _ = tx.send(result);
                        continue; // consumed internally
                    }
                }
                // Unmatched response — forward to caller
                let _ = outgoing_tx.send(IncomingMessage::Response { id, result });
            }
            ChannelMessage::Request { id, method, params } => {
                let _ = outgoing_tx.send(IncomingMessage::Request { id, method, params });
            }
            ChannelMessage::Notification { method, params } => {
                let _ = outgoing_tx.send(IncomingMessage::Notification { method, params });
            }
        }
    }
}

// ---------- MpscClientTransport ----------

/// Client-side (TUI) transport.
pub struct MpscClientTransport {
    /// Sends client → server messages.
    client_tx: mpsc::UnboundedSender<ChannelMessage>,
    /// Receives processed incoming messages from the pump.
    incoming_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<IncomingMessage>>,
    /// Pending requests awaiting response.
    pending: PendingMap,
    /// Next request ID.
    next_id: Arc<AtomicI64>,
}

impl MpscClientTransport {
    fn new(
        client_tx: mpsc::UnboundedSender<ChannelMessage>,
        server_rx: mpsc::UnboundedReceiver<ChannelMessage>,
        pending: PendingMap,
        next_id: Arc<AtomicI64>,
    ) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let pending_clone = pending.clone();

        // Background pump: dispatches Response messages to the pending map,
        // forwards Requests and Notifications to incoming_rx.
        tokio::spawn(async move {
            pump_incoming(server_rx, pending_clone, incoming_tx).await;
        });

        Self {
            client_tx,
            incoming_rx: tokio::sync::Mutex::new(incoming_rx),
            pending,
            next_id,
        }
    }
}

#[async_trait]
impl AcpTransport for MpscClientTransport {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = RequestId::Number(id_num);
        let (response_tx, response_rx) = oneshot::channel();

        self.pending.lock().await.insert(id_num, response_tx);

        self.client_tx
            .send(ChannelMessage::Request {
                id: id.clone(),
                method: method.to_string(),
                params,
            })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))?;

        response_rx
            .await
            .map_err(|_| AcpError::new(-32603, "Request cancelled"))?
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<(), AcpError> {
        self.client_tx
            .send(ChannelMessage::Notification {
                method: method.to_string(),
                params,
            })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))
    }

    async fn recv(&self) -> Option<IncomingMessage> {
        self.incoming_rx.lock().await.recv().await
    }

    async fn send_response(
        &self,
        id: RequestId,
        result: Result<Value, AcpError>,
    ) -> Result<(), AcpError> {
        self.client_tx
            .send(ChannelMessage::Response { id, result })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))
    }
}

// ---------- MpscServerTransport ----------

/// Server-side (ACP) transport.
pub struct MpscServerTransport {
    /// Sends server → client messages.
    server_tx: mpsc::UnboundedSender<ChannelMessage>,
    /// Receives processed incoming messages from the pump.
    incoming_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<IncomingMessage>>,
    /// Pending responses from client (for server-initiated requests).
    pending: PendingMap,
    /// Next server request ID.
    next_id: Arc<AtomicI64>,
}

impl MpscServerTransport {
    fn new(
        client_rx: mpsc::UnboundedReceiver<ChannelMessage>,
        server_tx: mpsc::UnboundedSender<ChannelMessage>,
        pending: PendingMap,
        next_id: Arc<AtomicI64>,
    ) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let pending_clone = pending.clone();

        // Background pump
        tokio::spawn(async move {
            pump_incoming(client_rx, pending_clone, incoming_tx).await;
        });

        Self {
            server_tx,
            incoming_rx: tokio::sync::Mutex::new(incoming_rx),
            pending,
            next_id,
        }
    }
}

#[async_trait]
impl AcpTransport for MpscServerTransport {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = RequestId::Number(id_num);
        let (response_tx, response_rx) = oneshot::channel();

        self.pending.lock().await.insert(id_num, response_tx);

        self.server_tx
            .send(ChannelMessage::Request {
                id: id.clone(),
                method: method.to_string(),
                params,
            })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))?;

        response_rx
            .await
            .map_err(|_| AcpError::new(-32603, "Request cancelled"))?
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<(), AcpError> {
        self.server_tx
            .send(ChannelMessage::Notification {
                method: method.to_string(),
                params,
            })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))
    }

    async fn recv(&self) -> Option<IncomingMessage> {
        self.incoming_rx.lock().await.recv().await
    }

    async fn send_response(
        &self,
        id: RequestId,
        result: Result<Value, AcpError>,
    ) -> Result<(), AcpError> {
        self.server_tx
            .send(ChannelMessage::Response { id, result })
            .map_err(|_| AcpError::new(-32603, "Transport closed"))
    }
}

// ---------- factory ----------

/// Create a connected pair of in-memory ACP transports.
///
/// Returns `(client, server)` where:
/// - `client` is used by the TUI / ACP client side
/// - `server` is used by the ACP session manager side
///
/// Each transport spawns a background pump task for processing incoming
/// messages, so the pair must be created within a tokio runtime.
pub fn mpsc_transport_pair() -> (MpscClientTransport, MpscServerTransport) {
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let (server_tx, server_rx) = mpsc::unbounded_channel();

    let pending = Arc::new(Mutex::new(HashMap::new()));
    let next_id = Arc::new(AtomicI64::new(1));

    let client = MpscClientTransport::new(client_tx, server_rx, pending.clone(), next_id.clone());
    let server = MpscServerTransport::new(client_rx, server_tx, pending, next_id);

    (client, server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_request_response() {
        let (client, server) = mpsc_transport_pair();

        // Server side: echo back the params
        let server_handle = tokio::spawn(async move {
            if let Some(IncomingMessage::Request {
                id,
                method: _,
                params,
            }) = server.recv().await
            {
                let _ = server.send_response(id, Ok(params)).await;
            }
        });

        // Client sends a request
        let result = client
            .send_request("test/echo", json!({"hello": "world"}))
            .await
            .unwrap();
        assert_eq!(result, json!({"hello": "world"}));

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_notification() {
        let (client, server) = mpsc_transport_pair();

        client
            .send_notification("test/notify", json!({"msg": "ping"}))
            .await
            .unwrap();

        // Server receives it
        if let Some(IncomingMessage::Notification { method, params }) = server.recv().await {
            assert_eq!(method, "test/notify");
            assert_eq!(params, json!({"msg": "ping"}));
        } else {
            panic!("expected notification");
        }
    }

    #[tokio::test]
    async fn test_bidirectional_server_notification_to_client() {
        let (client, server) = mpsc_transport_pair();

        // Server sends a notification to client
        server
            .send_notification("test/hello", json!({"msg": "from_server"}))
            .await
            .unwrap();

        // Client receives it
        if let Some(IncomingMessage::Notification { method, params }) = client.recv().await {
            assert_eq!(method, "test/hello");
            assert_eq!(params, json!({"msg": "from_server"}));
        } else {
            panic!("expected notification from server");
        }
    }
}
