use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "rust-vless-master",
    version,
    about = "rust-vless control plane"
)]
pub struct MasterConfig {
    /// Turso/SQLite database file (use ":memory:" for tests).
    #[arg(long, env = "RUST_VLESS_DB", default_value = "master.db")]
    pub db: String,

    /// HTTP/Web UI listen address.
    #[arg(long, env = "RUST_VLESS_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: String,

    /// gRPC listen address for child nodes.
    #[arg(long, env = "RUST_VLESS_GRPC_ADDR", default_value = "0.0.0.0:50051")]
    pub grpc_addr: String,

    /// Directory containing the built frontend (index.html + assets).
    #[arg(long, env = "RUST_VLESS_WEB_DIR", default_value = "web/dist")]
    pub web_dir: String,

    /// Bootstrap administrator email (only used when the admins table is empty).
    #[arg(
        long,
        env = "RUST_VLESS_ADMIN_EMAIL",
        default_value = "admin@example.com"
    )]
    pub admin_email: String,

    /// Bootstrap administrator password. When omitted on first startup a
    /// random password is generated and printed once.
    #[arg(long, env = "RUST_VLESS_ADMIN_PASSWORD")]
    pub admin_password: Option<String>,

    /// Session lifetime in seconds (default 7 days).
    #[arg(long, env = "RUST_VLESS_SESSION_TTL_SECS", default_value_t = 604_800)]
    pub session_ttl_secs: i64,

    /// Stable master node id. Generated and stored in settings when omitted.
    #[arg(long, env = "RUST_VLESS_MASTER_NODE_ID")]
    pub master_node_id: Option<String>,

    /// Master display name in the topology.
    #[arg(long, env = "RUST_VLESS_MASTER_NODE_NAME", default_value = "master")]
    pub master_node_name: String,

    /// Public HTTP URL of the master node (used by child link endpoints).
    #[arg(long, env = "RUST_VLESS_MASTER_PUBLIC_HTTP_URL", default_value = "")]
    pub master_public_http_url: String,

    /// Public RPC URL of the master node (used by child link endpoints).
    #[arg(long, env = "RUST_VLESS_MASTER_PUBLIC_RPC_URL", default_value = "")]
    pub master_public_rpc_url: String,

    /// Access log retention in days.
    #[arg(
        long,
        env = "RUST_VLESS_ACCESS_LOG_RETENTION_DAYS",
        default_value_t = 30
    )]
    pub access_log_retention_days: i64,

    /// Maximum concurrent VLESS sessions served by the master data plane.
    #[arg(long, env = "RUST_VLESS_MAX_CONN", default_value_t = 4096)]
    pub max_conn: usize,

    /// Maximum relay hops before a session is rejected.
    #[arg(long, env = "RUST_VLESS_MAX_HOPS", default_value_t = 8)]
    pub max_hops: usize,

    /// Fall back to a direct dial when all outbound links fail.
    #[arg(
        long,
        env = "RUST_VLESS_ALLOW_DIRECT_FALLBACK",
        default_value_t = true,
        action = clap::ArgAction::Set
    )]
    pub allow_direct_fallback: bool,
}
