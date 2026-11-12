//! JSON HTTP surface plus SPA hosting.
//!
//! Every handler is a thin adapter: it extracts credentials from the
//! `Authorization` header, delegates to [`crate::service`], and maps the
//! result to JSON. The browser console itself uses the `WebAdmin` gRPC-Web
//! service mounted on the same port, so both surfaces share one
//! implementation.

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header::HeaderName, HeaderMap};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use rust_vless_proto::web_admin_server::WebAdminServer;

use crate::auth::{bearer_token, parse_uuid};
use crate::db::{
    AccessLogPage, AccessLogQuery, AdminView, LinkView, NodeView, ProxyUserView, Role,
};
use crate::error::ApiError;
use crate::service::{self, TopologyResponse};
use crate::state::AppState;
use crate::web_rpc::WebAdminService;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub token: String,
    pub admin: AdminView,
}

#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeResponse {
    pub node: NodeView,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    pub name: String,
    pub owner_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub email: String,
    pub password: String,
    pub role: Option<Role>,
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

fn ok() -> Json<OkResponse> {
    Json(OkResponse { ok: true })
}

/// Resolve the caller from the `Authorization` header.
async fn session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::db::AdminRecord, ApiError> {
    let token = bearer_token(headers).ok_or(ApiError::Unauthorized)?;
    service::admin_for_token(state, &token).await
}

async fn admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::db::AdminRecord, ApiError> {
    let token = bearer_token(headers).ok_or(ApiError::Unauthorized)?;
    service::admin_for_token_strict(state, &token).await
}

/// Build the complete master HTTP router.
///
/// The router serves three kinds of paths, in this order:
/// 1. `/api/*` JSON endpoints (kept for scripts and backwards compatibility);
/// 2. `/rustvless.rpc.v1.WebAdmin/*` gRPC-Web requests from the browser;
/// 3. everything else from `web_dir`, falling back to `index.html` for client
///    side routes.
///
/// gRPC-Web is decoded by [`GrpcWebLayer`] before reaching the tonic service.
/// The layer is applied only to the gRPC routes, so JSON and static requests
/// are untouched.
pub fn router(state: Arc<AppState>) -> Router {
    let web_dir = state.config.web_dir.clone();
    let index = Path::new(&web_dir).join("index.html");
    let spa = ServeDir::new(&web_dir).fallback(ServeFile::new(index));

    let grpc =
        tonic::service::Routes::new(WebAdminServer::new(WebAdminService::new(state.clone())))
            .into_axum_router()
            .layer(GrpcWebLayer::new())
            .fallback_service(spa);

    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/stats", get(stats))
        .route("/api/nodes", get(list_nodes).post(create_node))
        .route("/api/nodes/{id}", patch(update_node).delete(delete_node))
        .route("/api/links", get(list_links).post(create_link))
        .route("/api/links/{id}", delete(delete_link))
        .route("/api/topology", get(topology))
        .route("/api/access-logs", get(access_logs))
        .route("/api/traffic/topology", get(traffic_topology))
        .route("/ws", get(master_ws))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", patch(update_user).delete(delete_user))
        .route("/api/admins", get(list_admins).post(create_admin))
        .route("/api/admins/{id}", delete(delete_admin))
        .route("/api/traffic", get(traffic))
        .route("/health", get(health))
        .with_state(state)
        .fallback_service(grpc)
        .layer(
            // gRPC-Web clients read the status from response headers, so they
            // must be explicitly exposed to cross-origin callers.
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .expose_headers([
                    HeaderName::from_static("grpc-status"),
                    HeaderName::from_static("grpc-message"),
                    HeaderName::from_static("grpc-status-details-bin"),
                ]),
        )
        .layer(TraceLayer::new_for_http())
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let result = service::login(&state, &req.email, &req.password).await?;
    Ok(Json(LoginResponse {
        token: result.token,
        admin: result.admin,
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, ApiError> {
    if let Some(token) = bearer_token(&headers) {
        service::logout(&state, &token).await?;
    }
    Ok(ok())
}

async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<AdminView>, ApiError> {
    let admin = session(&state, &headers).await?;
    Ok(Json(AdminView::from(&admin)))
}

async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<crate::db::StatsView>, ApiError> {
    let admin = session(&state, &headers).await?;
    Ok(Json(service::stats(&state, &admin).await?))
}

async fn list_nodes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<NodeView>>, ApiError> {
    admin_session(&state, &headers).await?;
    Ok(Json(service::list_nodes(&state).await?))
}

async fn create_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateNodeRequest>,
) -> Result<Json<CreateNodeResponse>, ApiError> {
    admin_session(&state, &headers).await?;
    let (node, token) = service::create_node(&state, &req.name).await?;
    Ok(Json(CreateNodeResponse { node, token }))
}

/// Partial node update: every field is optional and omitted fields keep their
/// stored value, matching PATCH semantics.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub http_url: Option<String>,
    pub rpc_url: Option<String>,
}

async fn update_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateNodeRequest>,
) -> Result<Json<NodeView>, ApiError> {
    admin_session(&state, &headers).await?;
    let id = parse_uuid(&id)?;
    let node = service::update_node(
        &state,
        id,
        req.name.as_deref(),
        req.http_url.as_deref(),
        req.rpc_url.as_deref(),
    )
    .await?;
    Ok(Json(node))
}

async fn delete_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<OkResponse>, ApiError> {
    admin_session(&state, &headers).await?;
    service::delete_node(&state, parse_uuid(&id)?).await?;
    Ok(ok())
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProxyUserView>>, ApiError> {
    let admin = session(&state, &headers).await?;
    Ok(Json(service::list_users(&state, &admin).await?))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<ProxyUserView>, ApiError> {
    // Admin-only owner override; regular users create users they own.
    let admin = session(&state, &headers).await?;
    let owner_id = match req.owner_id.as_deref() {
        Some(raw) if admin.role == Role::Admin => Some(parse_uuid(raw)?),
        _ => None,
    };
    Ok(Json(
        service::create_user(&state, &admin, &req.name, owner_id).await?,
    ))
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<ProxyUserView>, ApiError> {
    let admin = session(&state, &headers).await?;
    let updated = service::update_user(
        &state,
        &admin,
        parse_uuid(&id)?,
        req.name.as_deref(),
        req.enabled,
    )
    .await?;
    Ok(Json(updated))
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let admin = session(&state, &headers).await?;
    service::delete_user(&state, &admin, parse_uuid(&id)?).await?;
    Ok(ok())
}

async fn list_admins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminView>>, ApiError> {
    admin_session(&state, &headers).await?;
    Ok(Json(service::list_admins(&state).await?))
}

async fn create_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateAdminRequest>,
) -> Result<Json<AdminView>, ApiError> {
    admin_session(&state, &headers).await?;
    Ok(Json(
        service::create_admin(&state, &req.email, &req.password, req.role).await?,
    ))
}

async fn delete_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let current = admin_session(&state, &headers).await?;
    service::delete_admin(&state, &current, parse_uuid(&id)?).await?;
    Ok(ok())
}

async fn traffic(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::db::TrafficRow>>, ApiError> {
    let admin = session(&state, &headers).await?;
    Ok(Json(service::traffic_overview(&state, &admin).await?))
}

async fn master_ws(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    rust_vless_dataplane::router::ws_handler(ws, State(state.dataplane.clone()), headers).await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "service": "rust-vless-master",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLinkRequest {
    pub from_node_id: String,
    pub to_node_id: String,
    pub transport: Option<String>,
    pub priority: Option<i64>,
}

async fn list_links(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<LinkView>>, ApiError> {
    admin_session(&state, &headers).await?;
    Ok(Json(service::list_links(&state).await?))
}

async fn create_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateLinkRequest>,
) -> Result<Json<LinkView>, ApiError> {
    admin_session(&state, &headers).await?;
    let link = service::create_link(
        &state,
        parse_uuid(&req.from_node_id)?,
        parse_uuid(&req.to_node_id)?,
        req.transport.as_deref().unwrap_or("rpc"),
        req.priority.unwrap_or(0),
    )
    .await?;
    Ok(Json(link))
}

async fn delete_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<OkResponse>, ApiError> {
    admin_session(&state, &headers).await?;
    service::delete_link(&state, parse_uuid(&id)?).await?;
    Ok(ok())
}

async fn topology(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TopologyResponse>, ApiError> {
    admin_session(&state, &headers).await?;
    Ok(Json(service::topology(&state).await?))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccessLogsQuery {
    pub user_id: Option<String>,
    pub node_id: Option<String>,
    pub status: Option<String>,
    pub target: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn access_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AccessLogsQuery>,
) -> Result<Json<AccessLogPage>, ApiError> {
    let admin = session(&state, &headers).await?;
    let page = service::access_logs(
        &state,
        &admin,
        AccessLogQuery {
            user_id: query.user_id,
            node_id: query.node_id,
            status: query.status,
            target: query.target,
            from: query.from,
            to: query.to,
            limit: query.limit.unwrap_or(0),
            offset: query.offset.unwrap_or(0),
        },
    )
    .await?;
    Ok(Json(page))
}

#[derive(Debug, Deserialize, Default)]
pub struct TrafficTopologyQuery {
    pub window: Option<i64>,
}

async fn traffic_topology(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<TrafficTopologyQuery>,
) -> Result<Json<crate::db::TrafficTopology>, ApiError> {
    let admin = session(&state, &headers).await?;
    Ok(Json(
        service::traffic_topology(&state, &admin, query.window).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MasterConfig;
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use clap::Parser;
    use rust_vless_common::UserRegistry;
    use rust_vless_dataplane::{DataPlaneConfig, DataPlaneState};
    use tower::ServiceExt;

    async fn app() -> (Router, String) {
        let db = crate::db::Db::open(":memory:").await.unwrap();
        let hash = crate::auth::hash_password("password123").unwrap();
        db.create_admin("admin@example.com", &hash, Role::Admin)
            .await
            .unwrap();
        let config = MasterConfig::parse_from(["rust-vless-master"]);
        let master_id = uuid::Uuid::new_v4();
        db.ensure_master_node(
            master_id,
            "master",
            "http://127.0.0.1:8080",
            "http://127.0.0.1:50051",
        )
        .await
        .unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let plane = DataPlaneState::new(
            master_id,
            "master".into(),
            std::sync::Arc::new(UserRegistry::new()),
            16,
            tx,
            DataPlaneConfig {
                max_hops: 8,
                allow_direct_fallback: true,
            },
        );
        let state = AppState::new(db, config, master_id, plane);
        state.refresh_dataplane().await.unwrap();
        let app = router(state);
        let token = login_token(&app, "admin@example.com", "password123").await;
        (app, token)
    }

    async fn login_token(app: &Router, email: &str, password: &str) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "email": email, "password": password }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        value["token"].as_str().unwrap().to_string()
    }

    async fn call(
        app: &Router,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let body = match body {
            Some(value) => {
                builder = builder.header("content-type", "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = app
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn login_and_auth_guards() {
        let (app, token) = app().await;
        let (status, _) = call(&app, Method::GET, "/api/nodes", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "admin@example.com",
                            "password": "wrong-password"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!token.is_empty());
    }

    #[tokio::test]
    async fn node_user_link_flow() {
        let (app, token) = app().await;

        let (status, node) = call(
            &app,
            Method::POST,
            "/api/nodes",
            Some(&token),
            Some(serde_json::json!({ "name": "edge" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let node_id = node["node"]["id"].as_str().unwrap().to_string();

        let (status, user) = call(
            &app,
            Method::POST,
            "/api/users",
            Some(&token),
            Some(serde_json::json!({ "name": "phone" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(user["name"], "phone");

        let (status, _) = call(
            &app,
            Method::POST,
            "/api/links",
            Some(&token),
            Some(serde_json::json!({
                "fromNodeId": node_id,
                "toNodeId": node_id,
                "transport": "rpc"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn link_cycle_is_rejected() {
        let (app, token) = app().await;
        let (_, topology) = call(&app, Method::GET, "/api/topology", Some(&token), None).await;
        let master_id = topology["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["isMaster"].as_bool().unwrap())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, node) = call(
            &app,
            Method::POST,
            "/api/nodes",
            Some(&token),
            Some(serde_json::json!({ "name": "edge" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let node_id = node["node"]["id"].as_str().unwrap().to_string();

        let (status, _) = call(
            &app,
            Method::POST,
            "/api/links",
            Some(&token),
            Some(serde_json::json!({
                "fromNodeId": master_id,
                "toNodeId": node_id,
                "transport": "rpc"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, body) = call(
            &app,
            Method::POST,
            "/api/links",
            Some(&token),
            Some(serde_json::json!({
                "fromNodeId": node_id,
                "toNodeId": master_id,
                "transport": "rpc"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
    }

    #[tokio::test]
    async fn access_logs_limit_is_clamped() {
        let (app, token) = app().await;
        let (status, page) = call(
            &app,
            Method::GET,
            "/api/access-logs?limit=100000",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["limit"], service::MAX_ACCESS_LOG_LIMIT);
        assert_eq!(page["offset"], 0);
    }

    /// Sends a real gRPC-Web framed request through the axum router to prove
    /// the layer, path mount, and trailing status headers are wired correctly.
    /// A unit test on the service alone would not catch a broken transport.
    #[tokio::test]
    async fn grpc_web_login_is_served_on_http_port() {
        use prost::Message;

        let (app, _) = app().await;
        let message = rust_vless_proto::LoginRequest {
            email: "admin@example.com".into(),
            password: "password123".into(),
        };
        let response = grpc_web_call(&app, "Login", &message).await;
        assert_eq!(response.status(), StatusCode::OK);
        let (status, payload) = read_grpc_web(response).await;
        assert_eq!(status.as_deref(), Some("0"), "unexpected status {status:?}");
        let login = rust_vless_proto::LoginResponse::decode(payload.as_slice()).unwrap();
        assert!(!login.token.is_empty());
        assert_eq!(login.admin.unwrap().email, "admin@example.com");

        // A call without metadata must fail with UNAUTHENTICATED (16) instead
        // of reaching the handler.
        let nodes: rust_vless_proto::Empty = Default::default();
        let response = grpc_web_call(&app, "ListNodes", &nodes).await;
        let (status, _) = read_grpc_web(response).await;
        assert_eq!(status.as_deref(), Some("16"));
    }

    /// Parse a gRPC-Web response: message frames are flag 0x00, the trailer
    /// frame is flag 0x80 and carries `grpc-status`. Trailers-only responses
    /// may carry the status in the HTTP headers instead.
    async fn read_grpc_web(response: axum::response::Response) -> (Option<String>, Vec<u8>) {
        let header_status = response
            .headers()
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let mut status = header_status;
        let mut message = Vec::new();
        let mut offset = 0;
        while offset + 5 <= bytes.len() {
            let flags = bytes[offset];
            let len =
                u32::from_be_bytes(bytes[offset + 1..offset + 5].try_into().unwrap()) as usize;
            let end = offset + 5 + len;
            assert!(end <= bytes.len(), "truncated grpc-web frame");
            let payload = &bytes[offset + 5..end];
            if flags & 0x80 != 0 {
                for line in String::from_utf8_lossy(payload).lines() {
                    if let Some(value) = line.strip_prefix("grpc-status:") {
                        status = Some(value.trim().to_string());
                    }
                }
            } else {
                message = payload.to_vec();
            }
            offset = end;
        }
        (status, message)
    }

    /// Encode one unary message as a gRPC-Web request and send it through the
    /// router without a listening socket.
    async fn grpc_web_call<M: prost::Message>(
        app: &Router,
        method: &str,
        message: &M,
    ) -> axum::response::Response {
        let mut frame = Vec::with_capacity(message.encoded_len() + 5);
        frame.push(0);
        frame.extend_from_slice(&(message.encoded_len() as u32).to_be_bytes());
        message.encode(&mut frame).unwrap();
        app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/rustvless.rpc.v1.WebAdmin/{method}"))
                    .header("content-type", "application/grpc-web+proto")
                    .header("x-grpc-web", "1")
                    .body(Body::from(frame))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
}
