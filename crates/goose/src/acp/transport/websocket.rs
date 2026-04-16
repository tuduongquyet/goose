use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info, warn};

use super::HEADER_CONNECTION_ID;
use crate::acp::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::acp::server_factory::AcpServer;

/// Transport-level state for WebSocket connections.
///
/// Each WebSocket upgrade creates a new connection keyed by
/// `Acp-Connection-Id`. The connection owns a single agent task and a pair
/// of channels for bidirectional JSON-RPC messaging over the WebSocket.
pub(crate) struct WsState {
    server: Arc<AcpServer>,
    connections: RwLock<HashMap<String, WsConnection>>,
}

struct WsConnection {
    to_agent_tx: mpsc::Sender<String>,
    from_agent_rx: mpsc::UnboundedReceiver<String>,
    handle: tokio::task::JoinHandle<()>,
}

impl WsState {
    pub fn new(server: Arc<AcpServer>) -> Self {
        Self {
            server,
            connections: RwLock::new(HashMap::new()),
        }
    }

    async fn create_connection(&self) -> anyhow::Result<String> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

        let agent = self.server.create_agent().await?;

        let connection_id = uuid::Uuid::new_v4().to_string();

        let read_stream = ReceiverToAsyncRead::new(to_agent_rx);
        let write_stream = SenderToAsyncWrite::new(from_agent_tx);
        let fut =
            crate::acp::server::serve(agent, read_stream.compat(), write_stream.compat_write());
        let handle = tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("ACP WebSocket connection error: {}", e);
            }
        });

        self.connections.write().await.insert(
            connection_id.clone(),
            WsConnection {
                to_agent_tx,
                from_agent_rx,
                handle,
            },
        );

        info!(connection_id = %connection_id, "WebSocket connection created");
        Ok(connection_id)
    }

    async fn remove_connection(&self, connection_id: &str) {
        if let Some(conn) = self.connections.write().await.remove(connection_id) {
            conn.handle.abort();
            info!(connection_id = %connection_id, "WebSocket connection removed");
        }
    }
}

/// GET /acp with `Upgrade: websocket` — create a connection and upgrade.
///
/// Returns `Acp-Connection-Id` in the HTTP 101 response headers. The client
/// must still send `initialize` as the first JSON-RPC message over the
/// WebSocket to negotiate capabilities.
pub(crate) async fn handle_get(state: Arc<WsState>, ws: WebSocketUpgrade) -> Response {
    let connection_id = match state.create_connection().await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to create WebSocket connection: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create WebSocket connection",
            )
                .into_response();
        }
    };

    let mut response = ws.on_upgrade({
        let connection_id = connection_id.clone();
        move |socket| handle_ws(socket, state, connection_id)
    });
    response
        .headers_mut()
        .insert(HEADER_CONNECTION_ID, connection_id.parse().unwrap());
    response
}

/// Bidirectional message loop for a WebSocket connection.
pub(crate) async fn handle_ws(socket: WebSocket, state: Arc<WsState>, connection_id: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Take ownership of the channels out of the connection map so we can
    // use them without holding the lock.
    let (to_agent, mut from_agent_rx) = {
        let mut connections = state.connections.write().await;
        match connections.get_mut(&connection_id) {
            Some(conn) => {
                // Take the receiver — we'll drive it in this task.
                // Replace with a dummy that will never produce messages.
                let (_, dummy_rx) = mpsc::unbounded_channel();
                let real_rx = std::mem::replace(&mut conn.from_agent_rx, dummy_rx);
                (conn.to_agent_tx.clone(), real_rx)
            }
            None => {
                error!(connection_id = %connection_id, "Connection not found after creation");
                return;
            }
        }
    };

    debug!(connection_id = %connection_id, "Starting bidirectional message loop");

    loop {
        tokio::select! {
            Some(msg_result) = ws_rx.next() => {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str = text.to_string();
                        debug!(connection_id = %connection_id, "Client → Agent: {} bytes", text_str.len());
                        if let Err(e) = to_agent.send(text_str).await {
                            error!(connection_id = %connection_id, "Failed to send to agent: {}", e);
                            break;
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        debug!(connection_id = %connection_id, "Client closed connection: {:?}", frame);
                        break;
                    }
                    Ok(Message::Ping(_) | Message::Pong(_)) => continue,
                    Ok(Message::Binary(_)) => {
                        warn!(connection_id = %connection_id, "Ignoring binary message (ACP uses text)");
                        continue;
                    }
                    Err(e) => {
                        error!(connection_id = %connection_id, "WebSocket error: {}", e);
                        break;
                    }
                }
            }

            Some(text) = from_agent_rx.recv() => {
                debug!(connection_id = %connection_id, "Agent → Client: {} bytes", text.len());
                if let Err(e) = ws_tx.send(Message::Text(text.into())).await {
                    error!(connection_id = %connection_id, "Failed to send to client: {}", e);
                    break;
                }
            }

            else => {
                debug!(connection_id = %connection_id, "Both channels closed");
                break;
            }
        }
    }

    debug!(connection_id = %connection_id, "Cleaning up connection");
    state.remove_connection(&connection_id).await;
}
