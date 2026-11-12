// `tonic::Status` is ~176 bytes; boxing it here would only move the cost into
// every trait method signature, and tonic's own generated code does the same.
#![allow(clippy::result_large_err)]

//! `WebAdmin` gRPC implementation used by the browser console.
//!
//! The browser speaks gRPC-Web (Connect wire format) and supplies its session
//! token as `authorization: Bearer <token>` metadata. This module only handles
//! protocol concerns: extracting credentials, converting protobuf messages to
//! and from the shared view types, and mapping [`ApiError`] to gRPC status
//! codes. All behaviour lives in [`crate::service`].

use std::sync::Arc;

use rust_vless_proto as pb;
use rust_vless_proto::web_admin_server::WebAdmin;
use tonic::{Request, Response, Status};

use crate::db::{
    AccessLogQuery, AccessLogView, AdminRecord, LinkView, NodeView, ProxyUserView, Role, StatsView,
    TopologyLinkView, TopologyNodeView, TrafficRow, TrafficTopology,
};
use crate::error::ApiError;
use crate::service;
use crate::state::AppState;

/// Tonic service adapter for the browser console.
///
/// Holds the shared application state; each call resolves the session token and
/// then delegates to [`crate::service`].
#[derive(Clone)]
pub struct WebAdminService {
    state: Arc<AppState>,
}

impl WebAdminService {
    /// Build a service around the master state. The server keeps this clone in
    /// every request, so the state handle is cheap (`Arc`).
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Extract the bearer token from request metadata.
    ///
    /// Only the standard `authorization` header is accepted; gRPC-Web clients
    /// cannot set `:authority`-style pseudo headers, and cookies are not used
    /// because sessions must survive cross-origin deployments.
    fn bearer<T>(request: &Request<T>) -> Result<String, Status> {
        request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| Status::unauthenticated("missing bearer token"))
    }

    async fn session<T>(&self, request: &Request<T>) -> Result<AdminRecord, Status> {
        let token = Self::bearer(request)?;
        service::admin_for_token(&self.state, &token)
            .await
            .map_err(status)
    }

    async fn admin<T>(&self, request: &Request<T>) -> Result<AdminRecord, Status> {
        let token = Self::bearer(request)?;
        service::admin_for_token_strict(&self.state, &token)
            .await
            .map_err(status)
    }
}

/// Map domain errors onto gRPC codes.
///
/// `Internal` deliberately loses its detail: the database error has already
/// been logged server side and must not leak schema or SQL text to clients.
fn status(err: ApiError) -> Status {
    match err {
        ApiError::BadRequest(message) => Status::invalid_argument(message),
        ApiError::Unauthorized => Status::unauthenticated("unauthorized"),
        ApiError::Forbidden => Status::permission_denied("forbidden"),
        ApiError::NotFound => Status::not_found("not found"),
        ApiError::Conflict(message) => Status::already_exists(message),
        ApiError::Internal => Status::internal("internal server error"),
    }
}

fn role_to_proto(role: Role) -> i32 {
    match role {
        Role::Admin => pb::WebRole::Admin as i32,
        Role::User => pb::WebRole::User as i32,
    }
}

fn role_from_proto(value: i32) -> Option<Role> {
    match pb::WebRole::try_from(value).ok()? {
        pb::WebRole::Admin => Some(Role::Admin),
        pb::WebRole::User => Some(Role::User),
        pb::WebRole::Unspecified => None,
    }
}

/// Persisted links store `"rpc"` or `"http"`; unknown values are surfaced as
/// `UNSPECIFIED` instead of being silently coerced.
fn transport_to_proto(value: &str) -> i32 {
    match value {
        "rpc" => pb::LinkTransport::Rpc as i32,
        "http" => pb::LinkTransport::Http as i32,
        _ => pb::LinkTransport::Unspecified as i32,
    }
}

/// Translate the wire enum into the persisted transport string.
///
/// `UNSPECIFIED` means "use the default" (rpc). Any other unmapped number is a
/// malformed request: falling back to rpc would silently create a link the
/// caller did not ask for, so it is rejected instead.
fn transport_from_proto(value: i32) -> Result<&'static str, Status> {
    match pb::LinkTransport::try_from(value) {
        Ok(pb::LinkTransport::Unspecified) | Ok(pb::LinkTransport::Rpc) => Ok("rpc"),
        Ok(pb::LinkTransport::Http) => Ok("http"),
        Err(_) => Err(Status::invalid_argument("unknown link transport")),
    }
}

fn admin_to_proto(admin: crate::db::AdminView) -> pb::AdminView {
    pb::AdminView {
        id: admin.id,
        email: admin.email,
        role: role_to_proto(admin.role),
        created_at: admin.created_at,
    }
}

fn node_to_proto(node: NodeView) -> pb::NodeView {
    pb::NodeView {
        id: node.id,
        name: node.name,
        status: node.status,
        version: node.version,
        http_url: node.http_url,
        rpc_url: node.rpc_url,
        is_master: node.is_master,
        last_seen: node.last_seen,
        created_at: node.created_at,
        active_connections: node.active_connections,
        uptime_sec: node.uptime_sec,
    }
}

fn user_to_proto(user: ProxyUserView) -> pb::ProxyUserView {
    pb::ProxyUserView {
        id: user.id,
        name: user.name,
        enabled: user.enabled,
        owner_id: user.owner_id,
        owner_email: user.owner_email,
        created_at: user.created_at,
        up_bytes: user.up_bytes,
        down_bytes: user.down_bytes,
    }
}

fn link_to_proto(link: LinkView) -> pb::LinkView {
    pb::LinkView {
        id: link.id,
        from_node_id: link.from_node_id,
        to_node_id: link.to_node_id,
        from_name: link.from_name,
        to_name: link.to_name,
        transport: transport_to_proto(&link.transport),
        priority: link.priority,
        enabled: link.enabled,
        created_at: link.created_at,
    }
}

fn stats_to_proto(stats: StatsView) -> pb::StatsView {
    pb::StatsView {
        nodes_total: stats.nodes_total,
        nodes_online: stats.nodes_online,
        users_total: stats.users_total,
        users_enabled: stats.users_enabled,
        admins_total: stats.admins_total,
        up_bytes: stats.up_bytes,
        down_bytes: stats.down_bytes,
        active_connections: stats.active_connections,
        links_total: stats.links_total,
    }
}

fn access_log_to_proto(log: AccessLogView) -> pb::AccessLogView {
    pb::AccessLogView {
        id: log.id,
        session_id: log.session_id,
        user_id: log.user_id,
        entry_node_id: log.entry_node_id,
        target: log.target,
        command: log.command,
        started_at: log.started_at,
        ended_at: log.ended_at,
        duration_ms: log.duration_ms,
        up_bytes: log.up_bytes,
        down_bytes: log.down_bytes,
        status: log.status,
        error: log.error,
        path: log.path,
        client_addr: log.client_addr,
    }
}

fn traffic_to_proto(topology: TrafficTopology) -> pb::TrafficTopologyView {
    pb::TrafficTopologyView {
        window_sec: topology.window_sec,
        nodes: topology
            .nodes
            .into_iter()
            .map(node_topology_to_proto)
            .collect(),
        links: topology
            .links
            .into_iter()
            .map(link_topology_to_proto)
            .collect(),
    }
}

fn node_topology_to_proto(node: TopologyNodeView) -> pb::TopologyNodeView {
    pb::TopologyNodeView {
        id: node.id,
        name: node.name,
        is_master: node.is_master,
        online: node.online,
        active_connections: node.active_connections,
    }
}

fn link_topology_to_proto(link: TopologyLinkView) -> pb::TopologyLinkView {
    pb::TopologyLinkView {
        id: link.id,
        from_node_id: link.from_node_id,
        to_node_id: link.to_node_id,
        transport: transport_to_proto(&link.transport),
        up_bytes: link.up_bytes,
        down_bytes: link.down_bytes,
        sessions: link.sessions,
    }
}

fn traffic_row_to_proto(row: TrafficRow) -> pb::TrafficRow {
    pb::TrafficRow {
        user_id: row.user_id,
        name: row.name,
        owner_email: row.owner_email,
        up_bytes: row.up_bytes,
        down_bytes: row.down_bytes,
    }
}

#[tonic::async_trait]
impl WebAdmin for WebAdminService {
    async fn login(
        &self,
        request: Request<pb::LoginRequest>,
    ) -> Result<Response<pb::LoginResponse>, Status> {
        let req = request.into_inner();
        let result = service::login(&self.state, &req.email, &req.password)
            .await
            .map_err(status)?;
        Ok(Response::new(pb::LoginResponse {
            token: result.token,
            admin: Some(admin_to_proto(result.admin)),
        }))
    }

    async fn logout(&self, request: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        let token = Self::bearer(&request)?;
        service::logout(&self.state, &token).await.map_err(status)?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn me(&self, request: Request<pb::Empty>) -> Result<Response<pb::AdminView>, Status> {
        let admin = self.session(&request).await?;
        Ok(Response::new(admin_to_proto((&admin).into())))
    }

    async fn get_stats(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::StatsView>, Status> {
        let admin = self.session(&request).await?;
        let stats = service::stats(&self.state, &admin).await.map_err(status)?;
        Ok(Response::new(stats_to_proto(stats)))
    }

    async fn list_nodes(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::NodeList>, Status> {
        self.admin(&request).await?;
        let nodes = service::list_nodes(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::NodeList {
            nodes: nodes.into_iter().map(node_to_proto).collect(),
        }))
    }

    async fn create_node(
        &self,
        request: Request<pb::CreateNodeRequest>,
    ) -> Result<Response<pb::CreateNodeResponse>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        let (node, token) = service::create_node(&self.state, &req.name)
            .await
            .map_err(status)?;
        Ok(Response::new(pb::CreateNodeResponse {
            node: Some(node_to_proto(node)),
            token,
        }))
    }

    async fn update_node(
        &self,
        request: Request<pb::UpdateNodeRequest>,
    ) -> Result<Response<pb::NodeView>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        let id = crate::auth::parse_uuid(&req.id).map_err(status)?;
        let node = service::update_node(
            &self.state,
            id,
            req.name.as_deref(),
            req.http_url.as_deref(),
            req.rpc_url.as_deref(),
        )
        .await
        .map_err(status)?;
        Ok(Response::new(node_to_proto(node)))
    }

    async fn delete_node(
        &self,
        request: Request<pb::DeleteNodeRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        service::delete_node(
            &self.state,
            crate::auth::parse_uuid(&req.id).map_err(status)?,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn list_users(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::UserList>, Status> {
        let admin = self.session(&request).await?;
        let users = service::list_users(&self.state, &admin)
            .await
            .map_err(status)?;
        Ok(Response::new(pb::UserList {
            users: users.into_iter().map(user_to_proto).collect(),
        }))
    }

    async fn create_user(
        &self,
        request: Request<pb::CreateUserRequest>,
    ) -> Result<Response<pb::ProxyUserView>, Status> {
        let admin = self.session(&request).await?;
        let req = request.into_inner();
        // Only admins may target another owner. For everyone else the field is
        // ignored before parsing so a malformed value cannot turn into an
        // error that the JSON transport would not produce.
        let owner_id = if admin.role == Role::Admin {
            match req.owner_id.as_deref() {
                Some(raw) => Some(crate::auth::parse_uuid(raw).map_err(status)?),
                None => None,
            }
        } else {
            None
        };
        let user = service::create_user(&self.state, &admin, &req.name, owner_id)
            .await
            .map_err(status)?;
        Ok(Response::new(user_to_proto(user)))
    }

    async fn update_user(
        &self,
        request: Request<pb::UpdateUserRequest>,
    ) -> Result<Response<pb::ProxyUserView>, Status> {
        let admin = self.session(&request).await?;
        let req = request.into_inner();
        let user = service::update_user(
            &self.state,
            &admin,
            crate::auth::parse_uuid(&req.id).map_err(status)?,
            req.name.as_deref(),
            req.enabled,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(user_to_proto(user)))
    }

    async fn delete_user(
        &self,
        request: Request<pb::DeleteUserRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        let admin = self.session(&request).await?;
        let req = request.into_inner();
        service::delete_user(
            &self.state,
            &admin,
            crate::auth::parse_uuid(&req.id).map_err(status)?,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn list_admins(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::AdminList>, Status> {
        self.admin(&request).await?;
        let admins = service::list_admins(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::AdminList {
            admins: admins.into_iter().map(admin_to_proto).collect(),
        }))
    }

    async fn create_admin(
        &self,
        request: Request<pb::CreateAdminRequest>,
    ) -> Result<Response<pb::AdminView>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        let role = req.role.and_then(role_from_proto);
        let admin = service::create_admin(&self.state, &req.email, &req.password, role)
            .await
            .map_err(status)?;
        Ok(Response::new(admin_to_proto(admin)))
    }

    async fn delete_admin(
        &self,
        request: Request<pb::DeleteAdminRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        let current = self.admin(&request).await?;
        let req = request.into_inner();
        service::delete_admin(
            &self.state,
            &current,
            crate::auth::parse_uuid(&req.id).map_err(status)?,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn list_links(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::LinkList>, Status> {
        self.admin(&request).await?;
        let links = service::list_links(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::LinkList {
            links: links.into_iter().map(link_to_proto).collect(),
        }))
    }

    async fn create_link(
        &self,
        request: Request<pb::CreateLinkRequest>,
    ) -> Result<Response<pb::LinkView>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        let link = service::create_link(
            &self.state,
            crate::auth::parse_uuid(&req.from_node_id).map_err(status)?,
            crate::auth::parse_uuid(&req.to_node_id).map_err(status)?,
            transport_from_proto(req.transport)?,
            req.priority,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(link_to_proto(link)))
    }

    async fn delete_link(
        &self,
        request: Request<pb::DeleteLinkRequest>,
    ) -> Result<Response<pb::Empty>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        service::delete_link(
            &self.state,
            crate::auth::parse_uuid(&req.id).map_err(status)?,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::Empty {}))
    }

    async fn get_topology(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::TopologyResponse>, Status> {
        self.admin(&request).await?;
        let topology = service::topology(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::TopologyResponse {
            nodes: topology.nodes.into_iter().map(node_to_proto).collect(),
            links: topology.links.into_iter().map(link_to_proto).collect(),
        }))
    }

    async fn list_access_logs(
        &self,
        request: Request<pb::AccessLogsRequest>,
    ) -> Result<Response<pb::AccessLogPage>, Status> {
        let admin = self.session(&request).await?;
        let req = request.into_inner();
        let page = service::access_logs(
            &self.state,
            &admin,
            AccessLogQuery {
                user_id: req.user_id,
                node_id: req.node_id,
                status: req.status,
                target: req.target,
                from: req.from,
                to: req.to,
                limit: req.limit.unwrap_or(0),
                offset: req.offset.unwrap_or(0),
            },
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::AccessLogPage {
            items: page.items.into_iter().map(access_log_to_proto).collect(),
            total: page.total,
            limit: page.limit,
            offset: page.offset,
        }))
    }

    async fn get_traffic_topology(
        &self,
        request: Request<pb::TrafficTopologyRequest>,
    ) -> Result<Response<pb::TrafficTopologyView>, Status> {
        let admin = self.session(&request).await?;
        let req = request.into_inner();
        let topology = service::traffic_topology(&self.state, &admin, req.window)
            .await
            .map_err(status)?;
        Ok(Response::new(traffic_to_proto(topology)))
    }

    async fn get_traffic_overview(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::TrafficOverview>, Status> {
        let admin = self.session(&request).await?;
        let rows = service::traffic_overview(&self.state, &admin)
            .await
            .map_err(status)?;
        Ok(Response::new(pb::TrafficOverview {
            rows: rows.into_iter().map(traffic_row_to_proto).collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{app_state, ADMIN_EMAIL, ADMIN_PASSWORD};

    async fn state() -> (Arc<AppState>, WebAdminService) {
        let state = app_state().await;
        let service = WebAdminService::new(state.clone());
        (state, service)
    }

    fn authed<T>(message: T, token: &str) -> Request<T> {
        let mut request = Request::new(message);
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        request
    }

    async fn login(service: &WebAdminService) -> String {
        let response = service
            .login(Request::new(pb::LoginRequest {
                email: ADMIN_EMAIL.into(),
                password: ADMIN_PASSWORD.into(),
            }))
            .await
            .unwrap();
        let token = response.into_inner().token;
        assert!(!token.is_empty());
        token
    }

    #[tokio::test]
    async fn login_then_list_nodes() {
        let (_state, service) = state().await;
        let token = login(&service).await;
        let nodes = service
            .list_nodes(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].is_master);
        assert_eq!(nodes[0].name, "master");
    }

    #[tokio::test]
    async fn rejects_missing_or_invalid_token() {
        let (_state, service) = state().await;
        let err = service
            .list_nodes(Request::new(pb::Empty {}))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        let err = service
            .list_nodes(authed(pb::Empty {}, "not-a-session"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn login_with_wrong_password_fails() {
        let (_state, service) = state().await;
        let err = service
            .login(Request::new(pb::LoginRequest {
                email: "admin@example.com".into(),
                password: "wrong-password".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn node_and_link_round_trip() {
        let (_state, service) = state().await;
        let token = login(&service).await;

        let created = service
            .create_node(authed(
                pb::CreateNodeRequest {
                    name: "edge".into(),
                },
                &token,
            ))
            .await
            .unwrap()
            .into_inner();
        let node = created.node.unwrap();
        assert!(!created.token.is_empty());

        let topology = service
            .get_topology(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner();
        let master_id = topology
            .nodes
            .iter()
            .find(|node| node.is_master)
            .unwrap()
            .id
            .clone();

        service
            .create_link(authed(
                pb::CreateLinkRequest {
                    from_node_id: master_id.clone(),
                    to_node_id: node.id.clone(),
                    transport: pb::LinkTransport::Rpc as i32,
                    priority: 3,
                },
                &token,
            ))
            .await
            .unwrap();

        let links = service
            .list_links(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner()
            .links;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].transport, pb::LinkTransport::Rpc as i32);
        assert_eq!(links[0].priority, 3);

        // A link back to the master would close a cycle and must be rejected
        // with AlreadyExists rather than panicking or being silently dropped.
        let err = service
            .create_link(authed(
                pb::CreateLinkRequest {
                    from_node_id: node.id.clone(),
                    to_node_id: master_id.clone(),
                    transport: pb::LinkTransport::Rpc as i32,
                    priority: 0,
                },
                &token,
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::AlreadyExists);

        // Unknown enum values must fail validation instead of silently
        // creating an RPC link the caller never asked for.
        let err = service
            .create_link(authed(
                pb::CreateLinkRequest {
                    from_node_id: node.id,
                    to_node_id: master_id,
                    transport: 99,
                    priority: 0,
                },
                &token,
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn regular_user_cannot_manage_nodes() {
        let (_state, service) = state().await;
        let token = login(&service).await;
        service
            .create_admin(authed(
                pb::CreateAdminRequest {
                    email: "user@example.com".into(),
                    password: "password123".into(),
                    role: Some(pb::WebRole::User as i32),
                },
                &token,
            ))
            .await
            .unwrap();

        let user_token = service
            .login(Request::new(pb::LoginRequest {
                email: "user@example.com".into(),
                password: "password123".into(),
            }))
            .await
            .unwrap()
            .into_inner()
            .token;

        let err = service
            .list_nodes(authed(pb::Empty {}, &user_token))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::PermissionDenied);

        // Regular users may still read their own (empty) user list.
        let users = service
            .list_users(authed(pb::Empty {}, &user_token))
            .await
            .unwrap()
            .into_inner()
            .users;
        assert!(users.is_empty());
    }

    #[tokio::test]
    async fn access_log_page_is_clamped() {
        let (_state, service) = state().await;
        let token = login(&service).await;
        let page = service
            .list_access_logs(authed(
                pb::AccessLogsRequest {
                    limit: Some(100_000),
                    offset: Some(-5),
                    ..Default::default()
                },
                &token,
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(page.limit, service::MAX_ACCESS_LOG_LIMIT);
        assert_eq!(page.offset, 0);
        assert!(page.items.is_empty());
    }
}
