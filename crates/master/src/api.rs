use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use rust_vless_common::util::{now_ms, random_hex};

use crate::auth::{
    bearer_token, current_admin, hash_password, parse_uuid, require_admin, verify_password,
};
use crate::db::{
    AccessLogPage, AccessLogQuery, AdminView, DeleteAdminOutcome, LinkView, NodeView,
    ProxyUserView, Role, TrafficRow,
};
use crate::error::ApiError;
use crate::state::AppState;

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

fn scope(admin: &crate::db::AdminRecord) -> Option<Uuid> {
    if admin.role == Role::Admin {
        None
    } else {
        Some(admin.id)
    }
}

fn validate_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }
    Ok(name.to_string())
}

fn validate_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(ApiError::BadRequest("invalid email".into()));
    };
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || email.len() > 254
        || email.chars().any(char::is_whitespace)
    {
        return Err(ApiError::BadRequest("invalid email".into()));
    }
    Ok(email)
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 || password.len() > 128 {
        return Err(ApiError::BadRequest(
            "password must be between 8 and 128 characters".into(),
        ));
    }
    Ok(())
}

pub fn router(state: Arc<AppState>) -> Router {
    let web_dir = state.config.web_dir.clone();
    let index = std::path::Path::new(&web_dir).join("index.html");

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
        .fallback_service(ServeDir::new(&web_dir).fallback(ServeFile::new(index)))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let email = req.email.trim().to_lowercase();
    let Some(admin) = state.db.find_admin_by_email(&email).await? else {
        return Err(ApiError::Unauthorized);
    };
    let permit = state
        .login_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::BadRequest("too many login attempts; retry later".into()))?;
    let hash = admin.password_hash.clone();
    let password = req.password;
    let verified = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        verify_password(&password, &hash)
    })
    .await
    .map_err(|_| ApiError::Internal)?;
    if !verified {
        return Err(ApiError::Unauthorized);
    }

    let token = random_hex(32);
    let expires_at = now_ms() + state.config.session_ttl_secs * 1000;
    state
        .db
        .create_session(admin.id, &token, expires_at)
        .await?;
    Ok(Json(LoginResponse {
        token,
        admin: AdminView::from(&admin),
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, ApiError> {
    if let Some(token) = bearer_token(&headers) {
        state.db.delete_session(&token).await?;
    }
    Ok(ok())
}

async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<AdminView>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    Ok(Json(AdminView::from(&admin)))
}

async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<crate::db::StatsView>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    let mut stats = state.db.stats(scope(&admin)).await?;
    stats.active_connections =
        state.active_connections().await + state.dataplane.active_connections();
    Ok(Json(stats))
}

async fn list_nodes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<NodeView>>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let nodes = state.db.list_nodes().await?;
    let live = state.live_snapshot().await;
    let views = nodes
        .iter()
        .map(|node| {
            let mut view = NodeView::from(node);
            if let Some(conn) = live.get(&node.id) {
                view.status = "online".into();
                view.version = conn.version.clone();
                view.active_connections = conn.active_connections;
                view.uptime_sec = if conn.uptime_sec > 0 {
                    conn.uptime_sec
                } else {
                    ((now_ms() - conn.connected_at).max(0) / 1000) as u64
                };
            }
            view
        })
        .collect();
    Ok(Json(views))
}

async fn create_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateNodeRequest>,
) -> Result<Json<CreateNodeResponse>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let name = validate_name(&req.name)?;
    let (node, token) = state.db.create_node(&name, false).await?;
    Ok(Json(CreateNodeResponse {
        node: NodeView::from(&node),
        token,
    }))
}

async fn delete_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    if state.db.get_node(id).await?.map(|node| node.is_master) == Some(true) {
        return Err(ApiError::BadRequest(
            "the master node cannot be deleted".into(),
        ));
    }
    if !state.db.delete_node(id).await? {
        return Err(ApiError::NotFound);
    }
    state.set_offline(id).await;
    state.publish_topology().await;
    Ok(ok())
}

async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProxyUserView>>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    let users = state.db.list_proxy_users(scope(&admin)).await?;
    Ok(Json(users.iter().map(ProxyUserView::from).collect()))
}

async fn create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<ProxyUserView>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    let name = validate_name(&req.name)?;
    let owner_id = match (admin.role, req.owner_id.as_deref()) {
        (Role::Admin, Some(raw)) => parse_uuid(raw)?,
        _ => admin.id,
    };
    let user = state.db.create_proxy_user(owner_id, &name).await?;
    state.publish_config().await;
    Ok(Json(ProxyUserView::from(&user)))
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<ProxyUserView>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    let Some(user) = state.db.get_proxy_user(id).await? else {
        return Err(ApiError::NotFound);
    };
    if admin.role != Role::Admin && user.owner_id != admin.id {
        return Err(ApiError::Forbidden);
    }
    let name = match req.name {
        Some(name) => Some(validate_name(&name)?),
        None => None,
    };
    state
        .db
        .update_proxy_user(id, name.as_deref(), req.enabled)
        .await?;
    state.publish_config().await;
    let updated = state
        .db
        .get_proxy_user(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ProxyUserView::from(&updated)))
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    let Some(user) = state.db.get_proxy_user(id).await? else {
        return Err(ApiError::NotFound);
    };
    if admin.role != Role::Admin && user.owner_id != admin.id {
        return Err(ApiError::Forbidden);
    }
    state.db.delete_proxy_user(id).await?;
    state.publish_config().await;
    Ok(ok())
}

async fn list_admins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminView>>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let admins = state.db.list_admins().await?;
    Ok(Json(admins.iter().map(AdminView::from).collect()))
}

async fn create_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateAdminRequest>,
) -> Result<Json<AdminView>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let email = validate_email(&req.email)?;
    validate_password(&req.password)?;
    let role = req.role.unwrap_or(Role::User);
    let hash = hash_password(&req.password)?;
    let admin = state.db.create_admin(&email, &hash, role).await?;
    Ok(Json(AdminView::from(&admin)))
}

async fn delete_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let current = require_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    if current.id == id {
        return Err(ApiError::BadRequest("cannot delete yourself".into()));
    }
    match state.db.delete_admin_checked(id).await? {
        DeleteAdminOutcome::Deleted => {}
        DeleteAdminOutcome::NotFound => return Err(ApiError::NotFound),
        DeleteAdminOutcome::LastAdmin => {
            return Err(ApiError::BadRequest("cannot delete the last admin".into()))
        }
    }
    state.publish_config().await;
    Ok(ok())
}

async fn traffic(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<TrafficRow>>, ApiError> {
    let admin = current_admin(&state.db, &headers).await?;
    Ok(Json(state.db.traffic_overview(scope(&admin)).await?))
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
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub http_url: Option<String>,
    pub rpc_url: Option<String>,
}

async fn update_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateNodeRequest>,
) -> Result<Json<NodeView>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    if state.db.get_node(id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let name = match req.name {
        Some(name) => Some(validate_name(&name)?),
        None => None,
    };
    state
        .db
        .update_node(
            id,
            name.as_deref(),
            req.http_url.as_deref(),
            req.rpc_url.as_deref(),
        )
        .await?;
    state.publish_topology().await;
    let node = state.db.get_node(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(NodeView::from(&node)))
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
    require_admin(&state.db, &headers).await?;
    let links = state.db.list_links().await?;
    Ok(Json(links.iter().map(LinkView::from).collect()))
}

async fn create_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<CreateLinkRequest>,
) -> Result<Json<LinkView>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let from = parse_uuid(&req.from_node_id)?;
    let to = parse_uuid(&req.to_node_id)?;
    let transport = req.transport.unwrap_or_else(|| "rpc".into());
    let link = state
        .db
        .create_link(from, to, &transport, req.priority.unwrap_or(0))
        .await?;
    state.publish_topology().await;
    Ok(Json(LinkView::from(&link)))
}

async fn delete_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let id = parse_uuid(&id)?;
    if !state.db.delete_link(id).await? {
        return Err(ApiError::NotFound);
    }
    state.publish_topology().await;
    Ok(ok())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyResponse {
    pub nodes: Vec<NodeView>,
    pub links: Vec<LinkView>,
}

async fn topology(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<TopologyResponse>, ApiError> {
    require_admin(&state.db, &headers).await?;
    let nodes = state.db.list_nodes().await?;
    let live = state.live_snapshot().await;
    let nodes = nodes
        .iter()
        .map(|node| {
            let mut view = NodeView::from(node);
            if let Some(conn) = live.get(&node.id) {
                view.status = "online".into();
                view.active_connections = conn.active_connections;
                view.uptime_sec = conn.uptime_sec;
            }
            view
        })
        .collect();
    let links = state.db.list_links().await?;
    Ok(Json(TopologyResponse {
        nodes,
        links: links.iter().map(LinkView::from).collect(),
    }))
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
    let admin = current_admin(&state.db, &headers).await?;
    let page = state
        .db
        .query_access_logs(
            scope(&admin),
            &AccessLogQuery {
                user_id: query.user_id,
                node_id: query.node_id,
                status: query.status,
                target: query.target,
                from: query.from,
                to: query.to,
                limit: query.limit.unwrap_or(50),
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
    let admin = current_admin(&state.db, &headers).await?;
    let window = query.window.unwrap_or(3600).clamp(60, 604_800);
    let live = state.live_snapshot().await;
    let mut topology = state.db.traffic_topology(window, &live).await?;
    if admin.role != Role::Admin {
        for link in &mut topology.links {
            link.up_bytes = 0;
            link.down_bytes = 0;
            link.sessions = 0;
        }
    }
    Ok(Json(topology))
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
}
