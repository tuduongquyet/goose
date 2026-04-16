pub mod http;
pub mod websocket;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
        State,
    },
    http::{header, Method, Request},
    response::Response,
    routing::{delete, get, post},
    Router,
};
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::acp::server_factory::AcpServer;

pub(crate) const HEADER_CONNECTION_ID: &str = "Acp-Connection-Id";
pub(crate) const HEADER_SESSION_ID: &str = "Acp-Session-Id";
pub(crate) const EVENT_STREAM_MIME_TYPE: &str = "text/event-stream";
pub(crate) const JSON_MIME_TYPE: &str = "application/json";

pub(crate) fn accepts_json_and_sse(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept.contains(JSON_MIME_TYPE) && accept.contains(EVENT_STREAM_MIME_TYPE)
        })
}

pub(crate) fn content_type_is_json(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with(JSON_MIME_TYPE))
}

pub(crate) fn get_connection_id(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get(HEADER_CONNECTION_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub(crate) fn get_session_id(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get(HEADER_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub(crate) fn is_initialize_request(value: &Value) -> bool {
    value.get("method").is_some_and(|m| m == "initialize") && value.get("id").is_some()
}

pub(crate) fn is_session_creating_request(value: &Value) -> bool {
    value
        .get("method")
        .and_then(|m| m.as_str())
        .is_some_and(|m| m == "session/new" || m == "session/load" || m == "session/fork")
}

pub(crate) fn is_jsonrpc_request(value: &Value) -> bool {
    value.get("method").is_some() && value.get("id").is_some()
}

pub(crate) fn is_jsonrpc_response_or_error(value: &Value) -> bool {
    value.get("id").is_some() && (value.get("result").is_some() || value.get("error").is_some())
}

/// Extract the JSON-RPC `id` from a message. Returns the id as a string
/// regardless of whether it was originally a number or string.
pub(crate) fn extract_jsonrpc_id(value: &Value) -> Option<String> {
    value.get("id").map(|id| match id {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    })
}

/// Extract `sessionId` from a JSON-RPC result body.
/// Used by the transport to set `Acp-Session-Id` on session/new and session/load responses.
pub(crate) fn extract_session_id_from_result(value: &Value) -> Option<String> {
    value
        .get("result")
        .and_then(|r| r.get("sessionId"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

async fn handle_get(
    ws_upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    State(state): State<(Arc<http::HttpState>, Arc<websocket::WsState>)>,
    request: Request<Body>,
) -> Response {
    match ws_upgrade {
        Ok(ws) => websocket::handle_get(state.1, ws).await,
        Err(_) => http::handle_get(state.0, request).await,
    }
}

async fn health() -> &'static str {
    "ok"
}

pub fn create_router(server: Arc<AcpServer>) -> Router {
    let http_state = Arc::new(http::HttpState::new(server.clone()));
    let ws_state = Arc::new(websocket::WsState::new(server));

    let connection_id_header = HEADER_CONNECTION_ID.parse().unwrap();
    let session_id_header = HEADER_SESSION_ID.parse().unwrap();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            connection_id_header,
            session_id_header,
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_KEY,
            header::CONNECTION,
            header::UPGRADE,
        ]);

    Router::new()
        .route("/health", get(health))
        .route("/status", get(health))
        .route(
            "/acp",
            post(http::handle_post).with_state(http_state.clone()),
        )
        .route(
            "/acp",
            get(handle_get).with_state((http_state.clone(), ws_state)),
        )
        .route("/acp", delete(http::handle_delete).with_state(http_state))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}
