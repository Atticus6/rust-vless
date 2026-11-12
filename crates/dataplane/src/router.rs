use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_vless_common::util::now_ms;
use rust_vless_common::{parse_request, MAX_WS_MESSAGE};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::timeout;
use uuid::Uuid;

use crate::frame::{from_axum_ws, FrameIo};
use crate::session::{run_session, SessionContext, SessionMode};
use crate::state::DataPlaneState;

const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

pub fn router(state: Arc<DataPlaneState>) -> Router {
    Router::new()
        .route("/", get(ws_handler))
        .route("/ws", get(ws_handler))
        .route("/health", get(health))
        .with_state(state)
}

/// Only the VLESS WebSocket route, for the master whose `/` serves the SPA.
pub fn ws_only_router(state: Arc<DataPlaneState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn health(State(state): State<Arc<DataPlaneState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "service": "rust-vless-node",
        "version": env!("CARGO_PKG_VERSION"),
        "nodeId": state.node_id.to_string(),
        "users": state.users.len(),
        "activeConnections": state.active_connections(),
    }))
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DataPlaneState>>,
    headers: HeaderMap,
) -> Response {
    let relay = match relay_context(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };

    let Ok(permit) = state.sem.clone().try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "too many connections").into_response();
    };

    ws.max_message_size(MAX_WS_MESSAGE)
        .on_upgrade(move |socket| handle_socket(socket, state, permit, relay))
}

struct RelayContext {
    mode: SessionMode,
    report_traffic: bool,
    visited: Vec<String>,
    client_addr: String,
}

#[allow(clippy::result_large_err)]
async fn relay_context(
    state: &Arc<DataPlaneState>,
    headers: &HeaderMap,
) -> Result<RelayContext, Response> {
    let client_addr = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    let Some(token) = headers
        .get("x-rust-vless-link-token")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(RelayContext {
            mode: SessionMode::Entry,
            report_traffic: true,
            visited: Vec::new(),
            client_addr,
        });
    };

    let from_raw = headers
        .get("x-rust-vless-from")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing relay source").into_response())?;
    let from_id = Uuid::parse_str(from_raw)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid relay source").into_response())?;
    if state.inbound_match(token, from_id, "http").await.is_none() {
        return Err((StatusCode::UNAUTHORIZED, "invalid relay link token").into_response());
    }

    let visited: Vec<String> = headers
        .get("x-rust-vless-visited")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    if visited.contains(&state.node_id.to_string()) || visited.len() >= state.config.max_hops {
        return Err((StatusCode::FORBIDDEN, "relay loop or max hops exceeded").into_response());
    }

    Ok(RelayContext {
        mode: SessionMode::HttpRelay,
        report_traffic: false,
        visited,
        client_addr,
    })
}

async fn handle_socket(
    socket: axum::extract::ws::WebSocket,
    state: Arc<DataPlaneState>,
    _permit: OwnedSemaphorePermit,
    relay: RelayContext,
) {
    let _guard = state.active_guard();
    let FrameIo { mut reader, writer } = from_axum_ws(socket);

    let first = match timeout(HEADER_TIMEOUT, reader.recv()).await {
        Ok(Some(data)) => data,
        Ok(None) => return,
        Err(_) => {
            tracing::debug!("timed out waiting for VLESS header");
            return;
        }
    };
    if first.len() > MAX_WS_MESSAGE {
        return;
    }

    let request = match parse_request(&first, &state.users) {
        Ok(request) => request,
        Err(err) => {
            tracing::debug!(error = %err, "invalid VLESS request");
            return;
        }
    };

    let frame = FrameIo { reader, writer };
    let ctx = SessionContext {
        report_traffic: relay.report_traffic,
        visited: relay.visited,
        client_addr: relay.client_addr,
        session_id: Uuid::new_v4().to_string(),
        started_at_ms: now_ms(),
    };
    run_session(frame, request, first, ctx, state, relay.mode).await;
}
