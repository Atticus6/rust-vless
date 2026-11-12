/// 入口会话结束时产生的事件；只有入口节点会记录。
#[derive(Debug, Clone)]
pub struct AccessRecord {
    pub session_id: String,
    pub user_id: String,
    pub entry_node_id: String,
    pub target: String,
    pub command: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub status: String,
    pub error: String,
    pub path: Vec<String>,
    pub client_addr: String,
}

/// 通过出边转发会话的每个节点都会产生该事件。
#[derive(Debug, Clone)]
pub struct LinkTrafficRecord {
    pub link_id: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub sessions: u64,
}

#[derive(Debug, Clone)]
pub enum DataEvent {
    Access(AccessRecord),
    Link(LinkTrafficRecord),
}
