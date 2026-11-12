// `tonic::Status` 约 176 字节；在这里装箱只会把成本转移到每个 trait 方法
// 的签名上，tonic 自己生成的代码也采用同样的豁免。
#![allow(clippy::result_large_err)]

//! 浏览器控制台使用的 `WebAdmin` gRPC 实现。
//!
//! 浏览器通过 gRPC-Web（Connect 客户端）通信，并在 metadata 中携带
//! `authorization: Bearer <token>`。本模块只处理协议层：提取凭证、
//! 在 protobuf 与共享视图类型之间转换、把 [`ApiError`] 映射为 gRPC 状态码；
//! 全部业务行为都在 [`crate::service`]。

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

/// 浏览器控制台使用的 tonic 服务适配器。
///
/// 持有共享应用状态；每次调用先解析会话 token，再委托给 [`crate::service`]。
#[derive(Clone)]
pub struct WebAdminService {
    state: Arc<AppState>,
}

impl WebAdminService {
    /// 围绕主节点状态构建服务。服务器会在每个请求中克隆该服务，
    /// 因此状态句柄必须廉价（`Arc`）。
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// 从请求 metadata 中提取 Bearer token。
    ///
    /// 只接受标准 `authorization` 头；gRPC-Web 客户端无法设置
    /// `:authority` 之类的伪头，并且不使用 cookie，因为会话需要跨域部署。
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

/// 把业务错误映射为 gRPC 状态码。
///
/// `Internal` 故意丢弃细节：数据库错误已在服务端记录日志，不能把表结构或
/// SQL 文本泄露给客户端。
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

/// 持久化的连线类型是 `"rpc"` 或 `"http"`；未知值以 `UNSPECIFIED` 呈现，
/// 而不是静默改写。
fn transport_to_proto(value: &str) -> i32 {
    match value {
        "rpc" => pb::LinkTransport::Rpc as i32,
        "http" => pb::LinkTransport::Http as i32,
        _ => pb::LinkTransport::Unspecified as i32,
    }
}

/// 把线上枚举转换为持久化字符串。
///
/// `UNSPECIFIED` 表示「使用默认值」（rpc）；其他无法映射的数值属于非法请求，
/// 回退成 rpc 会悄悄创建调用方并未要求的连线，因此直接拒绝。
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

    async fn get_setup_status(
        &self,
        _request: Request<pb::Empty>,
    ) -> Result<Response<pb::SetupStatus>, Status> {
        let status = service::setup_status(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::SetupStatus {
            installed: status.installed,
            version: status.version,
        }))
    }

    async fn setup(
        &self,
        request: Request<pb::SetupRequest>,
    ) -> Result<Response<pb::LoginResponse>, Status> {
        let req = request.into_inner();
        let result = service::setup(
            &self.state,
            &req.email,
            &req.password,
            &req.master_name,
            &req.setup_token,
        )
        .await
        .map_err(status)?;
        Ok(Response::new(pb::LoginResponse {
            token: result.token,
            admin: Some(admin_to_proto(result.admin)),
        }))
    }

    async fn get_version_info(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::VersionInfo>, Status> {
        self.admin(&request).await?;
        let info = service::version_info(&self.state).await.map_err(status)?;
        Ok(Response::new(pb::VersionInfo {
            master_version: info.master_version,
            latest_version: info.latest_version,
            nodes: info
                .nodes
                .into_iter()
                .map(|node| pb::NodeVersionInfo {
                    node_id: node.node_id,
                    name: node.name,
                    version: node.version,
                    online: node.online,
                })
                .collect(),
        }))
    }

    async fn update_all_nodes(
        &self,
        request: Request<pb::UpdateNodesRequest>,
    ) -> Result<Response<pb::UpdateNodesResponse>, Status> {
        self.admin(&request).await?;
        let req = request.into_inner();
        let (requested, online) = service::update_all_nodes(&self.state, Some(&req.version))
            .await
            .map_err(status)?;
        Ok(Response::new(pb::UpdateNodesResponse { requested, online }))
    }

    async fn get_subscription(
        &self,
        request: Request<pb::Empty>,
    ) -> Result<Response<pb::SubscriptionInfo>, Status> {
        let admin = self.session(&request).await?;
        let (token, users) = service::subscription(&self.state, &admin)
            .await
            .map_err(status)?;
        Ok(Response::new(pb::SubscriptionInfo {
            v2ray_url: format!("/sub/{token}"),
            clash_url: format!("/sub/{token}/clash"),
            token,
            users: users.into_iter().map(user_to_proto).collect(),
        }))
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
        // 只有管理员能指定其他 owner。其他调用方在解析前就忽略该字段，
        // 使非法值不会产生 JSON 传输层不会出现的错误。
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

        // 指回主节点的连线会成环，必须返回 AlreadyExists，
        // 而不是 panic 或静默丢弃。
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

        // 未知枚举值必须校验失败，不能悄悄创建一条调用方没要求的 RPC 连线。
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

        // 普通用户仍然可以读取自己（空）的用户列表。
        let users = service
            .list_users(authed(pb::Empty {}, &user_token))
            .await
            .unwrap()
            .into_inner()
            .users;
        assert!(users.is_empty());

        // 批量升级会向节点投递运维命令，必须同样要求 admin 角色：
        // 页面隐藏按钮只是 UI 约束，不能替代服务端校验。
        let err = service
            .update_all_nodes(authed(
                pb::UpdateNodesRequest {
                    version: String::new(),
                },
                &user_token,
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
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

    #[tokio::test]
    async fn setup_token_is_locked_after_repeated_failures() {
        let state = crate::test_support::empty_app_state().await;
        let service = WebAdminService::new(state);
        for _ in 0..service::MAX_SETUP_FAILURES {
            let err = service
                .setup(Request::new(pb::SetupRequest {
                    email: "root@example.com".into(),
                    password: crate::test_support::ADMIN_PASSWORD.into(),
                    master_name: String::new(),
                    setup_token: "wrong".into(),
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }
        // 达到上限后即使安装码正确也被拒绝，必须重启主节点换码。
        let err = service
            .setup(Request::new(pb::SetupRequest {
                email: "root@example.com".into(),
                password: crate::test_support::ADMIN_PASSWORD.into(),
                master_name: String::new(),
                setup_token: crate::test_support::SETUP_TOKEN.into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[tokio::test]
    async fn setup_flow_creates_first_admin_once() {
        let state = crate::test_support::empty_app_state().await;
        let service = WebAdminService::new(state);

        let status = service
            .get_setup_status(Request::new(pb::Empty {}))
            .await
            .unwrap()
            .into_inner();
        assert!(!status.installed);
        assert_eq!(status.version, rust_vless_common::version());

        // 错误安装码必须被拒绝，且不能创建管理员。
        let err = service
            .setup(Request::new(pb::SetupRequest {
                email: "root@example.com".into(),
                password: crate::test_support::ADMIN_PASSWORD.into(),
                master_name: String::new(),
                setup_token: "wrong-token".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        let status = service
            .get_setup_status(Request::new(pb::Empty {}))
            .await
            .unwrap()
            .into_inner();
        assert!(!status.installed);

        let login = service
            .setup(Request::new(pb::SetupRequest {
                email: "root@example.com".into(),
                password: crate::test_support::ADMIN_PASSWORD.into(),
                master_name: "edge-master".into(),
                setup_token: crate::test_support::SETUP_TOKEN.into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!login.token.is_empty());
        assert_eq!(login.admin.unwrap().email, "root@example.com");

        let status = service
            .get_setup_status(Request::new(pb::Empty {}))
            .await
            .unwrap()
            .into_inner();
        assert!(status.installed);

        // 第二次安装必须被拒绝。
        let err = service
            .setup(Request::new(pb::SetupRequest {
                email: "other@example.com".into(),
                password: crate::test_support::ADMIN_PASSWORD.into(),
                master_name: String::new(),
                setup_token: crate::test_support::SETUP_TOKEN.into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::AlreadyExists);

        // 安装返回的 token 可以立即访问管理接口，主节点名称也已更新。
        let nodes = service
            .list_nodes(authed(pb::Empty {}, &login.token))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "edge-master");
    }

    #[tokio::test]
    async fn version_info_and_update_all_nodes() {
        let (_state, service) = state().await;
        let token = login(&service).await;

        let info = service
            .get_version_info(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(info.master_version, rust_vless_common::version());
        assert_eq!(info.latest_version, rust_vless_common::version());
        assert_eq!(info.nodes.len(), 1);
        assert!(!info.nodes[0].online, "测试环境没有真实控制流");

        let result = service
            .update_all_nodes(authed(
                pb::UpdateNodesRequest {
                    version: String::new(),
                },
                &token,
            ))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(result.requested, 0);
        assert_eq!(result.online, 0);
    }

    #[tokio::test]
    async fn subscription_token_is_stable_and_scoped() {
        let (_state, service) = state().await;
        let token = login(&service).await;

        let first = service
            .get_subscription(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner();
        assert!(!first.token.is_empty());
        assert!(first.v2ray_url.ends_with(&first.token));
        assert!(first.clash_url.ends_with(&format!("{}/clash", first.token)));
        assert!(first.users.is_empty());

        let second = service
            .get_subscription(authed(pb::Empty {}, &token))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(second.token, first.token);
    }
}
