//! JSON HTTP 接口与 SPA 托管。
//!
//! 每个处理器都是薄适配层：从 `Authorization` 头解析凭证，委托给
//! [`crate::service`]，再把结果转成 JSON。浏览器控制台本身通过同端口的
//! `WebAdmin` gRPC-Web 服务通信，因此两种传输共用同一套实现。

use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, header::HeaderName, HeaderMap};
use axum::response::IntoResponse;
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
use crate::subscription::{self, SubscriptionUser};
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

/// 从 `Authorization` 头解析当前调用方。
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

/// 构建主节点完整的 HTTP 路由。
///
/// 按顺序处理三类路径：
/// 1. `/api/*` JSON 接口（保留给脚本与向后兼容）；
/// 2. 浏览器发往 `/rustvless.rpc.v1.WebAdmin/*` 的 gRPC-Web 请求；
/// 3. 其余路径从 `web_dir` 提供静态文件，未命中时回退 `index.html` 以支持前端路由。
///
/// gRPC-Web 在进入 tonic 服务前由 [`GrpcWebLayer`] 解码；该层只应用在
/// gRPC 路由上，JSON 与静态请求不受影响。
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
        // 订阅入口信息（需要登录）；真正的订阅内容由 /sub/{token} 提供。
        .route("/api/subscription", get(subscription_meta))
        // 订阅接口用 token 鉴权，供 Clash / V2Ray 客户端直接拉取。
        .route("/sub/{token}", get(subscription_v2ray))
        .route("/sub/{token}/clash", get(subscription_clash))
        .route("/health", get(health))
        .with_state(state)
        .fallback_service(grpc)
        .layer(
            // gRPC-Web 客户端从响应头读取状态码，跨域调用时必须显式暴露。
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

/// 局部更新节点：所有字段可选，未提供的字段保持原值，符合 PATCH 语义。
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
    // 只有管理员可以代其他账号创建用户；普通用户创建的用户归自己。
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
        "version": rust_vless_common::version(),
    }))
}

/// 当前账号的订阅 token 与相对 URL；前端据此拼出可复制的完整地址。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionResponse {
    pub token: String,
    pub v2ray_url: String,
    pub clash_url: String,
}

async fn subscription_meta(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<SubscriptionResponse>, ApiError> {
    let admin = session(&state, &headers).await?;
    let (token, _) = service::subscription(&state, &admin).await?;
    Ok(Json(SubscriptionResponse {
        v2ray_url: format!("/sub/{token}"),
        clash_url: format!("/sub/{token}/clash"),
        token,
    }))
}

/// 根据订阅 token 与请求头生成订阅内容；token 无效时返回 404。
async fn subscription_body(
    state: &AppState,
    token: &str,
    headers: &HeaderMap,
    clash: bool,
) -> Result<String, ApiError> {
    let admin = state
        .db
        .admin_by_sub_token(token)
        .await?
        .ok_or(ApiError::NotFound)?;
    let users: Vec<SubscriptionUser> = state
        .db
        .enabled_proxy_users_for_owner(admin.id)
        .await?
        .into_iter()
        .map(|(id, name)| SubscriptionUser { id, name })
        .collect();
    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok());
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let endpoint =
        subscription::resolve_endpoint(&state.config.master_public_http_url, host, forwarded);
    Ok(if clash {
        subscription::clash_config(&endpoint, &users)
    } else {
        subscription::v2ray_subscription(&endpoint, &users)
    })
}

async fn subscription_v2ray(
    State(state): State<Arc<AppState>>,
    AxumPath(token): AxumPath<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let body = subscription_body(&state, &token, &headers, false).await?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body))
}

async fn subscription_clash(
    State(state): State<Arc<AppState>>,
    AxumPath(token): AxumPath<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let body = subscription_body(&state, &token, &headers, true).await?;
    Ok(([(header::CONTENT_TYPE, "text/yaml; charset=utf-8")], body))
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
    use crate::test_support::{app_state, ADMIN_EMAIL, ADMIN_PASSWORD};
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    async fn app() -> (Router, String) {
        let app = router(app_state().await);
        let token = login_token(&app, ADMIN_EMAIL, ADMIN_PASSWORD).await;
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

    #[tokio::test]
    async fn subscription_endpoints_serve_links() {
        use base64::Engine;

        let (app, token) = app().await;
        let (status, _) = call(
            &app,
            Method::POST,
            "/api/users",
            Some(&token),
            Some(serde_json::json!({ "name": "phone" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, meta) = call(&app, Method::GET, "/api/subscription", Some(&token), None).await;
        assert_eq!(status, StatusCode::OK);
        let v2ray_url = meta["v2rayUrl"].as_str().unwrap().to_string();
        let clash_url = meta["clashUrl"].as_str().unwrap().to_string();
        assert!(!meta["token"].as_str().unwrap().is_empty());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&v2ray_url)
                    .header("host", "master.example.com:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(body)
            .unwrap();
        let text = String::from_utf8(decoded).unwrap();
        assert!(text.starts_with("vless://"));
        assert!(text.contains("master.example.com:8080"));
        assert!(text.contains("#phone"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&clash_url)
                    .header("host", "master.example.com:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("type: vless"));
        assert!(text.contains("phone"));
    }

    /// 通过 axum 路由发送一个真实 gRPC-Web 帧请求，验证解码层、路径挂载与
    /// trailer 状态头都已接通；只测服务层无法发现传输层接线错误。
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

        // 没有 metadata 的调用必须以 UNAUTHENTICATED (16) 失败，
        // 而不是进入业务处理器。
        let nodes: rust_vless_proto::Empty = Default::default();
        let response = grpc_web_call(&app, "ListNodes", &nodes).await;
        let (status, _) = read_grpc_web(response).await;
        assert_eq!(status.as_deref(), Some("16"));
    }

    /// 解析 gRPC-Web 响应：消息帧标志为 0x00，trailer 帧标志为 0x80 并携带
    /// `grpc-status`；trailers-only 响应也可能把状态放在 HTTP 头里。
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

    /// 把一条 unary 消息编码成 gRPC-Web 请求，并通过路由发送（不监听端口）。
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
