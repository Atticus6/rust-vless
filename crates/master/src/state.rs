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

use crate::access_store::AccessStore;
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
    /// 独立的访问日志文件，查询走 DuckDB。
    pub access: AccessStore,
    /// 未安装时的一次性安装码；安装完成后为 `None`。
    pub setup_token: Option<String>,
    /// 安装码连续校验失败次数；达到上限后拒绝继续尝试。
    pub setup_failures: Mutex<u32>,
    publish_lock: Mutex<()>,
    revision: AtomicI64,
}

impl AppState {
    pub fn new(
        db: Db,
        config: MasterConfig,
        master_node_id: Uuid,
        dataplane: Arc<DataPlaneState>,
        access: AccessStore,
        setup_token: Option<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            config,
            live: RwLock::new(HashMap::new()),
            master_node_id,
            dataplane,
            login_sem: Arc::new(tokio::sync::Semaphore::new(4)),
            access,
            setup_token,
            setup_failures: Mutex::new(0),
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

    /// 向所有在线节点发送当前完整用户快照。
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

    /// 重新计算每个节点的出边/入边并下发。
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

    /// 超时内无法接收快照的节点会被断开，使其重连并重新同步，
    /// 而不是继续使用过期的鉴权数据。
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

    /// 启动后填充主节点自身的数据面。
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

    /// 在持有发布锁的情况下注册节点，避免并发的用户/连线变更插入到
    /// 「初始快照」与「节点对后续发布可见」之间。
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

    /// 仅当实时条目仍属于该连接时才移除节点；过期连接不能驱逐更新的重连。
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
