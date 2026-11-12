use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "rust-vless-master",
    version,
    about = "rust-vless control plane"
)]
pub struct MasterConfig {
    /// Turso/SQLite 数据库文件（测试可用 ":memory:"）。
    #[arg(long, env = "RUST_VLESS_DB", default_value = "master.db")]
    pub db: String,

    /// HTTP/Web UI 监听地址。
    #[arg(long, env = "RUST_VLESS_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: String,

    /// 子节点使用的 gRPC 监听地址。
    #[arg(long, env = "RUST_VLESS_GRPC_ADDR", default_value = "0.0.0.0:50051")]
    pub grpc_addr: String,

    /// 前端构建产物目录（index.html 与 assets）。
    #[arg(long, env = "RUST_VLESS_WEB_DIR", default_value = "web/dist")]
    pub web_dir: String,

    /// 自动创建管理员时使用的邮箱（仅在 admins 表为空时生效）。
    #[arg(
        long,
        env = "RUST_VLESS_ADMIN_EMAIL",
        default_value = "admin@example.com"
    )]
    pub admin_email: String,

    /// 首次启动时自动创建管理员用的密码。
    /// 留空则不做任何自动创建，浏览器首次访问会进入安装页面。
    #[arg(long, env = "RUST_VLESS_ADMIN_PASSWORD")]
    pub admin_password: Option<String>,

    /// 首次安装码。未设置且没有任何管理员时，主节点会生成随机值并打印到
    /// 启动日志；安装页面必须提交该值。设置后请通过安全渠道交给部署者。
    #[arg(long, env = "RUST_VLESS_SETUP_TOKEN")]
    pub setup_token: Option<String>,

    /// 登录会话有效期（秒，默认 7 天）。
    #[arg(long, env = "RUST_VLESS_SESSION_TTL_SECS", default_value_t = 604_800)]
    pub session_ttl_secs: i64,

    /// 固定的主节点 ID；未设置时自动生成并保存到 settings。
    #[arg(long, env = "RUST_VLESS_MASTER_NODE_ID")]
    pub master_node_id: Option<String>,

    /// 主节点在拓扑中的显示名。
    #[arg(long, env = "RUST_VLESS_MASTER_NODE_NAME", default_value = "master")]
    pub master_node_name: String,

    /// 主节点公网 HTTP 地址（供订阅链接使用）。
    #[arg(long, env = "RUST_VLESS_MASTER_PUBLIC_HTTP_URL", default_value = "")]
    pub master_public_http_url: String,

    /// 主节点公网 RPC 地址（供其他节点建立中转连线）。
    #[arg(long, env = "RUST_VLESS_MASTER_PUBLIC_RPC_URL", default_value = "")]
    pub master_public_rpc_url: String,

    /// 访问日志保留天数。
    #[arg(
        long,
        env = "RUST_VLESS_ACCESS_LOG_RETENTION_DAYS",
        default_value_t = 30
    )]
    pub access_log_retention_days: i64,

    /// 访问日志文件（JSON Lines）。与主数据库分离，便于用 DuckDB 分析。
    #[arg(
        long,
        env = "RUST_VLESS_ACCESS_LOG_FILE",
        default_value = "access_logs.jsonl"
    )]
    pub access_log_file: String,

    /// 对外公布的最新版本号；不设置时等于 `version.txt` 中的版本。
    /// 管理员据此判断哪些节点需要升级。
    #[arg(long, env = "RUST_VLESS_LATEST_VERSION")]
    pub latest_version: Option<String>,

    /// 主节点数据面允许的最大并发 VLESS 会话数。
    #[arg(long, env = "RUST_VLESS_MAX_CONN", default_value_t = 4096)]
    pub max_conn: usize,

    /// 会话被拒绝前的最大中转跳数。
    #[arg(long, env = "RUST_VLESS_MAX_HOPS", default_value_t = 8)]
    pub max_hops: usize,

    /// 所有出边都失败时是否回退为直连。
    #[arg(
        long,
        env = "RUST_VLESS_ALLOW_DIRECT_FALLBACK",
        default_value_t = true,
        action = clap::ArgAction::Set
    )]
    pub allow_direct_fallback: bool,
}
