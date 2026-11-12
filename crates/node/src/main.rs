mod config;
mod rpc;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use rust_vless_common::UserRegistry;
use rust_vless_dataplane::{DataPlaneConfig, DataPlaneState, RelayService};
use rust_vless_proto::relay_server::RelayServer;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::NodeConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = NodeConfig::parse();
    let node_id = Uuid::parse_str(&config.node_id).context("invalid RUST_VLESS_NODE_ID")?;
    let (event_tx, event_rx) = tokio::sync::mpsc::channel(1024);
    let plane = DataPlaneState::new(
        node_id,
        config.node_name.clone(),
        Arc::new(UserRegistry::new()),
        config.max_conn,
        event_tx,
        DataPlaneConfig {
            max_hops: config.max_hops,
            allow_direct_fallback: config.allow_direct_fallback,
        },
    );

    let http_listener = tokio::net::TcpListener::bind(&config.listen).await?;
    let http = axum::serve(http_listener, rust_vless_dataplane::router(plane.clone()));
    let rpc_addr: SocketAddr = config.rpc_addr.parse()?;
    let relay = tonic::transport::Server::builder()
        .add_service(RelayServer::new(RelayService::new(plane.clone())))
        .serve(rpc_addr);

    tracing::info!(
        listen = %config.listen,
        rpc = %config.rpc_addr,
        master = %config.master_grpc,
        node_id = %config.node_id,
        max_conn = config.max_conn,
        "rust-vless node listening"
    );

    let control = rpc::run(config.clone(), plane.clone(), event_rx);

    tokio::select! {
        result = http => result?,
        result = relay => result.map_err(|err| anyhow::anyhow!(err))?,
        result = control => result?,
        _ = tokio::signal::ctrl_c() => tracing::info!("shutdown signal received"),
    }

    Ok(())
}
