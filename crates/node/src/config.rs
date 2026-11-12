use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "rust-vless-node", version, about = "rust-vless child node")]
pub struct NodeConfig {
    /// 主节点 gRPC 地址；省略 scheme 时自动补 `http://`。
    #[arg(
        long,
        env = "RUST_VLESS_MASTER_GRPC",
        default_value = "http://127.0.0.1:50051"
    )]
    pub master_grpc: String,

    /// 在 Web 管理端创建节点时得到的 UUID。
    #[arg(long, env = "RUST_VLESS_NODE_ID")]
    pub node_id: String,

    /// 创建节点时只显示一次的 token。
    #[arg(long, env = "RUST_VLESS_NODE_TOKEN")]
    pub node_token: String,

    /// 上报给主节点的显示名。
    #[arg(long, env = "RUST_VLESS_NODE_NAME", default_value = "node")]
    pub node_name: String,

    /// VLESS WebSocket 监听地址。
    #[arg(long, env = "RUST_VLESS_NODE_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: String,

    /// gRPC 中继监听地址（所有节点都提供 Relay）。
    #[arg(long, env = "RUST_VLESS_RPC_ADDR", default_value = "0.0.0.0:50051")]
    pub rpc_addr: String,

    /// 供其他节点 HTTP 中转使用的公网 HTTP 地址。
    #[arg(long, env = "RUST_VLESS_PUBLIC_HTTP_URL", default_value = "")]
    pub public_http_url: String,

    /// 供其他节点 RPC 中转使用的公网 RPC 地址。
    #[arg(long, env = "RUST_VLESS_PUBLIC_RPC_URL", default_value = "")]
    pub public_rpc_url: String,

    /// 最大并发 VLESS 会话数。
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

    /// 主节点下发升级指令时执行的命令。目标版本通过环境变量
    /// `RUST_VLESS_TARGET_VERSION` 传入；一键安装脚本会把它设置为自带的
    /// 更新脚本。未配置时节点会拒绝升级并回报原因。
    ///
    /// 安全提示：命令通过 `sh -c` / `cmd /C` 以节点进程权限执行，只允许受信任
    /// 的运维人员设置；修改它等同于允许主节点在该用户权限下执行任意命令。
    #[arg(long, env = "RUST_VLESS_UPGRADE_COMMAND")]
    pub upgrade_command: Option<String>,
}
