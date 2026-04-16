use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response, Sse},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{mpsc, RwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info};

use super::{
    accepts_json_and_sse, content_type_is_json, extract_jsonrpc_id, extract_session_id_from_result,
    get_connection_id, get_session_id, is_initialize_request, is_jsonrpc_request,
    is_jsonrpc_response_or_error, is_session_creating_request, EVENT_STREAM_MIME_TYPE,
    HEADER_CONNECTION_ID, HEADER_SESSION_ID,
};
use crate::acp::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::acp::server_factory::AcpServer;

// ── Message Router ──────────────────────────────────────────────────────
//
// Each HTTP connection has a single agent task producing JSON-RPC messages
// on one `UnboundedReceiver<String>`. The router reads from that receiver
// and fans messages out to per-request SSE streams and GET listener streams.
//
// Routing rules:
//   Response (has `id` + `result`/`error`)
//     → Send to the request stream registered for that id, then remove it.
//   Everything else (notifications, server-to-client requests)
//     → Broadcast to ALL active request streams + ALL GET listeners.

struct MessageRouter {
    /// Per-request SSE senders, keyed by the JSON-RPC request `id` (as string).
    request_streams: RwLock<HashMap<String, mpsc::UnboundedSender<String>>>,
    /// GET listener senders, keyed by `Acp-Session-Id`.
    get_listeners: RwLock<HashMap<String, Vec<mpsc::UnboundedSender<String>>>>,
}

impl MessageRouter {
    fn new() -> Self {
        Self {
            request_streams: RwLock::new(HashMap::new()),
            get_listeners: RwLock::new(HashMap::new()),
        }
    }

    /// Register a per-request sender. Returns the receiver for the SSE stream.
    async fn register_request(&self, request_id: String) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.request_streams.write().await.insert(request_id, tx);
        rx
    }

    /// Register a GET listener for a session. Returns the receiver.
    async fn register_get_listener(&self, session_id: String) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.get_listeners
            .write()
            .await
            .entry(session_id)
            .or_default()
            .push(tx);
        rx
    }

    /// Route a single message from the agent to the appropriate streams.
    async fn route(&self, msg: &str) {
        let parsed: Value = match serde_json::from_str(msg) {
            Ok(v) => v,
            Err(_) => {
                self.broadcast(msg).await;
                return;
            }
        };

        if is_jsonrpc_response_or_error(&parsed) {
            if let Some(id) = extract_jsonrpc_id(&parsed) {
                let mut streams = self.request_streams.write().await;
                if let Some(tx) = streams.remove(&id) {
                    let _ = tx.send(msg.to_string());
                    // Dropping tx closes the channel, signalling the SSE stream to end.
                } else {
                    // No registered stream — broadcast as fallback.
                    drop(streams);
                    self.broadcast(msg).await;
                }
            } else {
                self.broadcast(msg).await;
            }
        } else {
            // Notification or server-to-client request — broadcast.
            self.broadcast(msg).await;
        }
    }

    /// Send a message to all active request streams and all GET listeners.
    async fn broadcast(&self, msg: &str) {
        {
            let streams = self.request_streams.read().await;
            for tx in streams.values() {
                let _ = tx.send(msg.to_string());
            }
        }
        {
            let mut listeners = self.get_listeners.write().await;
            for senders in listeners.values_mut() {
                senders.retain(|tx| tx.send(msg.to_string()).is_ok());
            }
        }
    }
}

// ── HTTP Connection ─────────────────────────────────────────────────────

struct HttpConnection {
    to_agent_tx: mpsc::Sender<String>,
    router: Arc<MessageRouter>,
    _router_handle: tokio::task::JoinHandle<()>,
    _agent_handle: tokio::task::JoinHandle<()>,
}

// ── HttpState ───────────────────────────────────────────────────────────

pub(crate) struct HttpState {
    server: Arc<AcpServer>,
    connections: RwLock<HashMap<String, HttpConnection>>,
}

impl HttpState {
    pub fn new(server: Arc<AcpServer>) -> Self {
        Self {
            server,
            connections: RwLock::new(HashMap::new()),
        }
    }

    async fn create_connection(&self) -> Result<String, StatusCode> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

        let agent = self.server.create_agent().await.map_err(|e| {
            error!("Failed to create agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let connection_id = uuid::Uuid::new_v4().to_string();

        let read_stream = ReceiverToAsyncRead::new(to_agent_rx);
        let write_stream = SenderToAsyncWrite::new(from_agent_tx);
        let fut =
            crate::acp::server::serve(agent, read_stream.compat(), write_stream.compat_write());
        let agent_handle = tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("ACP connection error: {}", e);
            }
        });

        // Spawn the router task that reads from the agent and dispatches.
        let router = Arc::new(MessageRouter::new());
        let router_clone = Arc::clone(&router);
        let router_handle = tokio::spawn(async move {
            run_router(from_agent_rx, router_clone).await;
        });

        self.connections.write().await.insert(
            connection_id.clone(),
            HttpConnection {
                to_agent_tx,
                router,
                _router_handle: router_handle,
                _agent_handle: agent_handle,
            },
        );

        info!(connection_id = %connection_id, "Connection created");
        Ok(connection_id)
    }

    async fn has_connection(&self, connection_id: &str) -> bool {
        self.connections.read().await.contains_key(connection_id)
    }

    async fn remove_connection(&self, connection_id: &str) {
        if let Some(conn) = self.connections.write().await.remove(connection_id) {
            conn._agent_handle.abort();
            conn._router_handle.abort();
            info!(connection_id = %connection_id, "Connection removed");
        }
    }

    async fn send_to_agent(&self, connection_id: &str, message: String) -> Result<(), StatusCode> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(connection_id)
            .ok_or(StatusCode::NOT_FOUND)?;
        conn.to_agent_tx
            .send(message)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn get_router(&self, connection_id: &str) -> Result<Arc<MessageRouter>, StatusCode> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(connection_id)
            .ok_or(StatusCode::NOT_FOUND)?;
        Ok(Arc::clone(&conn.router))
    }
}

/// Router task: reads every message from the agent and dispatches it.
async fn run_router(
    mut from_agent_rx: mpsc::UnboundedReceiver<String>,
    router: Arc<MessageRouter>,
) {
    while let Some(msg) = from_agent_rx.recv().await {
        router.route(&msg).await;
    }
    debug!("Router task exiting — agent channel closed");
}

// ── SSE helpers ─────────────────────────────────────────────────────────

fn sse_from_receiver(
    mut rx: mpsc::UnboundedReceiver<String>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = async_stream::stream! {
        while let Some(msg) = rx.recv().await {
            yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
        }
    };
    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text(""),
    )
}

// ── POST /acp handlers ─────────────────────────────────────────────────

/// Handle `initialize` — create a connection, open a per-request SSE stream,
/// and return `Acp-Connection-Id` in the response header.
///
/// The SSE stream delivers the initialize response and then closes.
async fn handle_initialize(state: Arc<HttpState>, json_message: &Value) -> Response {
    let connection_id = match state.create_connection().await {
        Ok(id) => id,
        Err(status) => return status.into_response(),
    };

    let request_id = match extract_jsonrpc_id(json_message) {
        Some(id) => id,
        None => {
            state.remove_connection(&connection_id).await;
            return (StatusCode::BAD_REQUEST, "Missing request id").into_response();
        }
    };

    let router = match state.get_router(&connection_id).await {
        Ok(r) => r,
        Err(status) => {
            state.remove_connection(&connection_id).await;
            return status.into_response();
        }
    };

    let rx = router.register_request(request_id).await;

    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_to_agent(&connection_id, message_str).await {
        state.remove_connection(&connection_id).await;
        return status.into_response();
    }

    let mut response = sse_from_receiver(rx).into_response();
    response
        .headers_mut()
        .insert(HEADER_CONNECTION_ID, connection_id.parse().unwrap());
    response
}

/// Handle a JSON-RPC request — open a per-request SSE stream that delivers
/// notifications and the eventual response, then closes.
///
/// For `session/new`, `session/load`, and `session/fork`, the transport peeks
/// at the response to extract `sessionId` and set the `Acp-Session-Id` header.
async fn handle_request(
    state: Arc<HttpState>,
    connection_id: &str,
    json_message: &Value,
) -> Response {
    let request_id = match extract_jsonrpc_id(json_message) {
        Some(id) => id,
        None => return (StatusCode::BAD_REQUEST, "Missing request id").into_response(),
    };

    let router = match state.get_router(connection_id).await {
        Ok(r) => r,
        Err(status) => return status.into_response(),
    };

    let rx = router.register_request(request_id).await;

    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_to_agent(connection_id, message_str).await {
        return status.into_response();
    }

    if is_session_creating_request(json_message) {
        return build_session_creating_sse(rx).await;
    }

    sse_from_receiver(rx).into_response()
}

/// For session-creating requests, peek at the response to extract `sessionId`
/// and set the `Acp-Session-Id` header before streaming events.
async fn build_session_creating_sse(mut rx: mpsc::UnboundedReceiver<String>) -> Response {
    // Collect all messages. The response (with sessionId) may not be the first
    // message — there can be notifications before it (e.g. history replay for
    // session/load). We buffer everything and look for the sessionId in any
    // response message.
    let mut messages = Vec::new();
    let mut session_id: Option<String> = None;

    while let Some(msg) = rx.recv().await {
        if session_id.is_none() {
            if let Ok(parsed) = serde_json::from_str::<Value>(&msg) {
                if is_jsonrpc_response_or_error(&parsed) {
                    session_id = extract_session_id_from_result(&parsed);
                }
            }
        }
        messages.push(msg);
    }

    let stream = async_stream::stream! {
        for msg in messages {
            yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
        }
    };

    let sse = Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text(""),
    );

    let mut response = sse.into_response();
    if let Some(sid) = session_id {
        if let Ok(val) = sid.parse() {
            response.headers_mut().insert(HEADER_SESSION_ID, val);
        }
    }
    response
}

/// Handle a notification or client response — forward to agent, return 202.
async fn handle_notification_or_response(
    state: Arc<HttpState>,
    connection_id: &str,
    json_message: &Value,
) -> Response {
    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_to_agent(connection_id, message_str).await {
        return status.into_response();
    }
    StatusCode::ACCEPTED.into_response()
}

// ── POST /acp entry point ───────────────────────────────────────────────

/// POST /acp — unified entry point.
///
/// Routing per the RFD:
/// - `initialize` (no `Acp-Connection-Id`) → create connection, return SSE
/// - JSON-RPC request → forward, return per-request SSE
/// - Notification/response → forward, return 202
pub(crate) async fn handle_post(
    State(state): State<Arc<HttpState>>,
    request: Request<Body>,
) -> Response {
    if !accepts_json_and_sse(&request) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "Not Acceptable: Client must accept both application/json and text/event-stream",
        )
            .into_response();
    }

    if !content_type_is_json(&request) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported Media Type: Content-Type must be application/json",
        )
            .into_response();
    }

    let connection_id = get_connection_id(&request);
    let _session_id = get_session_id(&request);

    let body_bytes = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    let json_message: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse JSON: {}", e);
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response();
        }
    };

    if json_message.is_array() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            "Batch requests are not supported",
        )
            .into_response();
    }

    // Initialize — no Acp-Connection-Id required.
    if is_initialize_request(&json_message) {
        return handle_initialize(state, &json_message).await;
    }

    // Everything else requires Acp-Connection-Id.
    let Some(conn_id) = connection_id else {
        return (
            StatusCode::BAD_REQUEST,
            "Bad Request: Acp-Connection-Id header required",
        )
            .into_response();
    };

    if !state.has_connection(&conn_id).await {
        return (StatusCode::NOT_FOUND, "Connection not found").into_response();
    }

    if is_jsonrpc_request(&json_message) {
        handle_request(state, &conn_id, &json_message).await
    } else {
        // Notification or client response — fire and forget.
        handle_notification_or_response(state, &conn_id, &json_message).await
    }
}

// ── GET /acp (session-scoped SSE listener) ──────────────────────────────

/// GET /acp — optional long-lived session-scoped SSE stream for
/// server-initiated messages.
///
/// Requires both `Acp-Connection-Id` and `Acp-Session-Id`. The server
/// delivers only events belonging to that session on this stream.
pub(crate) async fn handle_get(state: Arc<HttpState>, request: Request<Body>) -> Response {
    let accept = request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !accept.contains(EVENT_STREAM_MIME_TYPE) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "Not Acceptable: Client must accept text/event-stream",
        )
            .into_response();
    }

    let connection_id = match get_connection_id(&request) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Connection-Id header required",
            )
                .into_response();
        }
    };

    let session_id = match get_session_id(&request) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required",
            )
                .into_response();
        }
    };

    if !state.has_connection(&connection_id).await {
        return (StatusCode::NOT_FOUND, "Connection not found").into_response();
    }

    let router = match state.get_router(&connection_id).await {
        Ok(r) => r,
        Err(status) => return status.into_response(),
    };

    let rx = router.register_get_listener(session_id).await;
    sse_from_receiver(rx).into_response()
}

// ── DELETE /acp ─────────────────────────────────────────────────────────

/// DELETE /acp — terminate a connection and all associated sessions.
///
/// Requires `Acp-Connection-Id`.
pub(crate) async fn handle_delete(
    State(state): State<Arc<HttpState>>,
    request: Request<Body>,
) -> Response {
    let connection_id = match get_connection_id(&request) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Connection-Id header required",
            )
                .into_response();
        }
    };

    if !state.has_connection(&connection_id).await {
        return (StatusCode::NOT_FOUND, "Connection not found").into_response();
    }

    state.remove_connection(&connection_id).await;
    StatusCode::ACCEPTED.into_response()
}
