use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use rust_vless_proto::{
    ConfigUpdate, InboundLink, MasterMessage, OutboundLink, TopologyUpdate, VlessUser,
};
use std::time::Duration;

use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;

use rust_vless_dataplane::DataPlaneState;

use crate::config::MasterConfig;
use crate::db::{Db, DbError, LinkRecord};

#[derive(Debug, Clone)]
pub struct LiveNode {
    pub conn_id: Uuid,
    pub version: String,
    pub active_connections: u64,
    pub uptime_sec: u64,
    pub connected_at: i64,
    pub tx: mpsc::Sender<MasterMessage>,
}

pub struct AppState {
    pub db: Db,
    pub config: MasterConfig,
    pub live: RwLock<HashMap<Uuid, LiveNode>>,
    pub master_node_id: Uuid,
    pub dataplane: Arc<DataPlaneState>,
    pub login_sem: Arc<tokio::sync::Semaphore>,
    publish_lock: Mutex<()>,
    revision: AtomicI64,
}

impl AppState {
    pub fn new(
        db: Db,
        config: MasterConfig,
        master_node_id: Uuid,
        dataplane: Arc<DataPlaneState>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            config,
            live: RwLock::new(HashMap::new()),
            master_node_id,
            dataplane,
            login_sem: Arc::new(tokio::sync::Semaphore::new(4)),
            publish_lock: Mutex::new(()),
            revision: AtomicI64::new(0),
        })
    }

    pub async fn build_config(&self) -> Result<ConfigUpdate, DbError> {
        let users = self.db.enabled_proxy_users().await?;
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(ConfigUpdate {
            users: users
                .into_iter()
                .map(|(id, name)| VlessUser {
                    id: id.to_string(),
                    name,
                    enabled: true,
                })
                .collect(),
            revision,
        })
    }

    /// Send the current full user snapshot to every connected node.
    pub async fn publish_config(&self) {
        let _guard = self.publish_lock.lock().await;
        let config = match self.build_config().await {
            Ok(config) => config,
            Err(err) => {
                tracing::error!(error = %err, "failed to build node config");
                return;
            }
        };
        let ids = config
            .users
            .iter()
            .filter_map(|user| user.id.parse::<Uuid>().ok());
        self.dataplane.users.replace_all(ids);
        let nodes: Vec<(Uuid, Uuid, mpsc::Sender<MasterMessage>)> = self
            .live
            .read()
            .await
            .iter()
            .map(|(id, node)| (*id, node.conn_id, node.tx.clone()))
            .collect();
        for (id, conn_id, tx) in nodes {
            let message = MasterMessage {
                payload: Some(rust_vless_proto::master_message::Payload::Config(
                    config.clone(),
                )),
            };
            let delivered = matches!(
                tokio::time::timeout(Duration::from_secs(1), tx.send(message)).await,
                Ok(Ok(()))
            );
            if !delivered {
                self.drop_stalled_node(id, conn_id, "config").await;
            }
        }
    }

    /// Recompute each node's outbound/inbound links and push it.
    pub async fn publish_topology(&self) {
        let _guard = self.publish_lock.lock().await;
        let links = match self.db.list_links().await {
            Ok(links) => links,
            Err(err) => {
                tracing::error!(error = %err, "failed to load links");
                return;
            }
        };
        self.dataplane
            .apply_topology(self.topology_for(self.master_node_id, &links))
            .await;
        let nodes: Vec<(Uuid, Uuid, mpsc::Sender<MasterMessage>)> = self
            .live
            .read()
            .await
            .iter()
            .map(|(id, node)| (*id, node.conn_id, node.tx.clone()))
            .collect();
        for (id, conn_id, tx) in nodes {
            let message = MasterMessage {
                payload: Some(rust_vless_proto::master_message::Payload::Topology(
                    self.topology_for(id, &links),
                )),
            };
            let delivered = matches!(
                tokio::time::timeout(Duration::from_secs(1), tx.send(message)).await,
                Ok(Ok(()))
            );
            if !delivered {
                self.drop_stalled_node(id, conn_id, "topology").await;
            }
        }
    }

    /// A node that cannot accept a snapshot within the timeout is disconnected
    /// so it reconnects and resynchronizes instead of serving stale auth data.
    async fn drop_stalled_node(&self, id: Uuid, conn_id: Uuid, kind: &str) {
        if self.set_offline_if(id, conn_id).await {
            tracing::warn!(%id, kind, "node stalled; disconnecting for resync");
            if let Err(err) = self.db.mark_node_offline(id).await {
                tracing::warn!(%id, error = %err, "failed to mark stalled node offline");
            }
        }
    }

    pub fn topology_for(&self, node_id: Uuid, links: &[LinkRecord]) -> TopologyUpdate {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let outbound = links
            .iter()
            .filter(|link| link.enabled && link.from_node_id == node_id)
            .map(|link| OutboundLink {
                link_id: link.id.to_string(),
                to_node_id: link.to_node_id.to_string(),
                to_name: link.to_name.clone(),
                transport: link.transport.clone(),
                endpoint: if link.transport == "http" {
                    link.to_http_url.clone()
                } else {
                    link.to_rpc_url.clone()
                },
                token: link.token.clone(),
                priority: link.priority,
            })
            .collect();
        let inbound = links
            .iter()
            .filter(|link| link.enabled && link.to_node_id == node_id)
            .map(|link| InboundLink {
                link_id: link.id.to_string(),
                from_node_id: link.from_node_id.to_string(),
                token: link.token.clone(),
                transport: link.transport.clone(),
            })
            .collect();
        TopologyUpdate {
            outbound,
            inbound,
            revision,
        }
    }

    pub async fn topology_for_node(&self, node_id: Uuid) -> Result<TopologyUpdate, DbError> {
        let links = self.db.list_links().await?;
        Ok(self.topology_for(node_id, &links))
    }

    /// Populate the master's own data plane after startup.
    pub async fn refresh_dataplane(&self) -> Result<(), DbError> {
        let config = self.build_config().await?;
        let ids = config
            .users
            .iter()
            .filter_map(|user| user.id.parse::<Uuid>().ok());
        self.dataplane.users.replace_all(ids);
        let links = self.db.list_links().await?;
        self.dataplane
            .apply_topology(self.topology_for(self.master_node_id, &links))
            .await;
        Ok(())
    }

    /// Register a node while holding the publish lock, so a concurrent
    /// user/link change cannot slip between the initial snapshot and the
    /// node becoming visible to future publishes.
    pub async fn register_node(&self, id: Uuid, node: LiveNode) -> Result<(), DbError> {
        let _guard = self.publish_lock.lock().await;
        let config = self.build_config().await?;
        let topology = self.topology_for_node(id).await?;
        let tx = node.tx.clone();
        tx.send(MasterMessage {
            payload: Some(rust_vless_proto::master_message::Payload::Config(config)),
        })
        .await
        .map_err(|_| DbError::Invalid("node stream closed".into()))?;
        tx.send(MasterMessage {
            payload: Some(rust_vless_proto::master_message::Payload::Topology(
                topology,
            )),
        })
        .await
        .map_err(|_| DbError::Invalid("node stream closed".into()))?;
        self.live.write().await.insert(id, node);
        Ok(())
    }

    pub async fn set_offline(&self, id: Uuid) {
        self.live.write().await.remove(&id);
    }

    /// Remove a node only if the live entry still belongs to this connection.
    /// A stale stream must not evict a newer reconnection.
    pub async fn set_offline_if(&self, id: Uuid, conn_id: Uuid) -> bool {
        let mut live = self.live.write().await;
        if live.get(&id).map(|node| node.conn_id) == Some(conn_id) {
            live.remove(&id);
            true
        } else {
            false
        }
    }

    pub async fn live_snapshot(&self) -> HashMap<Uuid, LiveNode> {
        self.live.read().await.clone()
    }

    pub async fn active_connections(&self) -> u64 {
        self.live
            .read()
            .await
            .values()
            .map(|node| node.active_connections)
            .sum()
    }
}
