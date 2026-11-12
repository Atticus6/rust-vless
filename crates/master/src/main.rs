mod access_store;
mod api;
mod auth;
mod config;
mod db;
mod error;
mod rpc;
mod service;
mod state;
mod subscription;
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

use crate::access_store::AccessStore;
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
        match config
            .admin_password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            Some(password) => {
                let hash = auth::hash_password(password)?;
                // 与 Web 安装页面并发时，只有一个请求能真正创建首个管理员。
                if let Some(admin) = db
                    .create_first_admin_if_empty(&config.admin_email, &hash, Role::Admin)
                    .await?
                {
                    tracing::warn!(
                        email = %admin.email,
                        "已通过 RUST_VLESS_ADMIN_PASSWORD 创建初始管理员"
                    );
                }
            }
            None => {
                tracing::warn!(
                    "尚无管理员账号：请在浏览器打开主节点地址，使用启动日志中的安装码完成首次安装"
                );
            }
        }
    }

    // 未安装时生成一次性安装码，避免远程访问者抢先创建管理员。
    // 未安装时生成一次性安装码，避免远程访问者抢先创建管理员。
    // random_hex 的参数是字节数，16 字节 = 32 个十六进制字符 = 128 bit 熵。
    let setup_token = if db.count_admins().await? == 0 {
        let token = config
            .setup_token
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| rust_vless_common::util::random_hex(16));
        tracing::warn!(setup_token = %token, "首次安装码（只在启动日志中出现）");
        Some(token)
    } else {
        None
    };

    // 访问日志写入独立文件；旧数据库中的历史记录按自增 ID 游标分批迁移。
    // 使用 ID 而不是行偏移：保留期任务删除旧行后，偏移会错位漏数据。
    let access = AccessStore::new(&config.access_log_file);
    if db.get_setting("access_log_migrated").await?.as_deref() != Some("1") {
        const CURSOR_KEY: &str = "access_log_migrated_id";
        let mut last_id: i64 = db
            .get_setting(CURSOR_KEY)
            .await?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if last_id > 0 {
            tracing::info!(last_id, "从上次中断处继续迁移历史访问日志");
        }
        loop {
            match db.access_logs_batch_after(last_id, 500).await {
                Ok(batch) if batch.is_empty() => {
                    // 只有读完整张表才写完成标记，避免「文件非空」被误当成迁移完成。
                    match db.set_setting("access_log_migrated", "1").await {
                        Ok(()) => tracing::info!(
                            last_id,
                            file = %config.access_log_file,
                            "访问日志已迁移到独立文件"
                        ),
                        Err(err) => tracing::warn!(error = %err, "记录访问日志迁移标记失败"),
                    }
                    break;
                }
                Ok(batch) => {
                    let logs: Vec<AccessLogInput> =
                        batch.iter().map(|(_, log)| log.clone()).collect();
                    if let Err(err) = access.append_many(&logs).await {
                        tracing::warn!(
                            error = %err,
                            last_id,
                            "迁移批次写入失败，下次启动将从该 ID 继续"
                        );
                        break;
                    }
                    // 批次按 id 升序，最后一条即新游标。
                    if let Some((new_last, _)) = batch.last() {
                        last_id = *new_last;
                    }
                    if let Err(err) = db.set_setting(CURSOR_KEY, &last_id.to_string()).await {
                        // 游标未保存时重试可能重复写入；查询按 session_id 去重。
                        tracing::warn!(error = %err, "保存访问日志迁移游标失败");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, last_id, "读取历史访问日志失败");
                    break;
                }
            }
        }
    }

    if let Err(err) = retry_log_purge(&db, &access).await {
        tracing::warn!(error = %err, "启动时重试访问日志清理失败");
    }

    spawn_retention_task(db.clone(), access.clone(), config.access_log_retention_days);
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
        rust_vless_common::version(),
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
    let event_access = access.clone();
    tokio::spawn(async move { persist_events(event_db, event_access, event_rx).await });

    let state = AppState::new(
        db,
        config.clone(),
        master_node_id,
        plane.clone(),
        access,
        setup_token,
    );
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

/// 重试账号删除时未能完成的访问日志清理。
async fn retry_log_purge(db: &Db, access: &AccessStore) -> anyhow::Result<()> {
    let pending = db.list_pending_log_purges().await?;
    if pending.is_empty() {
        return Ok(());
    }
    access.remove_users(&pending).await?;
    // 只确认本次成功处理过的 ID，避免清掉等待期间新登记的任务。
    for user_id in &pending {
        db.clear_log_purge(user_id).await?;
    }
    tracing::info!(count = pending.len(), "已重试清理访问日志");
    Ok(())
}

fn spawn_retention_task(db: Db, access: AccessStore, retention_days: i64) {
    let retention_days = retention_days.max(1);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(86_400));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            // 重试之前失败的访问日志清理。
            if let Err(err) = retry_log_purge(&db, &access).await {
                tracing::warn!(error = %err, "重试访问日志清理队列失败");
            }
            let cutoff = rust_vless_common::util::now_ms() - retention_days * 86_400_000;
            if let Err(err) = access.prune(cutoff).await {
                tracing::warn!(error = %err, "清理访问日志文件失败");
            }
            // 旧版本把日志写在主数据库里；这里顺便清理遗留表。
            if let Err(err) = db.prune_access_logs(cutoff).await {
                tracing::warn!(error = %err, "failed to prune legacy access logs");
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

async fn persist_events(
    db: Db,
    access: AccessStore,
    mut rx: tokio::sync::mpsc::Receiver<DataEvent>,
) {
    let mut pending: std::collections::VecDeque<(DataEvent, bool, u32)> =
        std::collections::VecDeque::new();
    loop {
        let (event, mut traffic_done, attempts) = match pending.pop_front() {
            Some(entry) => entry,
            None => match rx.recv().await {
                Some(event) => (event, false, 0),
                None => break,
            },
        };

        if let Err(err) = persist_event(&db, &access, &event, &mut traffic_done).await {
            // 持续失败的单个事件不能无限占用消费循环；重试若干次后丢弃并告警，
            // 让后续流量/连线事件继续持久化。
            if attempts >= 4 {
                tracing::warn!(error = %err, "data event dropped after retries");
                continue;
            }
            tracing::warn!(error = %err, attempts, "failed to persist data event; retrying");
            if pending.len() < 1024 {
                pending.push_back((event, traffic_done, attempts + 1));
            } else {
                tracing::warn!("data event retry queue full; dropping event");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

async fn persist_event(
    db: &Db,
    access: &AccessStore,
    event: &DataEvent,
    traffic_done: &mut bool,
) -> anyhow::Result<()> {
    match event {
        DataEvent::Access(record) => {
            // 租约保证：删除流程标记用户时必须等待流量写入结束；
            // 已删除用户返回 None，延迟事件不会再写回数据。
            let Some(active) = access.user_guard(&record.user_id).await else {
                return Ok(());
            };
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
            // 先写流量汇总，再追加日志文件；`traffic_done` 让重试跳过已成功的
            // 阶段，避免一次数据库抖动产生重复日志或重复流量。
            if !*traffic_done {
                if let (Ok(user_id), Ok(node_id)) = (
                    Uuid::parse_str(&record.user_id),
                    Uuid::parse_str(&record.entry_node_id),
                ) {
                    db.record_traffic(node_id, user_id, record.up_bytes, record.down_bytes)
                        .await?;
                }
                *traffic_done = true;
            }
            // 租约保持到日志写入完成：否则删除流程可能在两步之间完成标记与清理，
            // 随后这次 append 又把该用户的日志写回文件。
            access.append(&log).await?;
            drop(active);
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
