//! JSON REST 与 `WebAdmin` RPC 共用的业务逻辑。
//!
//! 两种传输只在鉴权与编码方式上不同；所有数据库读写、输入校验、拓扑/配置
//! 下发都集中在这里，确保同一操作在 REST 与 RPC 下的行为不会分叉。

use uuid::Uuid;

use rust_vless_common::util::{now_ms, random_hex};

use crate::auth::{hash_password, verify_password};
use crate::db::{
    AccessLogPage, AccessLogQuery, AdminRecord, AdminView, DeleteAdminOutcome, LinkView, NodeView,
    ProxyUserView, Role, StatsView, TrafficRow, TrafficTopology,
};
use crate::error::ApiError;
use crate::state::AppState;

/// 调用方未指定 `limit` 时的默认分页大小。
pub const DEFAULT_ACCESS_LOG_LIMIT: i64 = 50;
/// 单次查询的硬上限，防止扫描无界历史。
pub const MAX_ACCESS_LOG_LIMIT: i64 = 200;
/// 默认流量聚合窗口（一小时）。
pub const DEFAULT_TRAFFIC_WINDOW_SECS: i64 = 3_600;
/// 允许的最小与最大流量窗口。
const MIN_TRAFFIC_WINDOW_SECS: i64 = 60;
const MAX_TRAFFIC_WINDOW_SECS: i64 = 604_800;

/// 密码登录成功后的结果。
pub struct LoginResult {
    pub token: String,
    pub admin: AdminView,
}

/// 管理员与普通账号都能读数据，但只有管理员能看到整个安装；
/// 普通账号的数据范围仅限自己名下的代理用户。
pub fn scope(admin: &AdminRecord) -> Option<Uuid> {
    if admin.role == Role::Admin {
        None
    } else {
        Some(admin.id)
    }
}

/// 安装码连续失败多少次后作废本次安装。
pub const MAX_SETUP_FAILURES: u32 = 5;

/// 去掉名称首尾空白，并校验 1–64 字符边界。
///
/// 返回的是 trim 后的值，调用方应持久化返回值。空名称或超过 64 个 Unicode
/// 标量值会返回 `BadRequest`；按字符而不是字节计数，避免中文名称与界面
/// 长度限制不一致。
pub fn validate_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }
    Ok(name.to_string())
}

/// 规范化并校验账号邮箱。
///
/// 结果会 trim 并转小写，保证查询大小写不敏感。有效地址恰好包含一个 `@`、
/// 本地部分非空、域名至少有一个点且没有空标签、不含空白，总长度不超过
/// 254 字节。
pub fn validate_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(ApiError::BadRequest("invalid email".into()));
    };
    // 拒绝空的域名标签和第二个 `@`："a@.com"、"a@com." 与 "a@b@c.com"
    // 都能被字符串解析，但无法真正投递。
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || domain.contains('@')
        || domain.split('.').any(str::is_empty)
        || email.len() > 254
        || email.chars().any(char::is_whitespace)
    {
        return Err(ApiError::BadRequest("invalid email".into()));
    }
    Ok(email)
}

/// 校验账号密码长度（8–128 字节）。
///
/// 按字节计数，与 Argon2 的输入一致；该边界同时限制哈希成本并拒绝过短的密码。
pub fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 || password.len() > 128 {
        return Err(ApiError::BadRequest(
            "password must be between 8 and 128 characters".into(),
        ));
    }
    Ok(())
}

/// 校验凭证并创建会话 token。
///
/// 密码哈希在 blocking 线程池执行：Argon2id 故意消耗 CPU，不能阻塞异步运行时。
/// 并发校验由信号量限制，攻击者无法用登录请求耗尽工作线程。
pub async fn login(state: &AppState, email: &str, password: &str) -> Result<LoginResult, ApiError> {
    let email = email.trim().to_lowercase();
    let Some(admin) = state.db.find_admin_by_email(&email).await? else {
        return Err(ApiError::Unauthorized);
    };
    let permit = state
        .login_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::BadRequest("too many login attempts; retry later".into()))?;
    let hash = admin.password_hash.clone();
    let password = password.to_string();
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
    Ok(LoginResult {
        token,
        admin: AdminView::from(&admin),
    })
}

/// 注销会话 token。未知 token 直接忽略，因此登出是幂等的。
pub async fn logout(state: &AppState, token: &str) -> Result<(), ApiError> {
    state.db.delete_session(token).await?;
    Ok(())
}

/// 把会话 token 解析为账号；过期或未知 token 映射为 `Unauthorized`。
pub async fn admin_for_token(state: &AppState, token: &str) -> Result<AdminRecord, ApiError> {
    state
        .db
        .session_admin(token)
        .await?
        .ok_or(ApiError::Unauthorized)
}

/// 与 [`admin_for_token`] 相同，但额外要求 admin 角色。
pub async fn admin_for_token_strict(
    state: &AppState,
    token: &str,
) -> Result<AdminRecord, ApiError> {
    let admin = admin_for_token(state, token).await?;
    if admin.role != Role::Admin {
        return Err(ApiError::Forbidden);
    }
    Ok(admin)
}

/// 按调用方范围聚合计数：管理员看整个安装，普通账号只看自己名下的用户与
/// 节点全局计数。
pub async fn stats(state: &AppState, admin: &AdminRecord) -> Result<StatsView, ApiError> {
    let mut stats = state.db.stats(scope(admin)).await?;
    stats.active_connections =
        state.active_connections().await + state.dataplane.active_connections();
    Ok(stats)
}

/// 合并数据库节点与实时连接状态，生成节点视图。
async fn node_views(state: &AppState) -> Result<Vec<NodeView>, ApiError> {
    let nodes = state.db.list_nodes().await?;
    let live = state.live_snapshot().await;
    Ok(nodes
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
        .collect())
}

/// 列出所有节点。调用方必须已校验 admin 角色；本函数只负责组装数据，
/// 不做任何授权判断。
pub async fn list_nodes(state: &AppState) -> Result<Vec<NodeView>, ApiError> {
    node_views(state).await
}

/// 创建子节点并返回一次性注册 token。
///
/// 传输层已限制为管理员。token 只在本次响应中返回，之后无法再次读取，
/// 调用方必须立即展示或保存。
pub async fn create_node(state: &AppState, name: &str) -> Result<(NodeView, String), ApiError> {
    let name = validate_name(name)?;
    let (node, token) = state.db.create_node(&name, false).await?;
    Ok((NodeView::from(&node), token))
}

/// 更新节点名称与公网地址。
///
/// 节点不存在返回 `NotFound`。地址变更会立即下发给所有在线节点，
/// 因为它们决定中转跳下一次往哪里拨号。
pub async fn update_node(
    state: &AppState,
    id: Uuid,
    name: Option<&str>,
    http_url: Option<&str>,
    rpc_url: Option<&str>,
) -> Result<NodeView, ApiError> {
    if state.db.get_node(id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let name = match name {
        Some(name) => Some(validate_name(name)?),
        None => None,
    };
    state
        .db
        .update_node(id, name.as_deref(), http_url, rpc_url)
        .await?;
    // 地址变化会影响其他节点拨号的目标，立即下发新的拓扑快照。
    state.publish_topology().await;
    let node = state.db.get_node(id).await?.ok_or(ApiError::NotFound)?;
    Ok(NodeView::from(&node))
}

/// 删除子节点，并从实时映射与拓扑中移除。
///
/// 主节点自身受保护：删除它会留下无法解析的连线。
pub async fn delete_node(state: &AppState, id: Uuid) -> Result<(), ApiError> {
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
    Ok(())
}

/// 列出调用方可见的代理用户（管理员看全部，普通用户只看自己名下）。
pub async fn list_users(
    state: &AppState,
    admin: &AdminRecord,
) -> Result<Vec<ProxyUserView>, ApiError> {
    let users = state.db.list_proxy_users(scope(admin)).await?;
    Ok(users.iter().map(ProxyUserView::from).collect())
}

/// 创建代理用户，并把新配置下发给所有在线节点。
///
/// `owner_id` 只对管理员生效，普通用户创建的用户总是归自己所有；
/// 指定的 owner 不存在返回 `NotFound`。
pub async fn create_user(
    state: &AppState,
    admin: &AdminRecord,
    name: &str,
    owner_id: Option<Uuid>,
) -> Result<ProxyUserView, ApiError> {
    let name = validate_name(name)?;
    // 管理员可以代其他账号创建；其他调用方只能创建自己的用户。
    let owner_id = match (admin.role, owner_id) {
        (Role::Admin, Some(owner_id)) => owner_id,
        _ => admin.id,
    };
    let user = state.db.create_proxy_user(owner_id, &name).await?;
    state.publish_config().await;
    Ok(ProxyUserView::from(&user))
}

/// 修改代理用户名称或启用状态。
///
/// 普通用户只能修改自己的用户（否则返回 `Forbidden`）；更新后的配置会
/// 下发到所有节点，使禁用立即生效。
pub async fn update_user(
    state: &AppState,
    admin: &AdminRecord,
    id: Uuid,
    name: Option<&str>,
    enabled: Option<bool>,
) -> Result<ProxyUserView, ApiError> {
    let Some(user) = state.db.get_proxy_user(id).await? else {
        return Err(ApiError::NotFound);
    };
    if admin.role != Role::Admin && user.owner_id != admin.id {
        return Err(ApiError::Forbidden);
    }
    let name = match name {
        Some(name) => Some(validate_name(name)?),
        None => None,
    };
    state
        .db
        .update_proxy_user(id, name.as_deref(), enabled)
        .await?;
    state.publish_config().await;
    let updated = state
        .db
        .get_proxy_user(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(ProxyUserView::from(&updated))
}

/// 访问日志清理失败时写入数据库队列表，由保留任务/下次启动重试。
async fn purge_user_logs(state: &AppState, user_ids: &[String]) {
    if user_ids.is_empty() {
        return;
    }
    if let Err(err) = state.access.remove_users(user_ids).await {
        tracing::warn!(error = %err, "清理访问日志文件失败，已加入重试队列");
        if let Err(err) = state.db.queue_log_purges(user_ids).await {
            tracing::warn!(error = %err, "登记访问日志清理队列失败");
        }
    }
}

/// 删除代理用户及其流量历史。
///
/// 归属规则与 [`update_user`] 相同。
pub async fn delete_user(state: &AppState, admin: &AdminRecord, id: Uuid) -> Result<(), ApiError> {
    let Some(user) = state.db.get_proxy_user(id).await? else {
        return Err(ApiError::NotFound);
    };
    if admin.role != Role::Admin && user.owner_id != admin.id {
        return Err(ApiError::Forbidden);
    }
    // 先标记删除，阻止排队中的事件在数据库删除后写回日志；失败再解除标记。
    let user_ids = [id.to_string()];
    state.access.mark_users_deleted(&user_ids).await;
    if let Err(err) = state.db.delete_proxy_user(id).await {
        state.access.unmark_users_deleted(&user_ids).await;
        return Err(err.into());
    }
    purge_user_logs(state, &user_ids).await;
    state.publish_config().await;
    Ok(())
}

/// 列出管理账号。传输层已限制为管理员。
pub async fn list_admins(state: &AppState) -> Result<Vec<AdminView>, ApiError> {
    let admins = state.db.list_admins().await?;
    Ok(admins.iter().map(AdminView::from).collect())
}

/// 校验邮箱/密码后创建管理账号。
///
/// 传输层已限制为管理员；密码哈希在这里完成，保证两种传输使用相同参数。
/// 邮箱重复映射为 `Conflict`。
pub async fn create_admin(
    state: &AppState,
    email: &str,
    password: &str,
    role: Option<Role>,
) -> Result<AdminView, ApiError> {
    let email = validate_email(email)?;
    validate_password(password)?;
    let hash = hash_password(password)?;
    let admin = state
        .db
        .create_admin(&email, &hash, role.unwrap_or(Role::User))
        .await?;
    Ok(AdminView::from(&admin))
}

/// 删除另一个管理账号。
///
/// 拒绝删除自己以及最后一个管理员（数据库内原子校验），
/// 避免把整个安装锁死。
pub async fn delete_admin(
    state: &AppState,
    current: &AdminRecord,
    id: Uuid,
) -> Result<(), ApiError> {
    if current.id == id {
        return Err(ApiError::BadRequest("cannot delete yourself".into()));
    }
    // 账号删除会级联删除其代理用户。先标记当前用户名下的用户，阻止延迟事件
    // 在删除后写回；事务内再返回实际删除的 ID 用于清理文件。
    let marked_users: Vec<String> = state
        .db
        .list_proxy_users(Some(id))
        .await?
        .iter()
        .map(|user| user.id.to_string())
        .collect();
    state.access.mark_users_deleted(&marked_users).await;
    let deleted_users = match state.db.delete_admin_checked(id).await {
        Ok(DeleteAdminOutcome::Deleted(users)) => users,
        Ok(DeleteAdminOutcome::NotFound) => {
            state.access.unmark_users_deleted(&marked_users).await;
            return Err(ApiError::NotFound);
        }
        Ok(DeleteAdminOutcome::LastAdmin) => {
            state.access.unmark_users_deleted(&marked_users).await;
            return Err(ApiError::BadRequest("cannot delete the last admin".into()));
        }
        Err(err) => {
            state.access.unmark_users_deleted(&marked_users).await;
            return Err(err.into());
        }
    };
    let deleted_users: Vec<String> = deleted_users.iter().map(Uuid::to_string).collect();
    // 事务内级联删除的用户可能不在事务前的快照里（并发新建），补标记后再清理，
    // 否则延迟事件仍能通过守卫把日志写回。
    let unmarked: Vec<String> = deleted_users
        .iter()
        .filter(|id| !marked_users.contains(id))
        .cloned()
        .collect();
    state.access.mark_users_deleted(&unmarked).await;
    purge_user_logs(state, &deleted_users).await;
    state.publish_config().await;
    Ok(())
}

/// 列出中继连线。传输层已限制为管理员。
pub async fn list_links(state: &AppState) -> Result<Vec<LinkView>, ApiError> {
    let links = state.db.list_links().await?;
    Ok(links.iter().map(LinkView::from).collect())
}

/// 创建有向中继连线并下发新拓扑。
///
/// 数据库会拒绝自连、重复、未知传输方式与成环的边，分别映射为
/// `BadRequest`/`Conflict`。生成的连线 token 只通过拓扑快照下发给两端，
/// 本函数不会返回它。
pub async fn create_link(
    state: &AppState,
    from: Uuid,
    to: Uuid,
    transport: &str,
    priority: i64,
) -> Result<LinkView, ApiError> {
    let link = state.db.create_link(from, to, transport, priority).await?;
    state.publish_topology().await;
    Ok(LinkView::from(&link))
}

/// 删除中继连线（数据库层面幂等；未知 id 返回 `NotFound`），并重新下发拓扑。
pub async fn delete_link(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    if !state.db.delete_link(id).await? {
        return Err(ApiError::NotFound);
    }
    state.publish_topology().await;
    Ok(())
}

/// 连线页面使用的节点 + 连线快照。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyResponse {
    pub nodes: Vec<NodeView>,
    pub links: Vec<LinkView>,
}

/// 节点与连线快照。传输层已限制为管理员。
pub async fn topology(state: &AppState) -> Result<TopologyResponse, ApiError> {
    // `node_views` 已合并实时状态；拓扑页必须与节点列表使用同一视图，
    // 否则两个页面会显示不一致的状态。
    Ok(TopologyResponse {
        nodes: node_views(state).await?,
        links: list_links(state).await?,
    })
}

/// 查询一页访问日志。
///
/// `limit = 0` 使用 [`DEFAULT_ACCESS_LOG_LIMIT`]，更大的值会被限制到
/// [`MAX_ACCESS_LOG_LIMIT`]，避免单次请求扫描无界历史；负 offset 按 0 处理。
/// 普通用户只能看到自己名下用户的日志。
pub async fn access_logs(
    state: &AppState,
    admin: &AdminRecord,
    query: AccessLogQuery,
) -> Result<AccessLogPage, ApiError> {
    let limit = match query.limit {
        0 => DEFAULT_ACCESS_LOG_LIMIT,
        limit => limit.clamp(1, MAX_ACCESS_LOG_LIMIT),
    };
    let offset = query.offset.max(0);
    // 普通账号只能看到自己名下用户的日志；在进入 DuckDB 前解析成用户白名单，
    // 避免把权限判断写进 SQL 文件扫描。
    let visible_users = match scope(admin) {
        None => None,
        Some(owner) => Some(
            state
                .db
                .list_proxy_users(Some(owner))
                .await?
                .into_iter()
                .map(|user| user.id.to_string())
                .collect::<Vec<_>>(),
        ),
    };
    state
        .access
        .query(
            &AccessLogQuery {
                limit,
                offset,
                ..query
            },
            visible_users,
        )
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "查询访问日志失败");
            ApiError::Internal
        })
}

/// 构建滚动窗口内的流量拓扑。
///
/// `window` 默认一小时，并限制在 [60 秒, 7 天]。连线计数是全局的，
/// 普通账号看到的值会被清零，避免泄露其他租户的流量。
pub async fn traffic_topology(
    state: &AppState,
    admin: &AdminRecord,
    window: Option<i64>,
) -> Result<TrafficTopology, ApiError> {
    let window = window
        .unwrap_or(DEFAULT_TRAFFIC_WINDOW_SECS)
        .clamp(MIN_TRAFFIC_WINDOW_SECS, MAX_TRAFFIC_WINDOW_SECS);
    let live = state.live_snapshot().await;
    let mut topology = state.db.traffic_topology(window, &live).await?;
    // 连线计数是全局的；普通用户不应看到不属于自己的流量。
    if admin.role != Role::Admin {
        for link in &mut topology.links {
            link.up_bytes = 0;
            link.down_bytes = 0;
            link.sessions = 0;
        }
    }
    Ok(topology)
}

/// 调用方范围内的每用户流量汇总。
pub async fn traffic_overview(
    state: &AppState,
    admin: &AdminRecord,
) -> Result<Vec<TrafficRow>, ApiError> {
    Ok(state.db.traffic_overview(scope(admin)).await?)
}

/// 首次安装状态：是否已有管理员，以及主节点版本。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusView {
    pub installed: bool,
    pub version: String,
}

/// 单个节点的版本快照。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVersionView {
    pub node_id: String,
    pub name: String,
    pub version: String,
    pub online: bool,
}

/// 主节点与所有节点的版本汇总。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfoView {
    pub master_version: String,
    pub latest_version: String,
    pub nodes: Vec<NodeVersionView>,
}

/// 查询是否已完成首次安装。
pub async fn setup_status(state: &AppState) -> Result<SetupStatusView, ApiError> {
    Ok(SetupStatusView {
        installed: state.db.count_admins().await? > 0,
        version: rust_vless_common::version().to_string(),
    })
}

/// 常量时间比较两个字节串，避免通过响应时间逐字符试探安装码。
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// 完成首次安装：创建第一个管理员并直接返回登录会话。
///
/// 仅在没有任何管理员时可用；并发提交由数据库事务保证只有一个成功，
/// 其余请求得到 `Conflict`。`master_name` 非空时同时更新拓扑中的主节点名称。
/// 安装码校验使用常量时间比较，并在连续失败达到上限后作废本次安装码。
pub async fn setup(
    state: &AppState,
    email: &str,
    password: &str,
    master_name: &str,
    setup_token: &str,
) -> Result<LoginResult, ApiError> {
    // 先快速拒绝已安装请求，避免匿名用户反复触发密码哈希消耗 CPU。
    if state.db.count_admins().await? > 0 {
        return Err(ApiError::Conflict("already installed".into()));
    }
    let Some(expected) = state.setup_token.as_deref() else {
        return Err(ApiError::Conflict("already installed".into()));
    };
    // 上限检查、安装码比较与失败计数在同一把锁内完成：否则并发请求都能
    // 读到未达上限，从而突破尝试次数限制。
    {
        let mut failures = state.setup_failures.lock().await;
        if *failures >= MAX_SETUP_FAILURES {
            return Err(ApiError::Forbidden);
        }
        if !constant_time_eq(setup_token.as_bytes(), expected.as_bytes()) {
            *failures += 1;
            if *failures >= MAX_SETUP_FAILURES {
                tracing::error!(
                    attempts = *failures,
                    "安装码连续校验失败过多，已作废本次安装码；请重启主节点获取新码"
                );
            }
            return Err(ApiError::Unauthorized);
        }
    }

    // 所有输入先校验完再落库，避免非法 master_name 造成「已安装但拿不到 token」。
    let email = validate_email(email)?;
    validate_password(password)?;
    let master_name = master_name.trim();
    let master_name = if master_name.is_empty() {
        None
    } else {
        Some(validate_name(master_name)?)
    };

    // 密码哈希是 CPU 密集操作：限制并发并放到 blocking 线程池。
    let permit = state
        .login_sem
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::BadRequest("too many setup attempts; retry later".into()))?;
    let password = password.to_string();
    let hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        hash_password(&password)
    })
    .await
    .map_err(|_| ApiError::Internal)??;

    let Some(admin) = state
        .db
        .create_first_admin_if_empty(&email, &hash, Role::Admin)
        .await?
    else {
        return Err(ApiError::Conflict("already installed".into()));
    };

    if let Some(name) = master_name {
        state
            .db
            .update_node(state.master_node_id, Some(&name), None, None)
            .await?;
        state.publish_topology().await;
    }

    let token = random_hex(32);
    let expires_at = now_ms() + state.config.session_ttl_secs * 1000;
    state
        .db
        .create_session(admin.id, &token, expires_at)
        .await?;
    Ok(LoginResult {
        token,
        admin: AdminView::from(&admin),
    })
}

/// 汇总主节点与各节点版本。
///
/// `latest_version` 来自 `RUST_VLESS_LATEST_VERSION`，未配置时等于主节点版本；
/// 节点是否在线取自实时控制通道。
pub async fn version_info(state: &AppState) -> Result<VersionInfoView, ApiError> {
    let master_version = rust_vless_common::version().to_string();
    let latest_version = state
        .config
        .latest_version
        .clone()
        .unwrap_or_else(|| master_version.clone());
    let nodes = list_nodes(state).await?;
    Ok(VersionInfoView {
        master_version,
        latest_version,
        nodes: nodes
            .into_iter()
            .map(|node| NodeVersionView {
                node_id: node.id,
                name: node.name,
                version: node.version,
                online: node.status == "online",
            })
            .collect(),
    })
}

/// 向所有在线节点广播升级指令，返回（在线节点数，成功投递数）。
///
/// 目标版本为空时使用对外公布的 `latest_version`（而不是主节点当前版本），
/// 这样「版本」页展示的最新版本与实际下发目标一致。升级是尽力而为：
/// 离线节点不会收到指令；投递成功也不等于升级成功，最终版本以节点重连后上报为准。
pub async fn update_all_nodes(
    state: &AppState,
    target_version: Option<&str>,
) -> Result<(u32, u32), ApiError> {
    let target = target_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            state
                .config
                .latest_version
                .clone()
                .unwrap_or_else(|| rust_vless_common::version().to_string())
        });

    let nodes: Vec<(
        Uuid,
        tokio::sync::mpsc::Sender<rust_vless_proto::MasterMessage>,
    )> = state
        .live
        .read()
        .await
        .iter()
        .map(|(id, node)| (*id, node.tx.clone()))
        .collect();
    let requested = nodes.len() as u32;
    // 并发投递并为每个节点单独计时：串行等待会让受阻节点把总耗时累加到
    // 节点数量 × 1 秒，客户端超时后剩余节点收不到指令。
    let sends = nodes.into_iter().map(|(id, tx)| {
        let target = target.clone();
        async move {
            let message = rust_vless_proto::MasterMessage {
                payload: Some(rust_vless_proto::master_message::Payload::Upgrade(
                    rust_vless_proto::Upgrade {
                        version: target.clone(),
                    },
                )),
            };
            match tokio::time::timeout(std::time::Duration::from_secs(1), tx.send(message)).await {
                Ok(Ok(())) => true,
                _ => {
                    tracing::warn!(%id, version = %target, "升级指令投递失败");
                    false
                }
            }
        }
    });
    let delivered = futures_util::future::join_all(sends)
        .await
        .into_iter()
        .filter(|delivered| *delivered)
        .count() as u32;
    Ok((requested, delivered))
}

/// 读取当前账号的订阅 token 与其名下的代理用户列表。
///
/// 返回账号名下的**全部**用户（含已禁用），供面板展示启用状态；
/// `/sub/{token}` 只把已启用的用户写进订阅内容。两者是同一份数据的
/// 「管理视图」与「订阅视图」，面板据此显示哪些用户当前不会下发。
pub async fn subscription(
    state: &AppState,
    admin: &AdminRecord,
) -> Result<(String, Vec<ProxyUserView>), ApiError> {
    let token = state.db.get_or_create_sub_token(admin.id).await?;
    let users = state
        .db
        .list_proxy_users(Some(admin.id))
        .await?
        .iter()
        .map(ProxyUserView::from)
        .collect();
    Ok((token, users))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{admin_record, app_state, ADMIN_PASSWORD};
    use uuid::Uuid;

    fn uuid(raw: &str) -> Uuid {
        Uuid::parse_str(raw).unwrap()
    }

    #[test]
    fn name_boundaries() {
        assert_eq!(validate_name("  edge  ").unwrap(), "edge");
        assert!(matches!(validate_name("   "), Err(ApiError::BadRequest(_))));
        assert!(validate_name(&"a".repeat(64)).is_ok());
        assert!(matches!(
            validate_name(&"a".repeat(65)),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn email_normalizes_and_rejects_malformed() {
        assert_eq!(
            validate_email(" Admin@Example.COM ").unwrap(),
            "admin@example.com"
        );
        for bad in [
            "",
            "no-at",
            "a@b",
            "a@.com",
            "a@com.",
            "a@b..com",
            "a@b@c.com",
            "@example.com",
            "a b@example.com",
        ] {
            assert!(
                matches!(validate_email(bad), Err(ApiError::BadRequest(_))),
                "{bad} should be rejected"
            );
        }
        let long = format!("{}@example.com", "a".repeat(250));
        assert!(validate_email(&long).is_err());
    }

    #[test]
    fn password_boundaries() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"a".repeat(128)).is_ok());
        assert!(validate_password(&"a".repeat(129)).is_err());
    }

    #[tokio::test]
    async fn access_log_limit_defaults_and_clamps() {
        let state = app_state().await;
        let admin = admin_record(&state).await;
        let query = |limit, offset| AccessLogQuery {
            limit,
            offset,
            ..Default::default()
        };

        let page = access_logs(&state, &admin, query(0, -5)).await.unwrap();
        assert_eq!(page.limit, DEFAULT_ACCESS_LOG_LIMIT);
        assert_eq!(page.offset, 0);

        let page = access_logs(&state, &admin, query(100_000, 0))
            .await
            .unwrap();
        assert_eq!(page.limit, MAX_ACCESS_LOG_LIMIT);
    }

    #[tokio::test]
    async fn traffic_window_defaults_and_clamps() {
        let state = app_state().await;
        let admin = admin_record(&state).await;

        assert_eq!(
            traffic_topology(&state, &admin, None)
                .await
                .unwrap()
                .window_sec,
            DEFAULT_TRAFFIC_WINDOW_SECS
        );
        assert_eq!(
            traffic_topology(&state, &admin, Some(1))
                .await
                .unwrap()
                .window_sec,
            MIN_TRAFFIC_WINDOW_SECS
        );
        assert_eq!(
            traffic_topology(&state, &admin, Some(10_000_000))
                .await
                .unwrap()
                .window_sec,
            MAX_TRAFFIC_WINDOW_SECS
        );
    }

    #[tokio::test]
    async fn users_are_scoped_to_their_owner() {
        let state = app_state().await;
        let alice = create_admin(
            &state,
            "alice@example.com",
            ADMIN_PASSWORD,
            Some(Role::User),
        )
        .await
        .unwrap();
        let bob = create_admin(&state, "bob@example.com", ADMIN_PASSWORD, Some(Role::User))
            .await
            .unwrap();
        let alice = state.db.get_admin(uuid(&alice.id)).await.unwrap().unwrap();
        let bob = state.db.get_admin(uuid(&bob.id)).await.unwrap().unwrap();

        let alice_user = create_user(&state, &alice, "alice-phone", None)
            .await
            .unwrap();
        let bob_user = create_user(&state, &bob, "bob-phone", None).await.unwrap();

        let alice_users = list_users(&state, &alice).await.unwrap();
        assert_eq!(alice_users.len(), 1);
        assert_eq!(alice_users[0].id, alice_user.id);

        // 通过修改接口跨账号访问必须被拒绝。
        let alice_user_id = uuid(&alice_user.id);
        assert!(matches!(
            update_user(&state, &bob, alice_user_id, Some("stolen"), None).await,
            Err(ApiError::Forbidden)
        ));
        assert!(matches!(
            delete_user(&state, &bob, alice_user_id).await,
            Err(ApiError::Forbidden)
        ));

        // 所有者仍可修改自己的用户。
        let updated = update_user(&state, &bob, uuid(&bob_user.id), Some("bob-2"), Some(false))
            .await
            .unwrap();
        assert_eq!(updated.name, "bob-2");
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn update_all_nodes_delivers_to_live_nodes() {
        use crate::state::LiveNode;

        let state = app_state().await;
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let node_id = Uuid::new_v4();
        state.live.write().await.insert(
            node_id,
            LiveNode {
                conn_id: Uuid::new_v4(),
                version: "0.0.0".into(),
                active_connections: 0,
                uptime_sec: 0,
                connected_at: 0,
                tx,
            },
        );

        let (requested, delivered) = update_all_nodes(&state, Some("9.9.9")).await.unwrap();
        assert_eq!((requested, delivered), (1, 1));

        let message = rx.recv().await.unwrap();
        match message.payload {
            Some(rust_vless_proto::master_message::Payload::Upgrade(upgrade)) => {
                assert_eq!(upgrade.version, "9.9.9");
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_all_nodes_counts_failed_delivery() {
        use crate::state::LiveNode;

        let state = app_state().await;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        // 接收端已关闭，发送必然失败，用来覆盖失败计数分支。
        drop(rx);
        state.live.write().await.insert(
            Uuid::new_v4(),
            LiveNode {
                conn_id: Uuid::new_v4(),
                version: "0.0.0".into(),
                active_connections: 0,
                uptime_sec: 0,
                connected_at: 0,
                tx,
            },
        );

        let (requested, delivered) = update_all_nodes(&state, None).await.unwrap();
        assert_eq!((requested, delivered), (1, 0));
    }
}
