//! Business logic shared by the JSON REST surface and the `WebAdmin` RPC
//! surface.
//!
//! The two transports only differ in how they authenticate and encode data.
//! Everything that touches the database, validates input, or publishes
//! topology/config updates lives here so a change cannot make the REST and RPC
//! views of the same operation drift apart.

use uuid::Uuid;

use rust_vless_common::util::{now_ms, random_hex};

use crate::auth::{hash_password, verify_password};
use crate::db::{
    AccessLogPage, AccessLogQuery, AdminRecord, AdminView, DeleteAdminOutcome, LinkView, NodeView,
    ProxyUserView, Role, StatsView, TrafficRow, TrafficTopology,
};
use crate::error::ApiError;
use crate::state::AppState;

/// Default page size for access log queries when the caller omits `limit`.
pub const DEFAULT_ACCESS_LOG_LIMIT: i64 = 50;
/// Hard cap so a single request cannot scan an unbounded slice of history.
pub const MAX_ACCESS_LOG_LIMIT: i64 = 200;
/// Default traffic aggregation window (one hour).
pub const DEFAULT_TRAFFIC_WINDOW_SECS: i64 = 3_600;
/// Smallest and largest accepted traffic windows.
const MIN_TRAFFIC_WINDOW_SECS: i64 = 60;
const MAX_TRAFFIC_WINDOW_SECS: i64 = 604_800;

/// Result of a successful password login.
pub struct LoginResult {
    pub token: String,
    pub admin: AdminView,
}

/// Both admin and regular accounts may read data, but only admins see the
/// whole installation. Ordinary users are scoped to their own proxy users.
pub fn scope(admin: &AdminRecord) -> Option<Uuid> {
    if admin.role == Role::Admin {
        None
    } else {
        Some(admin.id)
    }
}

/// Trim a display name and enforce the 1–64 character boundary.
///
/// Returns the trimmed value because callers persist what is returned. An
/// empty name or one longer than 64 Unicode scalar values fails with
/// `BadRequest`; UTF-16 bytes are not counted so multi-byte names behave the
/// same in the UI and the database.
pub fn validate_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(ApiError::BadRequest(
            "name must be between 1 and 64 characters".into(),
        ));
    }
    Ok(name.to_string())
}

/// Normalize and validate an account email.
///
/// The result is trimmed and lowercased so lookups are case-insensitive. A
/// valid address has exactly one `@`, a non-empty local part and a domain with
/// at least one dot and no empty labels, contains no whitespace, and is at most
/// 254 bytes.
pub fn validate_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(ApiError::BadRequest("invalid email".into()));
    };
    // Reject empty domain labels and a second `@`: "a@.com", "a@com." and
    // "a@b@c.com" parse as strings but cannot be addressed by any mail system.
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

/// Validate an account password length (8–128 bytes).
///
/// Length is measured in bytes, matching Argon2 input; this bounds hashing
/// cost and rejects trivially short secrets.
pub fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 || password.len() > 128 {
        return Err(ApiError::BadRequest(
            "password must be between 8 and 128 characters".into(),
        ));
    }
    Ok(())
}

/// Verify credentials and create a session token.
///
/// Password hashing runs on the blocking pool: Argon2id deliberately burns CPU
/// and must not stall the async runtime. A semaphore caps concurrent
/// verifications so an attacker cannot exhaust worker threads with login
/// attempts.
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

/// Drop a session token. Unknown tokens are ignored so logout is idempotent.
pub async fn logout(state: &AppState, token: &str) -> Result<(), ApiError> {
    state.db.delete_session(token).await?;
    Ok(())
}

/// Resolve a session token to its account, mapping expired or unknown tokens
/// to `Unauthorized`.
pub async fn admin_for_token(state: &AppState, token: &str) -> Result<AdminRecord, ApiError> {
    state
        .db
        .session_admin(token)
        .await?
        .ok_or(ApiError::Unauthorized)
}

/// Same as [`admin_for_token`] but additionally enforces the admin role.
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

/// Aggregate counters scoped to the caller: admins see the whole
/// installation, regular users only their own users and nodes' global counts.
pub async fn stats(state: &AppState, admin: &AdminRecord) -> Result<StatsView, ApiError> {
    let mut stats = state.db.stats(scope(admin)).await?;
    stats.active_connections =
        state.active_connections().await + state.dataplane.active_connections();
    Ok(stats)
}

/// Build the node list with live connection data overlaid on persisted rows.
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

/// List every node. Callers must already have verified the admin role; this
/// function only assembles data and does no authorization of its own.
pub async fn list_nodes(state: &AppState) -> Result<Vec<NodeView>, ApiError> {
    node_views(state).await
}

/// Create a child node and return its one-time registration token.
///
/// Admin-only at the transport layer. The token is returned exactly once and
/// is stored hashed by nothing more than a plain column, so treat the value in
/// this response as a secret that cannot be recovered later.
pub async fn create_node(state: &AppState, name: &str) -> Result<(NodeView, String), ApiError> {
    let name = validate_name(name)?;
    let (node, token) = state.db.create_node(&name, false).await?;
    Ok((NodeView::from(&node), token))
}

/// Patch a node's name and public URLs.
///
/// A missing node yields `NotFound`. URL changes are pushed to every connected
/// node immediately because they decide where relay hops dial next.
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
    // URL changes alter where other nodes dial this node, so push a fresh
    // topology snapshot to every connected node.
    state.publish_topology().await;
    let node = state.db.get_node(id).await?.ok_or(ApiError::NotFound)?;
    Ok(NodeView::from(&node))
}

/// Delete a child node and remove it from the live map/topology.
///
/// The master's own row is protected: deleting it would orphan every link.
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

/// List proxy users visible to the caller (all for admins, own only for users).
pub async fn list_users(
    state: &AppState,
    admin: &AdminRecord,
) -> Result<Vec<ProxyUserView>, ApiError> {
    let users = state.db.list_proxy_users(scope(admin)).await?;
    Ok(users.iter().map(ProxyUserView::from).collect())
}

/// Create a proxy user and push the new config to every connected node.
///
/// `owner_id` is honoured only for admin callers; regular users always own
/// what they create. A non-existent owner fails with `NotFound`.
pub async fn create_user(
    state: &AppState,
    admin: &AdminRecord,
    name: &str,
    owner_id: Option<Uuid>,
) -> Result<ProxyUserView, ApiError> {
    let name = validate_name(name)?;
    // Admins may create users on behalf of another account; everyone else can
    // only create users they own.
    let owner_id = match (admin.role, owner_id) {
        (Role::Admin, Some(owner_id)) => owner_id,
        _ => admin.id,
    };
    let user = state.db.create_proxy_user(owner_id, &name).await?;
    state.publish_config().await;
    Ok(ProxyUserView::from(&user))
}

/// Rename or enable/disable a proxy user.
///
/// Regular users may only mutate users they own (`Forbidden` otherwise). The
/// updated config is pushed to all nodes so revocation takes effect live.
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

/// Delete a proxy user and its traffic history.
///
/// Ownership rules match [`update_user`].
pub async fn delete_user(state: &AppState, admin: &AdminRecord, id: Uuid) -> Result<(), ApiError> {
    let Some(user) = state.db.get_proxy_user(id).await? else {
        return Err(ApiError::NotFound);
    };
    if admin.role != Role::Admin && user.owner_id != admin.id {
        return Err(ApiError::Forbidden);
    }
    state.db.delete_proxy_user(id).await?;
    state.publish_config().await;
    Ok(())
}

/// List management accounts. Admin-only at the transport layer.
pub async fn list_admins(state: &AppState) -> Result<Vec<AdminView>, ApiError> {
    let admins = state.db.list_admins().await?;
    Ok(admins.iter().map(AdminView::from).collect())
}

/// Create a management account after validating email/password.
///
/// Admin-only at the transport layer; Argon2 hashing happens here so both
/// transports share the same cost parameters. Duplicate emails map to
/// `Conflict`.
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

/// Delete another management account.
///
/// Refuses self-deletion and removal of the last admin (checked atomically in
/// the database) so the installation cannot be locked out.
pub async fn delete_admin(
    state: &AppState,
    current: &AdminRecord,
    id: Uuid,
) -> Result<(), ApiError> {
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
    Ok(())
}

/// List relay links. Admin-only at the transport layer.
pub async fn list_links(state: &AppState) -> Result<Vec<LinkView>, ApiError> {
    let links = state.db.list_links().await?;
    Ok(links.iter().map(LinkView::from).collect())
}

/// Create a directed relay link and publish the new topology.
///
/// The database rejects self-links, duplicates, unknown transports, and links
/// that would form a cycle; those surface as `BadRequest`/`Conflict`. The
/// generated link token is only delivered to the two endpoints via the topology
/// snapshot and is never returned by this function.
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

/// Delete a relay link (idempotent at the database level; unknown ids are
/// reported as `NotFound`) and republish the topology.
pub async fn delete_link(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    if !state.db.delete_link(id).await? {
        return Err(ApiError::NotFound);
    }
    state.publish_topology().await;
    Ok(())
}

/// Node and link snapshot used by the connections page.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyResponse {
    pub nodes: Vec<NodeView>,
    pub links: Vec<LinkView>,
}

/// Snapshot of nodes and links for the connections page. Admin-only at the
/// transport layer.
pub async fn topology(state: &AppState) -> Result<TopologyResponse, ApiError> {
    // `node_views` already overlays live status; the topology page needs the
    // same view as the node list so the two pages cannot disagree.
    Ok(TopologyResponse {
        nodes: node_views(state).await?,
        links: list_links(state).await?,
    })
}

/// Query one page of access logs.
///
/// `limit = 0` selects [`DEFAULT_ACCESS_LOG_LIMIT`]; larger values are clamped
/// to [`MAX_ACCESS_LOG_LIMIT`] so a single request cannot scan unbounded
/// history. Negative offsets are treated as zero. Regular users only see logs
/// for their own proxy users.
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
    state
        .db
        .query_access_logs(
            scope(admin),
            &AccessLogQuery {
                limit,
                offset,
                ..query
            },
        )
        .await
        .map_err(ApiError::from)
}

/// Build the traffic topology for a rolling window.
///
/// `window` defaults to one hour and is clamped to [60 s, 7 days]. Link
/// counters are global, so they are zeroed for regular users to avoid leaking
/// other tenants' traffic.
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
    // Link counters are global; regular users must not learn about traffic on
    // links they do not own.
    if admin.role != Role::Admin {
        for link in &mut topology.links {
            link.up_bytes = 0;
            link.down_bytes = 0;
            link.sessions = 0;
        }
    }
    Ok(topology)
}

/// Per-user traffic totals for the caller's scope.
pub async fn traffic_overview(
    state: &AppState,
    admin: &AdminRecord,
) -> Result<Vec<TrafficRow>, ApiError> {
    Ok(state.db.traffic_overview(scope(admin)).await?)
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

        // Cross-account reads through mutations must be rejected.
        let alice_user_id = uuid(&alice_user.id);
        assert!(matches!(
            update_user(&state, &bob, alice_user_id, Some("stolen"), None).await,
            Err(ApiError::Forbidden)
        ));
        assert!(matches!(
            delete_user(&state, &bob, alice_user_id).await,
            Err(ApiError::Forbidden)
        ));

        // The owner can still change their own user.
        let updated = update_user(&state, &bob, uuid(&bob_user.id), Some("bob-2"), Some(false))
            .await
            .unwrap();
        assert_eq!(updated.name, "bob-2");
        assert!(!updated.enabled);
    }
}
