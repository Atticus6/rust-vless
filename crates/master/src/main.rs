mod api;
mod auth;
mod config;
mod db;
mod error;
mod rpc;
mod service;
mod state;
#[cfg(test)]
mod test_support;
mod web_rpc;

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use rust_vless_common::UserRegistry;
use rust_vless_dataplane::{DataEvent, DataPlaneConfig, DataPlaneState, RelayService};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::config::MasterConfig;
use crate::db::{AccessLogInput, Db, Role};
use crate::rpc::NodeControlService;
use crate::state::AppState;
use rust_vless_proto::relay_server::RelayServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = MasterConfig::parse();
    let db = Db::open(&config.db).await?;

    if db.count_admins().await? == 0 {
        let (password, generated) = match config.admin_password.as_deref() {
            Some(password) if !password.is_empty() => (password.to_string(), false),
            _ => (rust_vless_common::util::random_hex(16), true),
        };
        let hash = auth::hash_password(&password)?;
        let admin = db
            .create_admin(&config.admin_email, &hash, Role::Admin)
            .await?;
        if generated {
            tracing::warn!(
                email = %admin.email,
                password = %password,
                "bootstrap administrator created with a generated password; save it now"
            );
        } else {
            tracing::warn!(email = %admin.email, "bootstrap administrator created");
        }
    }

    spawn_retention_task(db.clone(), config.access_log_retention_days);

    let master_node_id = resolve_master_node_id(&db, &config).await?;
    db.ensure_master_node(
        master_node_id,
        &config.master_node_name,
        &config.master_public_http_url,
        &config.master_public_rpc_url,
    )
    .await?;
    db.update_node_heartbeat(
        master_node_id,
        env!("CARGO_PKG_VERSION"),
        &config.master_public_http_url,
        &config.master_public_rpc_url,
    )
    .await?;

    let (event_tx, event_rx) = tokio::sync::mpsc::channel(1024);
    let plane = DataPlaneState::new(
        master_node_id,
        config.master_node_name.clone(),
        Arc::new(UserRegistry::new()),
        config.max_conn,
        event_tx,
        DataPlaneConfig {
            max_hops: config.max_hops,
            allow_direct_fallback: config.allow_direct_fallback,
        },
    );
    let event_db = db.clone();
    tokio::spawn(async move { persist_events(event_db, event_rx).await });

    let state = AppState::new(db, config.clone(), master_node_id, plane.clone());
    state.refresh_dataplane().await?;

    let http_listener = tokio::net::TcpListener::bind(&config.http_addr).await?;
    let http = axum::serve(http_listener, api::router(state.clone()));
    let grpc_addr: SocketAddr = config.grpc_addr.parse()?;

    tracing::info!(addr = %config.http_addr, web = %config.web_dir, "master HTTP listening");
    tracing::info!(addr = %config.grpc_addr, "master gRPC listening");

    let grpc = tonic::transport::Server::builder()
        .add_service(
            rust_vless_proto::node_control_server::NodeControlServer::new(NodeControlService::new(
                state.clone(),
            )),
        )
        .add_service(RelayServer::new(RelayService::new(plane.clone())))
        .serve(grpc_addr);

    tokio::select! {
        result = grpc => result.map_err(|err| anyhow::anyhow!(err))?,
        result = http => result?,
        _ = tokio::signal::ctrl_c() => tracing::info!("shutdown signal received"),
    }

    Ok(())
}

fn spawn_retention_task(db: Db, retention_days: i64) {
    let retention_days = retention_days.max(1);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(86_400));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let cutoff = rust_vless_common::util::now_ms() - retention_days * 86_400_000;
            if let Err(err) = db.prune_access_logs(cutoff).await {
                tracing::warn!(error = %err, "failed to prune access logs");
            }
            if let Err(err) = db.prune_traffic_samples(cutoff).await {
                tracing::warn!(error = %err, "failed to prune traffic samples");
            }
            if let Err(err) = db.prune_link_traffic_samples(cutoff).await {
                tracing::warn!(error = %err, "failed to prune link traffic samples");
            }
        }
    });
}

async fn persist_events(db: Db, mut rx: tokio::sync::mpsc::Receiver<DataEvent>) {
    let mut pending: std::collections::VecDeque<DataEvent> = std::collections::VecDeque::new();
    loop {
        let event = match pending.pop_front() {
            Some(event) => event,
            None => match rx.recv().await {
                Some(event) => event,
                None => break,
            },
        };

        if let Err(err) = persist_event(&db, &event).await {
            tracing::warn!(error = %err, "failed to persist data event; retrying");
            if pending.len() < 1024 {
                pending.push_back(event);
            } else {
                tracing::warn!("data event retry queue full; dropping event");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

async fn persist_event(db: &Db, event: &DataEvent) -> Result<(), crate::db::DbError> {
    match event {
        DataEvent::Access(record) => {
            let log = AccessLogInput {
                session_id: record.session_id.clone(),
                user_id: record.user_id.clone(),
                entry_node_id: record.entry_node_id.clone(),
                target: record.target.clone(),
                command: record.command.clone(),
                started_at_ms: record.started_at_ms,
                ended_at_ms: record.ended_at_ms,
                up_bytes: record.up_bytes,
                down_bytes: record.down_bytes,
                status: record.status.clone(),
                error: record.error.clone(),
                path: record.path.clone(),
                client_addr: record.client_addr.clone(),
            };
            db.insert_access_logs(&[log]).await?;
            if let (Ok(user_id), Ok(node_id)) = (
                Uuid::parse_str(&record.user_id),
                Uuid::parse_str(&record.entry_node_id),
            ) {
                db.record_traffic(node_id, user_id, record.up_bytes, record.down_bytes)
                    .await?;
            }
        }
        DataEvent::Link(record) => {
            db.record_link_traffic(
                &record.link_id,
                record.up_bytes,
                record.down_bytes,
                record.sessions,
            )
            .await?;
        }
    }
    Ok(())
}

async fn resolve_master_node_id(db: &Db, config: &MasterConfig) -> anyhow::Result<uuid::Uuid> {
    if let Some(raw) = &config.master_node_id {
        return Ok(uuid::Uuid::parse_str(raw)?);
    }
    const KEY: &str = "master_node_id";
    if let Some(raw) = db.get_setting(KEY).await? {
        return Ok(uuid::Uuid::parse_str(&raw)?);
    }
    let id = uuid::Uuid::new_v4();
    db.set_setting(KEY, &id.to_string()).await?;
    tracing::info!(%id, "generated stable master node id");
    Ok(id)
}
