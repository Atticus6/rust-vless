use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use rust_vless_common::util::now_ms;
use rust_vless_dataplane::{AccessRecord, DataEvent, DataPlaneState};
use rust_vless_proto::master_message::Payload as MasterPayload;
use rust_vless_proto::node_control_client::NodeControlClient;
use rust_vless_proto::node_message::Payload as NodePayload;
use rust_vless_proto::{
    AccessLog, AccessLogBatch, Heartbeat, Hello, LinkTraffic, LinkTrafficReport, MasterMessage,
    NodeMessage, TrafficReport, UserTraffic,
};
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use crate::config::NodeConfig;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const TRAFFIC_INTERVAL: Duration = Duration::from_secs(10);
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const ACCESS_BATCH: usize = 100;

pub async fn run(
    config: NodeConfig,
    plane: Arc<DataPlaneState>,
    mut events: mpsc::Receiver<DataEvent>,
) -> anyhow::Result<()> {
    let node_id = Uuid::parse_str(&config.node_id).context("invalid RUST_VLESS_NODE_ID")?;
    let mut access_buffer: Vec<AccessLog> = Vec::new();
    let mut link_buffer: HashMap<String, LinkTraffic> = HashMap::new();

    loop {
        match connect_once(
            &config,
            &plane,
            node_id,
            &mut events,
            &mut access_buffer,
            &mut link_buffer,
        )
        .await
        {
            Ok(()) => tracing::info!("master stream ended; reconnecting"),
            Err(err) => {
                tracing::warn!(error = %format!("{err:#}"), "master connection failed; retrying")
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn connect_once(
    config: &NodeConfig,
    plane: &Arc<DataPlaneState>,
    node_id: Uuid,
    events: &mut mpsc::Receiver<DataEvent>,
    access_buffer: &mut Vec<AccessLog>,
    link_buffer: &mut HashMap<String, LinkTraffic>,
) -> anyhow::Result<()> {
    let endpoint = normalize_endpoint(&config.master_grpc);
    let mut client = NodeControlClient::connect(endpoint.clone())
        .await
        .with_context(|| format!("connect {endpoint}"))?;

    let (tx, rx) = mpsc::channel::<NodeMessage>(256);
    let hello = NodeMessage {
        payload: Some(NodePayload::Hello(Hello {
            node_id: node_id.to_string(),
            token: config.node_token.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            http_url: config.public_http_url.clone(),
            rpc_url: config.public_rpc_url.clone(),
        })),
    };
    tx.send(hello)
        .await
        .map_err(|_| anyhow::anyhow!("rpc channel closed"))?;

    let response = client
        .stream(ReceiverStream::new(rx))
        .await
        .with_context(|| format!("open node stream to {endpoint}"))?;
    let mut inbound = response.into_inner();

    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut traffic = interval(TRAFFIC_INTERVAL);
    traffic.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut flush = interval(FLUSH_INTERVAL);
    flush.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let message = NodeMessage {
                    payload: Some(NodePayload::Heartbeat(Heartbeat {
                        timestamp_ms: now_ms(),
                        active_connections: plane.active_connections(),
                        uptime_sec: plane.started_at.elapsed().as_secs(),
                        cpu_percent: 0,
                        memory_bytes: 0,
                    })),
                };
                if tx.send(message).await.is_err() {
                    break;
                }
            }
            _ = traffic.tick() => {
                let taken = plane.users.take_all();
                if taken.is_empty() {
                    continue;
                }
                let users = taken
                    .iter()
                    .map(|(id, up, down)| UserTraffic {
                        user_id: id.to_string(),
                        up_bytes: *up,
                        down_bytes: *down,
                    })
                    .collect();
                let message = NodeMessage {
                    payload: Some(NodePayload::Traffic(TrafficReport { users })),
                };
                if tx.send(message).await.is_err() {
                    for (id, up, down) in taken {
                        if let Some(stats) = plane.users.stats_for(id) {
                            stats.add_up(up);
                            stats.add_down(down);
                        }
                    }
                    break;
                }
            }
            _ = flush.tick() => {
                flush_access(&tx, access_buffer).await;
                flush_links(&tx, link_buffer).await;
            }
            event = events.recv() => {
                let Some(event) = event else { break };
                match event {
                    DataEvent::Access(record) => {
                        access_buffer.push(access_to_proto(record));
                        if access_buffer.len() >= ACCESS_BATCH {
                            flush_access(&tx, access_buffer).await;
                        }
                    }
                    DataEvent::Link(record) => {
                        let link_id = record.link_id;
                        let entry = link_buffer.entry(link_id.clone()).or_insert(LinkTraffic {
                            link_id,
                            up_bytes: 0,
                            down_bytes: 0,
                            sessions: 0,
                        });
                        entry.up_bytes += record.up_bytes;
                        entry.down_bytes += record.down_bytes;
                        entry.sessions += record.sessions;
                    }
                }
            }
            message = inbound.message() => {
                match message {
                    Ok(Some(MasterMessage { payload: Some(MasterPayload::Config(config)) })) => {
                        let ids = config
                            .users
                            .iter()
                            .filter(|user| user.enabled)
                            .filter_map(|user| Uuid::parse_str(&user.id).ok());
                        let (added, removed, total) = plane.users.replace_all(ids);
                        tracing::info!(
                            added,
                            removed,
                            total,
                            revision = config.revision,
                            "user config updated"
                        );
                    }
                    Ok(Some(MasterMessage { payload: Some(MasterPayload::Topology(topology)) })) => {
                        let outbound = topology.outbound.len();
                        let inbound = topology.inbound.len();
                        plane.apply_topology(topology).await;
                        tracing::info!(outbound, inbound, "topology updated");
                    }
                    Ok(Some(MasterMessage { payload: Some(MasterPayload::Accepted(accepted)) })) => {
                        tracing::info!(
                            node = %accepted.node_name,
                            heartbeat_interval_sec = accepted.heartbeat_interval_sec,
                            "master accepted node"
                        );
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(err) => {
                        tracing::warn!(error = %err, "rpc stream error");
                        break;
                    }
                }
            }
        }
    }

    // Keep buffers in `run` so the next successful connection flushes them.
    // Sending into a stream that just ended could otherwise clear them
    // without the master ever receiving the data.
    Ok(())
}

async fn flush_access(tx: &mpsc::Sender<NodeMessage>, buffer: &mut Vec<AccessLog>) {
    if buffer.is_empty() {
        return;
    }
    let logs = buffer.clone();
    let message = NodeMessage {
        payload: Some(NodePayload::AccessLogs(AccessLogBatch { logs })),
    };
    if tx.send(message).await.is_ok() {
        buffer.clear();
    }
}

async fn flush_links(tx: &mpsc::Sender<NodeMessage>, buffer: &mut HashMap<String, LinkTraffic>) {
    if buffer.is_empty() {
        return;
    }
    let links: Vec<LinkTraffic> = buffer
        .iter()
        .map(|(link_id, traffic)| LinkTraffic {
            link_id: link_id.clone(),
            up_bytes: traffic.up_bytes,
            down_bytes: traffic.down_bytes,
            sessions: traffic.sessions,
        })
        .collect();
    let message = NodeMessage {
        payload: Some(NodePayload::LinkTraffic(LinkTrafficReport { links })),
    };
    if tx.send(message).await.is_ok() {
        buffer.clear();
    }
}

fn access_to_proto(record: AccessRecord) -> AccessLog {
    AccessLog {
        session_id: record.session_id,
        user_id: record.user_id,
        entry_node_id: record.entry_node_id,
        target: record.target,
        command: record.command,
        started_at_ms: record.started_at_ms,
        ended_at_ms: record.ended_at_ms,
        up_bytes: record.up_bytes,
        down_bytes: record.down_bytes,
        status: record.status,
        error: record.error,
        path: record.path,
        client_addr: record.client_addr,
    }
}

fn normalize_endpoint(raw: &str) -> String {
    if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_gets_scheme() {
        assert_eq!(
            normalize_endpoint("127.0.0.1:50051"),
            "http://127.0.0.1:50051"
        );
        assert_eq!(
            normalize_endpoint("http://master:50051"),
            "http://master:50051"
        );
    }
}
