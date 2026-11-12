use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(name = "rust-vless-node", version, about = "rust-vless child node")]
pub struct NodeConfig {
    /// Master gRPC endpoint. `http://` is added when the scheme is omitted.
    #[arg(
        long,
        env = "RUST_VLESS_MASTER_GRPC",
        default_value = "http://127.0.0.1:50051"
    )]
    pub master_grpc: String,

    /// Node UUID created in the master web UI.
    #[arg(long, env = "RUST_VLESS_NODE_ID")]
    pub node_id: String,

    /// Node token shown once when the node was created.
    #[arg(long, env = "RUST_VLESS_NODE_TOKEN")]
    pub node_token: String,

    /// Display name advertised to the master.
    #[arg(long, env = "RUST_VLESS_NODE_NAME", default_value = "node")]
    pub node_name: String,

    /// VLESS WebSocket listen address.
    #[arg(long, env = "RUST_VLESS_NODE_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: String,

    /// gRPC relay listen address (all nodes serve Relay).
    #[arg(long, env = "RUST_VLESS_RPC_ADDR", default_value = "0.0.0.0:50051")]
    pub rpc_addr: String,

    /// Public HTTP URL advertised to other nodes for HTTP relay links.
    #[arg(long, env = "RUST_VLESS_PUBLIC_HTTP_URL", default_value = "")]
    pub public_http_url: String,

    /// Public RPC URL advertised to other nodes for RPC relay links.
    #[arg(long, env = "RUST_VLESS_PUBLIC_RPC_URL", default_value = "")]
    pub public_rpc_url: String,

    /// Maximum concurrent VLESS sessions.
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
