use serde::{Deserialize, Serialize};
use turso::{params, Builder, Connection, Database, Row};
use uuid::Uuid;

use rust_vless_common::util::{now_ms, random_hex};

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Turso(#[from] turso::Error),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid data: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteAdminOutcome {
    /// 已删除，并返回同一事务内实际删除的代理用户 ID。
    Deleted(Vec<Uuid>),
    NotFound,
    LastAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "admin" => Some(Role::Admin),
            "user" => Some(Role::User),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdminRecord {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: Role,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminView {
    pub id: String,
    pub email: String,
    pub role: Role,
    pub created_at: i64,
}

impl From<&AdminRecord> for AdminView {
    fn from(admin: &AdminRecord) -> Self {
        Self {
            id: admin.id.to_string(),
            email: admin.email.clone(),
            role: admin.role,
            created_at: admin.created_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeRecord {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub version: String,
    pub http_url: String,
    pub rpc_url: String,
    pub is_master: bool,
    pub last_seen: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    pub id: String,
    pub name: String,
    pub status: String,
    pub version: String,
    pub http_url: String,
    pub rpc_url: String,
    pub is_master: bool,
    pub last_seen: i64,
    pub created_at: i64,
    pub active_connections: u64,
    pub uptime_sec: u64,
}

impl From<&NodeRecord> for NodeView {
    fn from(node: &NodeRecord) -> Self {
        Self {
            id: node.id.to_string(),
            name: node.name.clone(),
            status: node.status.clone(),
            version: node.version.clone(),
            http_url: node.http_url.clone(),
            rpc_url: node.rpc_url.clone(),
            is_master: node.is_master,
            last_seen: node.last_seen,
            created_at: node.created_at,
            active_connections: 0,
            uptime_sec: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub from_name: String,
    pub to_name: String,
    pub transport: String,
    pub priority: i64,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub id: Uuid,
    pub from_node_id: Uuid,
    pub to_node_id: Uuid,
    pub from_name: String,
    pub to_name: String,
    pub to_http_url: String,
    pub to_rpc_url: String,
    pub transport: String,
    pub token: String,
    pub priority: i64,
    pub enabled: bool,
    pub created_at: i64,
}

impl From<&LinkRecord> for LinkView {
    fn from(link: &LinkRecord) -> Self {
        Self {
            id: link.id.to_string(),
            from_node_id: link.from_node_id.to_string(),
            to_node_id: link.to_node_id.to_string(),
            from_name: link.from_name.clone(),
            to_name: link.to_name.clone(),
            transport: link.transport.clone(),
            priority: link.priority,
            enabled: link.enabled,
            created_at: link.created_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyUserRecord {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub owner_id: Uuid,
    pub owner_email: String,
    pub created_at: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUserView {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub owner_id: String,
    pub owner_email: String,
    pub created_at: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
}

impl From<&ProxyUserRecord> for ProxyUserView {
    fn from(user: &ProxyUserRecord) -> Self {
        Self {
            id: user.id.to_string(),
            name: user.name.clone(),
            enabled: user.enabled,
            owner_id: user.owner_id.to_string(),
            owner_email: user.owner_email.clone(),
            created_at: user.created_at,
            up_bytes: user.up_bytes,
            down_bytes: user.down_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficRow {
    pub user_id: String,
    pub name: String,
    pub owner_email: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsView {
    pub nodes_total: i64,
    pub nodes_online: i64,
    pub users_total: i64,
    pub users_enabled: i64,
    pub admins_total: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub active_connections: u64,
    pub links_total: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AccessLogInput {
    pub session_id: String,
    pub user_id: String,
    pub entry_node_id: String,
    pub target: String,
    pub command: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub status: String,
    pub error: String,
    pub path: Vec<String>,
    pub client_addr: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessLogView {
    pub id: i64,
    pub session_id: String,
    pub user_id: String,
    pub entry_node_id: String,
    pub target: String,
    pub command: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub status: String,
    pub error: String,
    pub path: Vec<String>,
    pub client_addr: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccessLogQuery {
    pub user_id: Option<String>,
    pub node_id: Option<String>,
    pub status: Option<String>,
    pub target: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessLogPage {
    pub items: Vec<AccessLogView>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTrafficRow {
    pub link_id: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub sessions: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNodeView {
    pub id: String,
    pub name: String,
    pub is_master: bool,
    pub online: bool,
    pub active_connections: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyLinkView {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transport: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub sessions: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficTopology {
    pub window_sec: i64,
    pub nodes: Vec<TopologyNodeView>,
    pub links: Vec<TopologyLinkView>,
}

#[derive(Clone)]
pub struct Db {
    db: Database,
}

impl Db {
    pub async fn open(path: &str) -> DbResult<Self> {
        let db = Builder::new_local(path).build().await?;
        let this = Self { db };
        this.migrate().await?;
        this.mark_all_nodes_offline().await?;
        Ok(this)
    }

    fn connect(&self) -> DbResult<Connection> {
        let conn = self.db.connect()?;
        let _ = conn.busy_timeout(std::time::Duration::from_secs(10));
        Ok(conn)
    }

    async fn migrate(&self) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS admins (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    admin_id   TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS nodes (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    token      TEXT NOT NULL UNIQUE,
    status     TEXT NOT NULL DEFAULT 'offline',
    version    TEXT NOT NULL DEFAULT '',
    addresses  TEXT NOT NULL DEFAULT '[]',
    http_url   TEXT NOT NULL DEFAULT '',
    rpc_url    TEXT NOT NULL DEFAULT '',
    is_master  INTEGER NOT NULL DEFAULT 0,
    last_seen  INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS node_links (
    id           TEXT PRIMARY KEY,
    from_node_id TEXT NOT NULL,
    to_node_id   TEXT NOT NULL,
    transport    TEXT NOT NULL DEFAULT 'rpc',
    token        TEXT NOT NULL,
    priority     INTEGER NOT NULL DEFAULT 0,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL,
    UNIQUE(from_node_id, to_node_id)
);
CREATE TABLE IF NOT EXISTS proxy_users (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    owner_id   TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS traffic_samples (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id     TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    up_bytes    INTEGER NOT NULL,
    down_bytes  INTEGER NOT NULL,
    reported_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS traffic_totals (
    user_id    TEXT PRIMARY KEY,
    up_bytes   INTEGER NOT NULL DEFAULT 0,
    down_bytes INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS access_logs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    entry_node_id TEXT NOT NULL,
    target        TEXT NOT NULL,
    command       TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    up_bytes      INTEGER NOT NULL DEFAULT 0,
    down_bytes    INTEGER NOT NULL DEFAULT 0,
    status        TEXT NOT NULL,
    error         TEXT NOT NULL DEFAULT '',
    path          TEXT NOT NULL DEFAULT '[]',
    client_addr   TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS link_traffic_samples (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    link_id     TEXT NOT NULL,
    up_bytes    INTEGER NOT NULL,
    down_bytes  INTEGER NOT NULL,
    sessions    INTEGER NOT NULL DEFAULT 0,
    reported_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS link_traffic_totals (
    link_id    TEXT PRIMARY KEY,
    up_bytes   INTEGER NOT NULL DEFAULT 0,
    down_bytes INTEGER NOT NULL DEFAULT 0,
    sessions   INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_log_purges (
    user_id TEXT PRIMARY KEY
);
CREATE INDEX IF NOT EXISTS idx_sessions_admin ON sessions(admin_id);
CREATE INDEX IF NOT EXISTS idx_users_owner ON proxy_users(owner_id);
CREATE INDEX IF NOT EXISTS idx_samples_user ON traffic_samples(user_id, reported_at);
CREATE INDEX IF NOT EXISTS idx_access_user ON access_logs(user_id, started_at);
CREATE INDEX IF NOT EXISTS idx_access_node ON access_logs(entry_node_id, started_at);
CREATE INDEX IF NOT EXISTS idx_link_samples ON link_traffic_samples(link_id, reported_at);
CREATE INDEX IF NOT EXISTS idx_links_from ON node_links(from_node_id);
"#,
        )
        .await?;

        // 为旧版本创建的数据库做增量迁移。
        //
        // 先用 PRAGMA table_info 判断列是否存在，再执行 ALTER：不依赖各版本
        // SQLite/Turso 的错误文案，重复启动不会失败，真正的锁/IO 错误会照常上报。
        for (table, column, definition) in [
            ("nodes", "http_url", "TEXT NOT NULL DEFAULT ''"),
            ("nodes", "rpc_url", "TEXT NOT NULL DEFAULT ''"),
            ("nodes", "is_master", "INTEGER NOT NULL DEFAULT 0"),
            ("admins", "sub_token", "TEXT NOT NULL DEFAULT ''"),
        ] {
            if !column_exists(&conn, table, column).await? {
                conn.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                    (),
                )
                .await?;
            }
        }
        // 订阅 token 必须唯一；空字符串表示「尚未生成」，用部分索引排除。
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_admins_sub_token
             ON admins(sub_token) WHERE sub_token <> ''",
            (),
        )
        .await?;
        Ok(())
    }

    async fn mark_all_nodes_offline(&self) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute("UPDATE nodes SET status = 'offline'", ())
            .await?;
        Ok(())
    }

    pub async fn get_setting(&self, key: &str) -> DbResult<Option<String>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT value FROM settings WHERE key = ?1", params![key])
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .await?;
        Ok(())
    }

    /// 登记未能立即清理的访问日志用户；主键保证并发登记不会互相覆盖。
    pub async fn queue_log_purges(&self, user_ids: &[String]) -> DbResult<()> {
        let conn = self.connect()?;
        for user_id in user_ids {
            conn.execute(
                "INSERT OR IGNORE INTO pending_log_purges (user_id) VALUES (?1)",
                params![user_id.clone()],
            )
            .await?;
        }
        Ok(())
    }

    /// 列出待清理的用户 ID。
    pub async fn list_pending_log_purges(&self) -> DbResult<Vec<String>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT user_id FROM pending_log_purges", ())
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get(0)?);
        }
        Ok(out)
    }

    /// 清理成功后逐条确认，只删除本次处理过的 ID。
    pub async fn clear_log_purge(&self, user_id: &str) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM pending_log_purges WHERE user_id = ?1",
            params![user_id.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn count_admins(&self) -> DbResult<i64> {
        let conn = self.connect()?;
        let mut rows = conn.query("SELECT COUNT(*) FROM admins", ()).await?;
        if let Some(row) = rows.next().await? {
            Ok(row.get(0)?)
        } else {
            Ok(0)
        }
    }

    pub async fn find_admin_by_email(&self, email: &str) -> DbResult<Option<AdminRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, email, password_hash, role, created_at FROM admins WHERE email = ?1",
                params![email.to_lowercase()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(admin_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn get_admin(&self, id: Uuid) -> DbResult<Option<AdminRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, email, password_hash, role, created_at FROM admins WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(admin_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn create_admin(
        &self,
        email: &str,
        password_hash: &str,
        role: Role,
    ) -> DbResult<AdminRecord> {
        let email = email.trim().to_lowercase();
        if self.find_admin_by_email(&email).await?.is_some() {
            return Err(DbError::Conflict("email already exists".into()));
        }
        let record = AdminRecord {
            id: Uuid::new_v4(),
            email,
            password_hash: password_hash.to_string(),
            role,
            created_at: now_ms(),
        };
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO admins (id, email, password_hash, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                record.id.to_string(),
                record.email.clone(),
                record.password_hash.clone(),
                record.role.as_str(),
                record.created_at
            ],
        )
        .await?;
        Ok(record)
    }

    /// 原子地创建第一个管理员；已有账号时返回 `None`。
    ///
    /// 首次安装页面可能被并发提交，`BEGIN IMMEDIATE` 保证只有一个请求能
    /// 观察到空表并插入，避免出现两个「初始管理员」。
    pub async fn create_first_admin_if_empty(
        &self,
        email: &str,
        password_hash: &str,
        role: Role,
    ) -> DbResult<Option<AdminRecord>> {
        let email = email.trim().to_lowercase();
        let conn = self.connect()?;
        conn.execute("BEGIN IMMEDIATE", ()).await?;

        let result: DbResult<Option<AdminRecord>> = async {
            let mut rows = conn.query("SELECT COUNT(*) FROM admins", ()).await?;
            let count: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            drop(rows);
            if count > 0 {
                return Ok(None);
            }

            let record = AdminRecord {
                id: Uuid::new_v4(),
                email,
                password_hash: password_hash.to_string(),
                role,
                created_at: now_ms(),
            };
            conn.execute(
                "INSERT INTO admins (id, email, password_hash, role, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.id.to_string(),
                    record.email.clone(),
                    record.password_hash.clone(),
                    record.role.as_str(),
                    record.created_at
                ],
            )
            .await?;
            Ok(Some(record))
        }
        .await;

        match result {
            Ok(Some(record)) => {
                conn.execute("COMMIT", ()).await?;
                Ok(Some(record))
            }
            Ok(None) => {
                conn.execute("ROLLBACK", ()).await?;
                Ok(None)
            }
            Err(err) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(err)
            }
        }
    }

    /// 读取账号的订阅 token，没有则生成并持久化。
    ///
    /// token 是 32 字节随机串；一旦生成就保持不变，避免客户端订阅地址失效。
    pub async fn get_or_create_sub_token(&self, admin_id: Uuid) -> DbResult<String> {
        let conn = self.connect()?;
        let token = random_hex(32);
        // 条件更新保证并发首次访问只有一个 token 生效。
        let updated = conn
            .execute(
                "UPDATE admins SET sub_token = ?1 WHERE id = ?2 AND sub_token = ''",
                params![token.clone(), admin_id.to_string()],
            )
            .await?;
        if updated > 0 {
            return Ok(token);
        }
        // 更新未生效：要么其他请求刚写入 token，要么账号不存在。
        let mut rows = conn
            .query(
                "SELECT sub_token FROM admins WHERE id = ?1 AND sub_token <> ''",
                params![admin_id.to_string()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(row.get(0)?),
            None => Err(DbError::NotFound),
        }
    }

    /// 按订阅 token 查找账号；空 token 永远不匹配。
    pub async fn admin_by_sub_token(&self, token: &str) -> DbResult<Option<AdminRecord>> {
        if token.is_empty() {
            return Ok(None);
        }
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, email, password_hash, role, created_at
                 FROM admins WHERE sub_token = ?1 AND sub_token <> ''",
                params![token],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(admin_from_row(&row)?)),
            None => Ok(None),
        }
    }

    /// 列出某账号名下启用的代理用户（订阅只包含启用项）。
    pub async fn enabled_proxy_users_for_owner(
        &self,
        owner: Uuid,
    ) -> DbResult<Vec<(Uuid, String)>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name FROM proxy_users
                 WHERE owner_id = ?1 AND enabled = 1
                 ORDER BY created_at ASC",
                params![owner.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            if let Ok(id) = Uuid::parse_str(&id) {
                out.push((id, name));
            }
        }
        Ok(out)
    }

    pub async fn list_admins(&self) -> DbResult<Vec<AdminRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, email, password_hash, role, created_at FROM admins ORDER BY created_at ASC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(admin_from_row(&row)?);
        }
        Ok(out)
    }

    /// 原子地删除管理员并检查是否为最后一个。
    /// 两个并发请求无法同时观察到两个管理员并互相删除。
    pub async fn delete_admin_checked(&self, id: Uuid) -> DbResult<DeleteAdminOutcome> {
        let conn = self.connect()?;
        conn.execute("BEGIN IMMEDIATE", ()).await?;
        let id_s = id.to_string();
        let result: DbResult<DeleteAdminOutcome> = async {
            let mut rows = conn.query("SELECT COUNT(*) FROM admins", ()).await?;
            let count: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            drop(rows);
            if count <= 1 {
                return Ok(DeleteAdminOutcome::LastAdmin);
            }

            let mut rows = conn
                .query(
                    "SELECT COUNT(*) FROM admins WHERE id = ?1",
                    params![id_s.clone()],
                )
                .await?;
            let exists: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            drop(rows);
            if exists == 0 {
                return Ok(DeleteAdminOutcome::NotFound);
            }

            // 在同一事务内读取将被级联删除的用户，事务外读取会漏掉并发新建的用户。
            let mut rows = conn
                .query(
                    "SELECT id FROM proxy_users WHERE owner_id = ?1",
                    params![id_s.clone()],
                )
                .await?;
            let mut owned_users = Vec::new();
            while let Some(row) = rows.next().await? {
                let raw: String = row.get(0)?;
                if let Ok(user_id) = Uuid::parse_str(&raw) {
                    owned_users.push(user_id);
                }
            }
            drop(rows);

            conn.execute(
                "DELETE FROM traffic_totals WHERE user_id IN (SELECT id FROM proxy_users WHERE owner_id = ?1)",
                params![id_s.clone()],
            )
            .await?;
            conn.execute(
                "DELETE FROM access_logs WHERE user_id IN (SELECT id FROM proxy_users WHERE owner_id = ?1)",
                params![id_s.clone()],
            )
            .await?;
            conn.execute(
                "DELETE FROM proxy_users WHERE owner_id = ?1",
                params![id_s.clone()],
            )
            .await?;
            conn.execute(
                "DELETE FROM sessions WHERE admin_id = ?1",
                params![id_s.clone()],
            )
            .await?;
            conn.execute("DELETE FROM admins WHERE id = ?1", params![id_s])
                .await?;
            Ok(DeleteAdminOutcome::Deleted(owned_users))
        }
        .await;

        match result {
            Ok(DeleteAdminOutcome::Deleted(owned_users)) => {
                conn.execute("COMMIT", ()).await?;
                Ok(DeleteAdminOutcome::Deleted(owned_users))
            }
            Ok(other) => {
                conn.execute("ROLLBACK", ()).await?;
                Ok(other)
            }
            Err(err) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(err)
            }
        }
    }

    pub async fn create_session(
        &self,
        admin_id: Uuid,
        token: &str,
        expires_at: i64,
    ) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO sessions (token, admin_id, expires_at) VALUES (?1, ?2, ?3)",
            params![token, admin_id.to_string(), expires_at],
        )
        .await?;
        Ok(())
    }

    pub async fn session_admin(&self, token: &str) -> DbResult<Option<AdminRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT a.id, a.email, a.password_hash, a.role, a.created_at, s.expires_at
                 FROM sessions s JOIN admins a ON a.id = s.admin_id
                 WHERE s.token = ?1",
                params![token],
            )
            .await?;
        let found = match rows.next().await? {
            Some(row) => {
                let expires_at: i64 = row.get(5)?;
                Some((admin_from_row(&row)?, expires_at))
            }
            None => None,
        };
        drop(rows);

        match found {
            Some((admin, expires_at)) if expires_at > now_ms() => Ok(Some(admin)),
            Some(_) => {
                conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])
                    .await?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub async fn delete_session(&self, token: &str) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])
            .await?;
        Ok(())
    }

    pub async fn create_node(&self, name: &str, is_master: bool) -> DbResult<(NodeRecord, String)> {
        let record = NodeRecord {
            id: Uuid::new_v4(),
            name: name.trim().to_string(),
            status: "offline".into(),
            version: String::new(),
            http_url: String::new(),
            rpc_url: String::new(),
            is_master,
            last_seen: 0,
            created_at: now_ms(),
        };
        let token = random_hex(32);
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO nodes (id, name, token, status, version, http_url, rpc_url, is_master, last_seen, created_at)
             VALUES (?1, ?2, ?3, 'offline', '', '', '', ?4, 0, ?5)",
            params![
                record.id.to_string(),
                record.name.clone(),
                token.clone(),
                if is_master { 1 } else { 0 },
                record.created_at
            ],
        )
        .await?;
        Ok((record, token))
    }

    /// 只在首次运行时创建主节点记录，之后保持其 URL 同步。
    pub async fn ensure_master_node(
        &self,
        id: Uuid,
        name: &str,
        http_url: &str,
        rpc_url: &str,
    ) -> DbResult<NodeRecord> {
        if self.get_node(id).await?.is_some() {
            self.update_node(
                id,
                Some(name),
                if http_url.is_empty() {
                    None
                } else {
                    Some(http_url)
                },
                if rpc_url.is_empty() {
                    None
                } else {
                    Some(rpc_url)
                },
            )
            .await?;
            return self.get_node(id).await?.ok_or(DbError::NotFound);
        }
        let token = random_hex(32);
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO nodes (id, name, token, status, version, http_url, rpc_url, is_master, last_seen, created_at)
             VALUES (?1, ?2, ?3, 'offline', '', ?4, ?5, 1, 0, ?6)",
            params![id.to_string(), name, token, http_url, rpc_url, now_ms()],
        )
        .await?;
        self.get_node(id).await?.ok_or(DbError::NotFound)
    }

    pub async fn get_node(&self, id: Uuid) -> DbResult<Option<NodeRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name, status, version, http_url, rpc_url, is_master, last_seen, created_at
                 FROM nodes WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(node_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_nodes(&self) -> DbResult<Vec<NodeRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name, status, version, http_url, rpc_url, is_master, last_seen, created_at
                 FROM nodes ORDER BY is_master DESC, created_at ASC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(node_from_row(&row)?);
        }
        Ok(out)
    }

    pub async fn update_node(
        &self,
        id: Uuid,
        name: Option<&str>,
        http_url: Option<&str>,
        rpc_url: Option<&str>,
    ) -> DbResult<()> {
        let Some(node) = self.get_node(id).await? else {
            return Err(DbError::NotFound);
        };
        let conn = self.connect()?;
        conn.execute(
            "UPDATE nodes SET name = ?1, http_url = ?2, rpc_url = ?3 WHERE id = ?4",
            params![
                name.map(str::trim).unwrap_or(&node.name),
                http_url.unwrap_or(&node.http_url),
                rpc_url.unwrap_or(&node.rpc_url),
                id.to_string()
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn verify_node(&self, id: Uuid, token: &str) -> DbResult<Option<NodeRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name, status, version, http_url, rpc_url, is_master, last_seen, created_at
                 FROM nodes WHERE id = ?1 AND token = ?2",
                params![id.to_string(), token],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(node_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn update_node_heartbeat(
        &self,
        id: Uuid,
        version: &str,
        http_url: &str,
        rpc_url: &str,
    ) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE nodes
             SET status = 'online', version = ?1,
                 http_url = CASE WHEN http_url = '' THEN ?2 ELSE http_url END,
                 rpc_url = CASE WHEN rpc_url = '' THEN ?3 ELSE rpc_url END,
                 last_seen = ?4
             WHERE id = ?5",
            params![version, http_url, rpc_url, now_ms(), id.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn mark_node_offline(&self, id: Uuid) -> DbResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE nodes SET status = 'offline' WHERE id = ?1",
            params![id.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn delete_node(&self, id: Uuid) -> DbResult<bool> {
        let Some(node) = self.get_node(id).await? else {
            return Ok(false);
        };
        if node.is_master {
            return Ok(false);
        }
        let conn = self.connect()?;
        let id_s = id.to_string();
        conn.execute(
            "DELETE FROM node_links WHERE from_node_id = ?1 OR to_node_id = ?1",
            params![id_s.clone()],
        )
        .await?;
        let deleted = conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id_s])
            .await?;
        Ok(deleted > 0)
    }

    pub async fn create_link(
        &self,
        from_id: Uuid,
        to_id: Uuid,
        transport: &str,
        priority: i64,
    ) -> DbResult<LinkRecord> {
        if from_id == to_id {
            return Err(DbError::Invalid("cannot link a node to itself".into()));
        }
        if transport != "rpc" && transport != "http" {
            return Err(DbError::Invalid("transport must be rpc or http".into()));
        }
        let Some(from) = self.get_node(from_id).await? else {
            return Err(DbError::NotFound);
        };
        let Some(to) = self.get_node(to_id).await? else {
            return Err(DbError::NotFound);
        };
        // 拒绝操作员创建成环的连线；运行时 `visited` 检查仍用于防御
        // 过期拓扑或手工改库。
        let existing = self.list_links().await?;
        if reaches(&existing, to_id, from_id) {
            return Err(DbError::Conflict("link would create a cycle".into()));
        }
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM node_links WHERE from_node_id = ?1 AND to_node_id = ?2",
                params![from_id.to_string(), to_id.to_string()],
            )
            .await?;
        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            if count > 0 {
                return Err(DbError::Conflict("link already exists".into()));
            }
        }
        drop(rows);

        let token = random_hex(32);
        let record = LinkRecord {
            id: Uuid::new_v4(),
            from_node_id: from_id,
            to_node_id: to_id,
            from_name: from.name,
            to_name: to.name,
            to_http_url: to.http_url,
            to_rpc_url: to.rpc_url,
            transport: transport.to_string(),
            token: token.clone(),
            priority,
            enabled: true,
            created_at: now_ms(),
        };
        conn.execute(
            "INSERT INTO node_links (id, from_node_id, to_node_id, transport, token, priority, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)",
            params![
                record.id.to_string(),
                from_id.to_string(),
                to_id.to_string(),
                transport,
                token,
                priority,
                record.created_at
            ],
        )
        .await?;
        Ok(record)
    }

    pub async fn list_links(&self) -> DbResult<Vec<LinkRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT l.id, l.from_node_id, l.to_node_id, l.transport, l.token, l.priority,
                        l.enabled, l.created_at,
                        COALESCE(f.name, ''), COALESCE(t.name, ''),
                        COALESCE(t.http_url, ''), COALESCE(t.rpc_url, '')
                 FROM node_links l
                 LEFT JOIN nodes f ON f.id = l.from_node_id
                 LEFT JOIN nodes t ON t.id = l.to_node_id
                 ORDER BY l.priority ASC, l.created_at ASC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(link_from_row(&row)?);
        }
        Ok(out)
    }

    pub async fn delete_link(&self, id: Uuid) -> DbResult<bool> {
        let conn = self.connect()?;
        let deleted = conn
            .execute(
                "DELETE FROM node_links WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;
        Ok(deleted > 0)
    }

    pub async fn create_proxy_user(&self, owner_id: Uuid, name: &str) -> DbResult<ProxyUserRecord> {
        let owner = self.get_admin(owner_id).await?.ok_or(DbError::NotFound)?;
        let record = ProxyUserRecord {
            id: Uuid::new_v4(),
            name: name.trim().to_string(),
            enabled: true,
            owner_id,
            owner_email: owner.email,
            created_at: now_ms(),
            up_bytes: 0,
            down_bytes: 0,
        };
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO proxy_users (id, name, enabled, owner_id, created_at)
             VALUES (?1, ?2, 1, ?3, ?4)",
            params![
                record.id.to_string(),
                record.name.clone(),
                record.owner_id.to_string(),
                record.created_at
            ],
        )
        .await?;
        Ok(record)
    }

    pub async fn list_proxy_users(&self, owner: Option<Uuid>) -> DbResult<Vec<ProxyUserRecord>> {
        let conn = self.connect()?;
        let sql = "SELECT u.id, u.name, u.enabled, u.owner_id, COALESCE(a.email, ''), u.created_at,
                          COALESCE(t.up_bytes, 0), COALESCE(t.down_bytes, 0)
                   FROM proxy_users u
                   LEFT JOIN admins a ON a.id = u.owner_id
                   LEFT JOIN traffic_totals t ON t.user_id = u.id";
        let mut rows = match owner {
            Some(owner) => {
                conn.query(
                    &format!("{sql} WHERE u.owner_id = ?1 ORDER BY u.created_at DESC"),
                    params![owner.to_string()],
                )
                .await?
            }
            None => {
                conn.query(&format!("{sql} ORDER BY u.created_at DESC"), ())
                    .await?
            }
        };
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(proxy_user_from_row(&row)?);
        }
        Ok(out)
    }

    pub async fn get_proxy_user(&self, id: Uuid) -> DbResult<Option<ProxyUserRecord>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT u.id, u.name, u.enabled, u.owner_id, COALESCE(a.email, ''), u.created_at,
                        COALESCE(t.up_bytes, 0), COALESCE(t.down_bytes, 0)
                 FROM proxy_users u
                 LEFT JOIN admins a ON a.id = u.owner_id
                 LEFT JOIN traffic_totals t ON t.user_id = u.id
                 WHERE u.id = ?1",
                params![id.to_string()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(proxy_user_from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn update_proxy_user(
        &self,
        id: Uuid,
        name: Option<&str>,
        enabled: Option<bool>,
    ) -> DbResult<()> {
        let Some(current) = self.get_proxy_user(id).await? else {
            return Err(DbError::NotFound);
        };
        let name = name.map(str::trim).unwrap_or(&current.name);
        let enabled = enabled.unwrap_or(current.enabled);
        let conn = self.connect()?;
        conn.execute(
            "UPDATE proxy_users SET name = ?1, enabled = ?2 WHERE id = ?3",
            params![name, if enabled { 1 } else { 0 }, id.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn delete_proxy_user(&self, id: Uuid) -> DbResult<bool> {
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM traffic_totals WHERE user_id = ?1",
            params![id.to_string()],
        )
        .await?;
        conn.execute(
            "DELETE FROM access_logs WHERE user_id = ?1",
            params![id.to_string()],
        )
        .await?;
        let deleted = conn
            .execute(
                "DELETE FROM proxy_users WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;
        Ok(deleted > 0)
    }

    pub async fn enabled_proxy_users(&self) -> DbResult<Vec<(Uuid, String)>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, name FROM proxy_users WHERE enabled = 1 ORDER BY created_at ASC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let raw: String = row.get(0)?;
            let id = Uuid::parse_str(&raw).map_err(|e| DbError::Invalid(e.to_string()))?;
            out.push((id, row.get(1)?));
        }
        Ok(out)
    }

    pub async fn record_traffic(
        &self,
        node_id: Uuid,
        user_id: Uuid,
        up: u64,
        down: u64,
    ) -> DbResult<()> {
        if up == 0 && down == 0 {
            return Ok(());
        }
        let up_i = i64::try_from(up).unwrap_or(i64::MAX);
        let down_i = i64::try_from(down).unwrap_or(i64::MAX);
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO traffic_samples (node_id, user_id, up_bytes, down_bytes, reported_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                node_id.to_string(),
                user_id.to_string(),
                up_i,
                down_i,
                now_ms()
            ],
        )
        .await?;
        conn.execute(
            "INSERT INTO traffic_totals (user_id, up_bytes, down_bytes) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET
                up_bytes = up_bytes + excluded.up_bytes,
                down_bytes = down_bytes + excluded.down_bytes",
            params![user_id.to_string(), up_i, down_i],
        )
        .await?;
        Ok(())
    }

    pub async fn traffic_overview(&self, owner: Option<Uuid>) -> DbResult<Vec<TrafficRow>> {
        let conn = self.connect()?;
        let sql = "SELECT u.id, u.name, COALESCE(a.email, ''),
                          COALESCE(t.up_bytes, 0), COALESCE(t.down_bytes, 0)
                   FROM proxy_users u
                   LEFT JOIN admins a ON a.id = u.owner_id
                   LEFT JOIN traffic_totals t ON t.user_id = u.id";
        let mut rows = match owner {
            Some(owner) => {
                conn.query(
                    &format!("{sql} WHERE u.owner_id = ?1 ORDER BY t.up_bytes + t.down_bytes DESC"),
                    params![owner.to_string()],
                )
                .await?
            }
            None => {
                conn.query(
                    &format!("{sql} ORDER BY t.up_bytes + t.down_bytes DESC"),
                    (),
                )
                .await?
            }
        };
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id: String = row.get(0)?;
            out.push(TrafficRow {
                user_id: id,
                name: row.get(1)?,
                owner_email: row.get(2)?,
                up_bytes: row.get::<i64>(3)?.max(0) as u64,
                down_bytes: row.get::<i64>(4)?.max(0) as u64,
            });
        }
        Ok(out)
    }

    pub async fn stats(&self, owner: Option<Uuid>) -> DbResult<StatsView> {
        let conn = self.connect()?;

        let mut nodes_total = 0i64;
        let mut nodes_online = 0i64;
        let mut rows = conn
            .query(
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status = 'online' THEN 1 ELSE 0 END), 0) FROM nodes",
                (),
            )
            .await?;
        if let Some(row) = rows.next().await? {
            nodes_total = row.get(0)?;
            nodes_online = row.get(1)?;
        }
        drop(rows);

        let mut links_total = 0i64;
        let mut rows = conn.query("SELECT COUNT(*) FROM node_links", ()).await?;
        if let Some(row) = rows.next().await? {
            links_total = row.get(0)?;
        }
        drop(rows);

        let (users_total, users_enabled) = match owner {
            Some(owner) => {
                let mut rows = conn
                    .query(
                        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END), 0)
                         FROM proxy_users WHERE owner_id = ?1",
                        params![owner.to_string()],
                    )
                    .await?;
                match rows.next().await? {
                    Some(row) => (row.get::<i64>(0)?, row.get::<i64>(1)?),
                    None => (0, 0),
                }
            }
            None => {
                let mut rows = conn
                    .query(
                        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END), 0)
                         FROM proxy_users",
                        (),
                    )
                    .await?;
                match rows.next().await? {
                    Some(row) => (row.get::<i64>(0)?, row.get::<i64>(1)?),
                    None => (0, 0),
                }
            }
        };

        let admins_total = match owner {
            Some(_) => 1,
            None => self.count_admins().await?,
        };

        let (up_bytes, down_bytes) = match owner {
            Some(owner) => {
                let mut rows = conn
                    .query(
                        "SELECT COALESCE(SUM(t.up_bytes), 0), COALESCE(SUM(t.down_bytes), 0)
                         FROM traffic_totals t JOIN proxy_users u ON u.id = t.user_id
                         WHERE u.owner_id = ?1",
                        params![owner.to_string()],
                    )
                    .await?;
                match rows.next().await? {
                    Some(row) => {
                        let up: i64 = row.get(0)?;
                        let down: i64 = row.get(1)?;
                        (up.max(0) as u64, down.max(0) as u64)
                    }
                    None => (0, 0),
                }
            }
            None => {
                let mut rows = conn
                    .query(
                        "SELECT COALESCE(SUM(up_bytes), 0), COALESCE(SUM(down_bytes), 0)
                         FROM traffic_totals",
                        (),
                    )
                    .await?;
                match rows.next().await? {
                    Some(row) => {
                        let up: i64 = row.get(0)?;
                        let down: i64 = row.get(1)?;
                        (up.max(0) as u64, down.max(0) as u64)
                    }
                    None => (0, 0),
                }
            }
        };

        Ok(StatsView {
            nodes_total,
            nodes_online,
            users_total,
            users_enabled,
            admins_total,
            up_bytes,
            down_bytes,
            active_connections: 0,
            links_total,
        })
    }

    /// 读出旧版数据库中的全部访问日志，用于一次性迁移到独立文件。
    /// 按自增 ID 游标读取一批历史日志；保留期删除旧行不会影响游标。
    pub async fn access_logs_batch_after(
        &self,
        after_id: i64,
        limit: i64,
    ) -> DbResult<Vec<(i64, AccessLogInput)>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT id, session_id, user_id, entry_node_id, target, command,
                        started_at, ended_at, up_bytes, down_bytes, status, error,
                        path, client_addr
                 FROM access_logs WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
                params![after_id, limit.max(1)],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let path: String = row.get(12)?;
            out.push((
                row.get(0)?,
                AccessLogInput {
                    session_id: row.get(1)?,
                    user_id: row.get(2)?,
                    entry_node_id: row.get(3)?,
                    target: row.get(4)?,
                    command: row.get(5)?,
                    started_at_ms: row.get(6)?,
                    ended_at_ms: row.get(7)?,
                    up_bytes: u64::try_from(row.get::<i64>(8)?).unwrap_or(0),
                    down_bytes: u64::try_from(row.get::<i64>(9)?).unwrap_or(0),
                    status: row.get(10)?,
                    error: row.get(11)?,
                    path: serde_json::from_str(&path).unwrap_or_default(),
                    client_addr: row.get(13)?,
                },
            ));
        }
        Ok(out)
    }

    pub async fn prune_traffic_samples(&self, before_ms: i64) -> DbResult<u64> {
        let conn = self.connect()?;
        Ok(conn
            .execute(
                "DELETE FROM traffic_samples WHERE reported_at < ?1",
                params![before_ms],
            )
            .await?)
    }

    pub async fn prune_link_traffic_samples(&self, before_ms: i64) -> DbResult<u64> {
        let conn = self.connect()?;
        Ok(conn
            .execute(
                "DELETE FROM link_traffic_samples WHERE reported_at < ?1",
                params![before_ms],
            )
            .await?)
    }

    pub async fn prune_access_logs(&self, before_ms: i64) -> DbResult<u64> {
        let conn = self.connect()?;
        Ok(conn
            .execute(
                "DELETE FROM access_logs WHERE started_at < ?1",
                params![before_ms],
            )
            .await?)
    }

    pub async fn record_link_traffic(
        &self,
        link_id: &str,
        up: u64,
        down: u64,
        sessions: u64,
    ) -> DbResult<()> {
        if up == 0 && down == 0 && sessions == 0 {
            return Ok(());
        }
        let up_i = i64::try_from(up).unwrap_or(i64::MAX);
        let down_i = i64::try_from(down).unwrap_or(i64::MAX);
        let sessions_i = i64::try_from(sessions).unwrap_or(i64::MAX);
        let now = now_ms();
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO link_traffic_samples (link_id, up_bytes, down_bytes, sessions, reported_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![link_id, up_i, down_i, sessions_i, now],
        )
        .await?;
        conn.execute(
            "INSERT INTO link_traffic_totals (link_id, up_bytes, down_bytes, sessions, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(link_id) DO UPDATE SET
                up_bytes = up_bytes + excluded.up_bytes,
                down_bytes = down_bytes + excluded.down_bytes,
                sessions = sessions + excluded.sessions,
                updated_at = excluded.updated_at",
            params![link_id, up_i, down_i, sessions_i, now],
        )
        .await?;
        Ok(())
    }

    pub async fn link_traffic_window(&self, since_ms: i64) -> DbResult<Vec<LinkTrafficRow>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                "SELECT link_id, COALESCE(SUM(up_bytes), 0), COALESCE(SUM(down_bytes), 0),
                        COALESCE(SUM(sessions), 0)
                 FROM link_traffic_samples WHERE reported_at >= ?1
                 GROUP BY link_id",
                params![since_ms],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(LinkTrafficRow {
                link_id: row.get(0)?,
                up_bytes: row.get::<i64>(1)?.max(0) as u64,
                down_bytes: row.get::<i64>(2)?.max(0) as u64,
                sessions: row.get::<i64>(3)?.max(0) as u64,
            });
        }
        Ok(out)
    }

    pub async fn traffic_topology(
        &self,
        window_sec: i64,
        live: &std::collections::HashMap<Uuid, crate::state::LiveNode>,
    ) -> DbResult<TrafficTopology> {
        let nodes = self.list_nodes().await?;
        let links = self.list_links().await?;
        let traffic = self
            .link_traffic_window(now_ms() - window_sec.max(1) * 1000)
            .await?;
        let by_link: std::collections::HashMap<&str, &LinkTrafficRow> = traffic
            .iter()
            .map(|row| (row.link_id.as_str(), row))
            .collect();

        Ok(TrafficTopology {
            window_sec,
            nodes: nodes
                .iter()
                .map(|node| TopologyNodeView {
                    id: node.id.to_string(),
                    name: node.name.clone(),
                    is_master: node.is_master,
                    online: node.is_master || live.contains_key(&node.id),
                    active_connections: live
                        .get(&node.id)
                        .map(|conn| conn.active_connections)
                        .unwrap_or(0),
                })
                .collect(),
            links: links
                .iter()
                .map(|link| {
                    let row = by_link.get(link.id.to_string().as_str());
                    TopologyLinkView {
                        id: link.id.to_string(),
                        from_node_id: link.from_node_id.to_string(),
                        to_node_id: link.to_node_id.to_string(),
                        transport: link.transport.clone(),
                        up_bytes: row.map(|r| r.up_bytes).unwrap_or(0),
                        down_bytes: row.map(|r| r.down_bytes).unwrap_or(0),
                        sessions: row.map(|r| r.sessions).unwrap_or(0),
                    }
                })
                .collect(),
        })
    }
}

fn reaches(links: &[LinkRecord], start: Uuid, goal: Uuid) -> bool {
    let mut stack = vec![start];
    let mut seen = std::collections::HashSet::new();
    while let Some(current) = stack.pop() {
        if current == goal {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        for link in links.iter().filter(|link| link.from_node_id == current) {
            stack.push(link.to_node_id);
        }
    }
    false
}

/// 判断表中是否已有指定列（用于增量迁移）。
async fn column_exists(conn: &Connection, table: &str, column: &str) -> DbResult<bool> {
    let mut rows = conn
        .query(&format!("PRAGMA table_info({table})"), ())
        .await?;
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn admin_from_row(row: &Row) -> DbResult<AdminRecord> {
    let id: String = row.get(0)?;
    let role_raw: String = row.get(3)?;
    Ok(AdminRecord {
        id: Uuid::parse_str(&id).map_err(|e| DbError::Invalid(e.to_string()))?,
        email: row.get(1)?,
        password_hash: row.get(2)?,
        role: Role::parse(&role_raw)
            .ok_or_else(|| DbError::Invalid(format!("bad role: {role_raw}")))?,
        created_at: row.get(4)?,
    })
}

fn node_from_row(row: &Row) -> DbResult<NodeRecord> {
    let id: String = row.get(0)?;
    Ok(NodeRecord {
        id: Uuid::parse_str(&id).map_err(|e| DbError::Invalid(e.to_string()))?,
        name: row.get(1)?,
        status: row.get(2)?,
        version: row.get(3)?,
        http_url: row.get(4)?,
        rpc_url: row.get(5)?,
        is_master: row.get::<i64>(6)? != 0,
        last_seen: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn link_from_row(row: &Row) -> DbResult<LinkRecord> {
    let id: String = row.get(0)?;
    let from: String = row.get(1)?;
    let to: String = row.get(2)?;
    Ok(LinkRecord {
        id: Uuid::parse_str(&id).map_err(|e| DbError::Invalid(e.to_string()))?,
        from_node_id: Uuid::parse_str(&from).map_err(|e| DbError::Invalid(e.to_string()))?,
        to_node_id: Uuid::parse_str(&to).map_err(|e| DbError::Invalid(e.to_string()))?,
        transport: row.get(3)?,
        token: row.get(4)?,
        priority: row.get(5)?,
        enabled: row.get::<i64>(6)? != 0,
        created_at: row.get(7)?,
        from_name: row.get(8)?,
        to_name: row.get(9)?,
        to_http_url: row.get(10)?,
        to_rpc_url: row.get(11)?,
    })
}

fn proxy_user_from_row(row: &Row) -> DbResult<ProxyUserRecord> {
    let id: String = row.get(0)?;
    let owner_id: String = row.get(3)?;
    Ok(ProxyUserRecord {
        id: Uuid::parse_str(&id).map_err(|e| DbError::Invalid(e.to_string()))?,
        name: row.get(1)?,
        enabled: row.get::<i64>(2)? != 0,
        owner_id: Uuid::parse_str(&owner_id).map_err(|e| DbError::Invalid(e.to_string()))?,
        owner_email: row.get(4)?,
        created_at: row.get(5)?,
        up_bytes: row.get::<i64>(6)?.max(0) as u64,
        down_bytes: row.get::<i64>(7)?.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> Db {
        Db::open(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn admin_and_session_roundtrip() {
        let db = db().await;
        let admin = db
            .create_admin("Admin@Example.com", "hash", Role::Admin)
            .await
            .unwrap();
        assert_eq!(admin.email, "admin@example.com");
        assert!(db
            .create_admin("admin@example.com", "x", Role::User)
            .await
            .is_err());

        db.create_session(admin.id, "token", now_ms() + 60_000)
            .await
            .unwrap();
        let found = db.session_admin("token").await.unwrap().unwrap();
        assert_eq!(found.id, admin.id);
        assert!(db.session_admin("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn first_admin_and_subscription_are_consistent() {
        let db = db().await;
        assert!(db
            .create_first_admin_if_empty("Root@Example.com", "hash", Role::Admin)
            .await
            .unwrap()
            .is_some());
        // 已有账号时拒绝重复开户。
        assert!(db
            .create_first_admin_if_empty("other@example.com", "hash", Role::Admin)
            .await
            .unwrap()
            .is_none());

        let admin = db
            .find_admin_by_email("root@example.com")
            .await
            .unwrap()
            .unwrap();
        let (first, second) = tokio::join!(
            db.get_or_create_sub_token(admin.id),
            db.get_or_create_sub_token(admin.id)
        );
        let token = first.unwrap();
        // 并发首次读取必须得到同一个 token。
        assert_eq!(second.unwrap(), token);
        assert_eq!(
            db.admin_by_sub_token(&token).await.unwrap().unwrap().id,
            admin.id
        );
        assert!(db.admin_by_sub_token("").await.unwrap().is_none());
        assert!(db.admin_by_sub_token("missing").await.unwrap().is_none());

        let other = db
            .create_admin("other@example.com", "hash", Role::User)
            .await
            .unwrap();
        let enabled = db.create_proxy_user(admin.id, "enabled").await.unwrap();
        let disabled = db.create_proxy_user(admin.id, "disabled").await.unwrap();
        db.update_proxy_user(disabled.id, None, Some(false))
            .await
            .unwrap();
        let foreign = db.create_proxy_user(other.id, "foreign").await.unwrap();

        let owned = db.enabled_proxy_users_for_owner(admin.id).await.unwrap();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].0, enabled.id);
        assert!(!owned
            .iter()
            .any(|(id, _)| *id == disabled.id || *id == foreign.id));
    }

    #[tokio::test]
    async fn first_admin_creation_is_atomic() {
        let db = db().await;
        let (first, second) = tokio::join!(
            db.create_first_admin_if_empty("a@example.com", "hash", Role::Admin),
            db.create_first_admin_if_empty("b@example.com", "hash", Role::Admin)
        );
        let created = [first.unwrap().is_some(), second.unwrap().is_some()]
            .into_iter()
            .filter(|created| *created)
            .count();
        assert_eq!(created, 1);
        assert_eq!(db.count_admins().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn admin_deletion_is_guarded() {
        let db = db().await;
        let first = db
            .create_admin("first@example.com", "hash", Role::Admin)
            .await
            .unwrap();
        let second = db
            .create_admin("second@example.com", "hash", Role::User)
            .await
            .unwrap();
        let owned = db.create_proxy_user(second.id, "owned").await.unwrap();
        assert_eq!(
            db.delete_admin_checked(second.id).await.unwrap(),
            DeleteAdminOutcome::Deleted(vec![owned.id])
        );
        // 事务内删除的用户不应再存在。
        assert!(db.get_proxy_user(owned.id).await.unwrap().is_none());
        assert_eq!(
            db.delete_admin_checked(first.id).await.unwrap(),
            DeleteAdminOutcome::LastAdmin
        );
        assert!(db.get_admin(first.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn node_user_link_roundtrip() {
        let db = db().await;
        let admin = db.create_admin("a@b.c", "hash", Role::Admin).await.unwrap();
        let (node_a, token_a) = db.create_node("a", false).await.unwrap();
        let (node_b, _) = db.create_node("b", false).await.unwrap();
        assert!(db.verify_node(node_a.id, "wrong").await.unwrap().is_none());
        assert!(db.verify_node(node_a.id, &token_a).await.unwrap().is_some());

        let link = db
            .create_link(node_a.id, node_b.id, "rpc", 10)
            .await
            .unwrap();
        assert_eq!(db.list_links().await.unwrap().len(), 1);
        assert!(db
            .create_link(node_a.id, node_b.id, "rpc", 10)
            .await
            .is_err());
        assert!(db
            .create_link(node_a.id, node_a.id, "rpc", 10)
            .await
            .is_err());
        assert!(db
            .create_link(node_b.id, node_a.id, "rpc", 10)
            .await
            .is_err());
        db.delete_link(link.id).await.unwrap();
        assert!(db.list_links().await.unwrap().is_empty());

        let user = db.create_proxy_user(admin.id, "phone").await.unwrap();
        assert_eq!(db.enabled_proxy_users().await.unwrap().len(), 1);
        db.update_proxy_user(user.id, Some("laptop"), Some(false))
            .await
            .unwrap();
        assert!(db.enabled_proxy_users().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn traffic_accumulates() {
        let db = db().await;
        let admin = db.create_admin("a@b.c", "hash", Role::Admin).await.unwrap();
        let (node, _) = db.create_node("n", false).await.unwrap();
        let user = db.create_proxy_user(admin.id, "u").await.unwrap();
        db.record_traffic(node.id, user.id, 100, 200).await.unwrap();
        db.record_traffic(node.id, user.id, 1, 2).await.unwrap();
        let overview = db.traffic_overview(None).await.unwrap();
        assert_eq!(overview[0].up_bytes, 101);
        assert_eq!(overview[0].down_bytes, 202);
        let stats = db.stats(None).await.unwrap();
        assert_eq!(stats.users_total, 1);
        assert_eq!(stats.up_bytes, 101);
    }

    #[tokio::test]
    async fn link_traffic_accumulates() {
        let db = db().await;
        let (a, _) = db.create_node("a", false).await.unwrap();
        let (b, _) = db.create_node("b", false).await.unwrap();
        let link = db.create_link(a.id, b.id, "rpc", 0).await.unwrap();

        db.record_link_traffic(&link.id.to_string(), 100, 200, 1)
            .await
            .unwrap();
        let rows = db.link_traffic_window(0).await.unwrap();
        assert_eq!(rows[0].up_bytes, 100);
        assert_eq!(rows[0].sessions, 1);
    }

    #[tokio::test]
    async fn reopen_file_database_applies_migrations_once() {
        // 文件库可以真正验证「第二次启动」路径：增量迁移与部分唯一索引必须
        // 都能在已迁移的库上重复执行而不报错。
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("master.db");
        let path = path.to_string_lossy().to_string();

        let db = Db::open(&path).await.unwrap();
        let admin = db
            .create_admin("root@example.com", "hash", Role::Admin)
            .await
            .unwrap();
        let token = db.get_or_create_sub_token(admin.id).await.unwrap();
        drop(db);

        let reopened = Db::open(&path).await.unwrap();
        assert_eq!(reopened.count_admins().await.unwrap(), 1);
        assert_eq!(
            reopened
                .admin_by_sub_token(&token)
                .await
                .unwrap()
                .unwrap()
                .id,
            admin.id
        );
        assert!(
            column_exists(&reopened.connect().unwrap(), "admins", "sub_token")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn master_node_and_settings() {
        let db = db().await;
        let id = Uuid::new_v4();
        db.set_setting("master_node_id", &id.to_string())
            .await
            .unwrap();
        assert_eq!(
            db.get_setting("master_node_id").await.unwrap(),
            Some(id.to_string())
        );
        let node = db
            .ensure_master_node(id, "master", "http://m:8080", "http://m:50051")
            .await
            .unwrap();
        assert!(node.is_master);
        let (edge, _) = db.create_node("edge", false).await.unwrap();
        db.create_link(id, edge.id, "rpc", 0).await.unwrap();
        assert!(!db.delete_node(id).await.unwrap());
        assert_eq!(db.list_links().await.unwrap().len(), 1);
    }
}
