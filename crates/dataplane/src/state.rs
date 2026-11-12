use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rust_vless_common::UserRegistry;
use rust_vless_proto::{InboundLink, OutboundLink, TopologyUpdate};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::event::DataEvent;

#[derive(Debug, Clone)]
pub struct DataPlaneConfig {
    pub max_hops: usize,
    pub allow_direct_fallback: bool,
}

#[derive(Debug, Clone)]
pub struct Outbound {
    pub link_id: String,
    pub to_node_id: Uuid,
    pub to_name: String,
    pub transport: String,
    pub endpoint: String,
    pub token: String,
    pub priority: i64,
}

#[derive(Debug, Clone)]
pub struct Inbound {
    pub link_id: String,
    pub from_node_id: Uuid,
    pub token: String,
    pub transport: String,
}

pub struct DataPlaneState {
    pub node_id: Uuid,
    pub node_name: String,
    pub users: Arc<UserRegistry>,
    pub sem: Arc<tokio::sync::Semaphore>,
    pub events: mpsc::Sender<DataEvent>,
    pub started_at: Instant,
    pub config: DataPlaneConfig,
    outbound: RwLock<Vec<Outbound>>,
    inbound: RwLock<Vec<Inbound>>,
    active: AtomicU64,
}

impl DataPlaneState {
    pub fn new(
        node_id: Uuid,
        node_name: String,
        users: Arc<UserRegistry>,
        max_conns: usize,
        events: mpsc::Sender<DataEvent>,
        config: DataPlaneConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            node_id,
            node_name,
            users,
            sem: Arc::new(tokio::sync::Semaphore::new(max_conns.max(1))),
            events,
            started_at: Instant::now(),
            config,
            outbound: RwLock::new(Vec::new()),
            inbound: RwLock::new(Vec::new()),
            active: AtomicU64::new(0),
        })
    }

    pub async fn apply_topology(&self, update: TopologyUpdate) {
        let mut outbound: Vec<Outbound> = update
            .outbound
            .into_iter()
            .map(outbound_from_proto)
            .collect();
        outbound.sort_by_key(|link| link.priority);
        let inbound: Vec<Inbound> = update.inbound.into_iter().map(inbound_from_proto).collect();
        *self.outbound.write().await = outbound;
        *self.inbound.write().await = inbound;
    }

    pub async fn outbound(&self) -> Vec<Outbound> {
        self.outbound.read().await.clone()
    }

    pub async fn inbound_match(
        &self,
        token: &str,
        from_node_id: Uuid,
        transport: &str,
    ) -> Option<Inbound> {
        self.inbound
            .read()
            .await
            .iter()
            .find(|link| {
                link.token == token
                    && link.from_node_id == from_node_id
                    && link.transport == transport
            })
            .cloned()
    }

    pub fn active_connections(&self) -> u64 {
        self.active.load(Ordering::Relaxed)
    }

    pub fn active_guard(self: &Arc<Self>) -> ActiveGuard {
        self.active.fetch_add(1, Ordering::Relaxed);
        ActiveGuard {
            state: self.clone(),
        }
    }

    /// Non-blocking event emission. A slow or disconnected master must never
    /// block a proxy session or exhaust connection permits.
    pub fn emit(&self, event: DataEvent) {
        if let Err(err) = self.events.try_send(event) {
            match err {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!("data event channel full; dropping event")
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    tracing::debug!("data event channel closed")
                }
            }
        }
    }
}

fn outbound_from_proto(link: OutboundLink) -> Outbound {
    Outbound {
        link_id: link.link_id,
        to_node_id: Uuid::parse_str(&link.to_node_id).unwrap_or(Uuid::nil()),
        to_name: link.to_name,
        transport: link.transport,
        endpoint: link.endpoint,
        token: link.token,
        priority: link.priority,
    }
}

fn inbound_from_proto(link: InboundLink) -> Inbound {
    Inbound {
        link_id: link.link_id,
        from_node_id: Uuid::parse_str(&link.from_node_id).unwrap_or(Uuid::nil()),
        token: link.token,
        transport: link.transport,
    }
}

pub struct ActiveGuard {
    state: Arc<DataPlaneState>,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.state.active.fetch_sub(1, Ordering::Relaxed);
    }
}
