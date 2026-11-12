use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use rust_vless_common::util::now_ms;
use rust_vless_proto::master_message::Payload as MasterPayload;
use rust_vless_proto::node_control_server::NodeControl;
use rust_vless_proto::node_message::Payload as NodePayload;
use rust_vless_proto::{Accepted, MasterMessage, NodeMessage};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::db::AccessLogInput;
use crate::state::{AppState, LiveNode};

pub struct NodeControlService {
    state: Arc<AppState>,
}

impl NodeControlService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl NodeControl for NodeControlService {
    type StreamStream = Pin<Box<dyn Stream<Item = Result<MasterMessage, Status>> + Send + 'static>>;

    async fn stream(
        &self,
        request: Request<Streaming<NodeMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let mut inbound = request.into_inner();

        let hello = match inbound.message().await.map_err(status_internal)? {
            Some(NodeMessage {
                payload: Some(NodePayload::Hello(hello)),
            }) => hello,
            Some(_) => return Err(Status::invalid_argument("first node message must be Hello")),
            None => return Err(Status::invalid_argument("empty node stream")),
        };

        let node_id = Uuid::parse_str(&hello.node_id)
            .map_err(|_| Status::invalid_argument("invalid node id"))?;
        let node = self
            .state
            .db
            .verify_node(node_id, &hello.token)
            .await
            .map_err(status_internal)?
            .ok_or_else(|| Status::unauthenticated("invalid node credentials"))?;

        let conn_id = Uuid::new_v4();
        let version = hello.version.clone();
        let http_url = hello.http_url.clone();
        let rpc_url = hello.rpc_url.clone();
        let now = now_ms();

        let (tx, rx) = mpsc::channel::<MasterMessage>(128);

        tx.send(MasterMessage {
            payload: Some(MasterPayload::Accepted(Accepted {
                server_time_ms: now,
                node_name: node.name.clone(),
                heartbeat_interval_sec: 10,
            })),
        })
        .await
        .map_err(|_| Status::unavailable("node stream closed"))?;

        self.state
            .register_node(
                node_id,
                LiveNode {
                    conn_id,
                    version: version.clone(),
                    active_connections: 0,
                    uptime_sec: 0,
                    connected_at: now,
                    tx,
                },
            )
            .await
            .map_err(status_internal)?;
        if let Err(err) = self
            .state
            .db
            .update_node_heartbeat(node_id, &version, &http_url, &rpc_url)
            .await
        {
            tracing::warn!(%node_id, error = %err, "failed to persist node online state");
        }
        tracing::info!(%node_id, name = %node.name, "node connected");

        let inbound_state = self.state.clone();
        tokio::spawn(async move {
            while let Some(message) = inbound.next().await {
                match message {
                    Ok(NodeMessage {
                        payload: Some(NodePayload::Heartbeat(heartbeat)),
                    }) => {
                        let mut matched = false;
                        if let Some(node) = inbound_state.live.write().await.get_mut(&node_id) {
                            if node.conn_id == conn_id {
                                node.active_connections = heartbeat.active_connections;
                                node.uptime_sec = heartbeat.uptime_sec;
                                matched = true;
                            }
                        }
                        if !matched {
                            tracing::debug!(%node_id, "ignoring heartbeat from stale connection");
                            continue;
                        }
                        if let Err(err) = inbound_state
                            .db
                            .update_node_heartbeat(node_id, &version, &http_url, &rpc_url)
                            .await
                        {
                            tracing::warn!(%node_id, error = %err, "failed to persist heartbeat");
                        }
                    }
                    Ok(NodeMessage {
                        payload: Some(NodePayload::Traffic(report)),
                    }) => {
                        for user in report.users {
                            let Ok(user_id) = Uuid::parse_str(&user.user_id) else {
                                tracing::warn!(node_id = %node_id, user = %user.user_id, "invalid traffic user id");
                                continue;
                            };
                            if let Err(err) = inbound_state
                                .db
                                .record_traffic(node_id, user_id, user.up_bytes, user.down_bytes)
                                .await
                            {
                                tracing::warn!(%node_id, %user_id, error = %err, "failed to record traffic");
                            }
                        }
                    }
                    Ok(NodeMessage {
                        payload: Some(NodePayload::AccessLogs(batch)),
                    }) => {
                        let logs: Vec<AccessLogInput> = batch
                            .logs
                            .into_iter()
                            .map(|log| AccessLogInput {
                                session_id: log.session_id,
                                user_id: log.user_id,
                                entry_node_id: node_id.to_string(),
                                target: log.target,
                                command: log.command,
                                started_at_ms: log.started_at_ms,
                                ended_at_ms: log.ended_at_ms,
                                up_bytes: log.up_bytes,
                                down_bytes: log.down_bytes,
                                status: log.status,
                                error: log.error,
                                path: log.path,
                                client_addr: log.client_addr,
                            })
                            .collect();
                        if let Err(err) = inbound_state.db.insert_access_logs(&logs).await {
                            tracing::warn!(%node_id, error = %err, "failed to store access logs");
                        }
                    }
                    Ok(NodeMessage {
                        payload: Some(NodePayload::LinkTraffic(report)),
                    }) => {
                        for link in report.links {
                            if let Err(err) = inbound_state
                                .db
                                .record_link_traffic(
                                    &link.link_id,
                                    link.up_bytes,
                                    link.down_bytes,
                                    link.sessions,
                                )
                                .await
                            {
                                tracing::warn!(%node_id, link = %link.link_id, error = %err, "failed to record link traffic");
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(%node_id, error = %err, "node stream error");
                        break;
                    }
                }
            }

            if inbound_state.set_offline_if(node_id, conn_id).await {
                if let Err(err) = inbound_state.db.mark_node_offline(node_id).await {
                    tracing::warn!(%node_id, error = %err, "failed to mark node offline");
                }
                tracing::info!(%node_id, "node disconnected");
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx).map(Ok))))
    }
}

fn status_internal(err: impl std::fmt::Display) -> Status {
    Status::internal(err.to_string())
}
