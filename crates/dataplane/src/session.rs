use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rust_vless_common::registry::UserStats;
use rust_vless_common::util::now_ms;
use rust_vless_common::{Command, ParsedRequest};
use rust_vless_proto::relay_packet::Payload as RelayPayload;
use rust_vless_proto::{RelayPacket, RelayReady};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tonic::Status;

use crate::event::{AccessRecord, DataEvent, LinkTrafficRecord};
use crate::relay::{connect, encode_metadata, OutboundConnection};
use crate::state::DataPlaneState;

const TCP_BUFFER: usize = 32 * 1024;
const UDP_MAX: usize = 65_535 + 2;
const UDP_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub report_traffic: bool,
    pub visited: Vec<String>,
    pub client_addr: String,
    pub session_id: String,
    pub started_at_ms: i64,
}

pub enum SessionMode {
    Entry,
    HttpRelay,
    RpcRelay(mpsc::Sender<Result<RelayPacket, Status>>),
}

#[derive(Default)]
struct SessionStats {
    up: AtomicU64,
    down: AtomicU64,
}

impl SessionStats {
    fn add_up(&self, n: u64) {
        self.up.fetch_add(n, Ordering::Relaxed);
    }

    fn add_down(&self, n: u64) {
        self.down.fetch_add(n, Ordering::Relaxed);
    }

    fn snapshot(&self) -> (u64, u64) {
        (
            self.up.load(Ordering::Relaxed),
            self.down.load(Ordering::Relaxed),
        )
    }
}

struct Outcome {
    status: &'static str,
    error: String,
    path: Vec<String>,
    link_id: Option<String>,
}

pub async fn run_session(
    frame: crate::frame::FrameIo,
    request: ParsedRequest,
    header: Vec<u8>,
    mut ctx: SessionContext,
    state: Arc<DataPlaneState>,
    mode: SessionMode,
) {
    let stats = Arc::new(SessionStats::default());
    let self_id = state.node_id.to_string();

    if ctx.visited.contains(&self_id) || ctx.visited.len() >= state.config.max_hops {
        let path = ctx.visited.clone();
        finish(
            &state,
            &request,
            &ctx,
            &stats,
            Outcome {
                status: "denied",
                error: "relay loop or max hops exceeded".into(),
                path,
                link_id: None,
            },
        )
        .await;
        return;
    }

    let base_path = {
        let mut path = ctx.visited.clone();
        path.push(self_id.clone());
        path
    };
    ctx.visited.push(self_id);

    let outbound = state.outbound().await;
    let mut chosen: Option<(OutboundConnection, String)> = None;
    let mut last_error = String::new();
    let mut header_sent = false;

    for link in &outbound {
        match connect(link, &state, &header, &ctx.visited, ctx.report_traffic).await {
            Ok(conn) => {
                chosen = Some((conn, link.link_id.clone()));
                break;
            }
            Err(err) => {
                header_sent |= err.header_sent;
                tracing::debug!(link = %link.link_id, error = %err.message, "relay hop failed");
                last_error = err.message;
                if err.header_sent {
                    // 下游可能已经开始执行该请求，不能再重试或直连。
                    break;
                }
            }
        }
    }

    let (reader, writer) = frame.split();

    if let Some((conn, link_id)) = chosen {
        stats.add_up(initial_payload_len(&request));
        let path = conn.path.clone();
        if !announce(&path, &mode, &writer).await {
            finish(
                &state,
                &request,
                &ctx,
                &stats,
                Outcome {
                    status: "error",
                    error: "failed to announce relay path".into(),
                    path,
                    link_id: Some(link_id),
                },
            )
            .await;
            return;
        }
        pump_channels(reader, conn.writer, conn.reader, writer, stats.clone()).await;
        finish(
            &state,
            &request,
            &ctx,
            &stats,
            Outcome {
                status: "ok",
                error: String::new(),
                path,
                link_id: Some(link_id),
            },
        )
        .await;
        return;
    }

    if !outbound.is_empty() && (header_sent || !state.config.allow_direct_fallback) {
        finish(
            &state,
            &request,
            &ctx,
            &stats,
            Outcome {
                status: "error",
                error: if last_error.is_empty() {
                    "no relay hop available".into()
                } else {
                    last_error
                },
                path: base_path,
                link_id: None,
            },
        )
        .await;
        return;
    }

    let result = match request.command {
        Command::Tcp => {
            direct_tcp(&request, reader, writer, &state, &stats, &mode, &base_path).await
        }
        Command::Udp => {
            direct_udp(&request, reader, writer, &state, &stats, &mode, &base_path).await
        }
    };
    finish(&state, &request, &ctx, &stats, result).await;
}

fn initial_payload_len(request: &ParsedRequest) -> u64 {
    match request.command {
        Command::Tcp => request.payload.len() as u64,
        Command::Udp => {
            let mut offset = 0usize;
            let mut total = 0u64;
            while offset + 2 <= request.payload.len() {
                let len = u16::from_be_bytes([request.payload[offset], request.payload[offset + 1]])
                    as usize;
                offset += 2;
                if len == 0 {
                    continue;
                }
                if offset + len > request.payload.len() {
                    break;
                }
                total += len as u64;
                offset += len;
            }
            total
        }
    }
}

async fn announce(path: &[String], mode: &SessionMode, writer: &mpsc::Sender<Vec<u8>>) -> bool {
    match mode {
        SessionMode::Entry => true,
        SessionMode::HttpRelay => writer.send(encode_metadata(path)).await.is_ok(),
        SessionMode::RpcRelay(tx) => tx
            .send(Ok(RelayPacket {
                payload: Some(RelayPayload::Ready(RelayReady {
                    path: path.to_vec(),
                })),
            }))
            .await
            .is_ok(),
    }
}

async fn direct_tcp(
    request: &ParsedRequest,
    mut reader: mpsc::Receiver<Vec<u8>>,
    writer: mpsc::Sender<Vec<u8>>,
    state: &Arc<DataPlaneState>,
    stats: &Arc<SessionStats>,
    mode: &SessionMode,
    path: &[String],
) -> Outcome {
    let mut tcp = match timeout(DIAL_TIMEOUT, TcpStream::connect(&request.target)).await {
        Ok(Ok(tcp)) => tcp,
        Ok(Err(err)) => {
            return Outcome {
                status: "error",
                error: format!("dial {}: {err}", request.target),
                path: path.to_vec(),
                link_id: None,
            };
        }
        Err(_) => {
            return Outcome {
                status: "timeout",
                error: format!("dial {}: timeout", request.target),
                path: path.to_vec(),
                link_id: None,
            };
        }
    };
    let _ = tcp.set_nodelay(true);
    if !request.payload.is_empty() {
        if let Err(err) = tcp.write_all(&request.payload).await {
            return Outcome {
                status: "error",
                error: format!("write initial payload: {err}"),
                path: path.to_vec(),
                link_id: None,
            };
        }
        stats.add_up(request.payload.len() as u64);
    }

    if !announce(path, mode, &writer).await {
        return Outcome {
            status: "error",
            error: "failed to announce relay path".into(),
            path: path.to_vec(),
            link_id: None,
        };
    }
    if writer.send(vec![0, 0]).await.is_err() {
        return Outcome {
            status: "error",
            error: "client closed".into(),
            path: path.to_vec(),
            link_id: None,
        };
    }

    let (mut tcp_rx, mut tcp_tx) = tcp.into_split();
    let up_stats = stats.clone();
    let down_stats = stats.clone();
    let down_writer = writer.clone();

    let up = async move {
        while let Some(data) = reader.recv().await {
            if tcp_tx.write_all(&data).await.is_err() {
                break;
            }
            up_stats.add_up(data.len() as u64);
        }
    };
    let down = async move {
        let mut buf = vec![0u8; TCP_BUFFER];
        loop {
            match tcp_rx.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    down_stats.add_down(n as u64);
                    if down_writer.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }

    let _ = state;
    Outcome {
        status: "ok",
        error: String::new(),
        path: path.to_vec(),
        link_id: None,
    }
}

async fn direct_udp(
    request: &ParsedRequest,
    mut reader: mpsc::Receiver<Vec<u8>>,
    writer: mpsc::Sender<Vec<u8>>,
    state: &Arc<DataPlaneState>,
    stats: &Arc<SessionStats>,
    mode: &SessionMode,
    path: &[String],
) -> Outcome {
    let bind_addr = if request.target.starts_with('[') {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let udp = match UdpSocket::bind(bind_addr).await {
        Ok(udp) => Arc::new(udp),
        Err(err) => {
            return Outcome {
                status: "error",
                error: format!("udp bind: {err}"),
                path: path.to_vec(),
                link_id: None,
            };
        }
    };
    if let Err(err) = udp.connect(&request.target).await {
        return Outcome {
            status: "error",
            error: format!("udp connect {}: {err}", request.target),
            path: path.to_vec(),
            link_id: None,
        };
    }

    if !announce(path, mode, &writer).await {
        return Outcome {
            status: "error",
            error: "failed to announce relay path".into(),
            path: path.to_vec(),
            link_id: None,
        };
    }
    if writer.send(vec![0, 0]).await.is_err() {
        return Outcome {
            status: "error",
            error: "client closed".into(),
            path: path.to_vec(),
            link_id: None,
        };
    }
    if !request.payload.is_empty() {
        forward_udp_payload(&udp, &request.payload, Some(stats.as_ref())).await;
    }

    let up_udp = udp.clone();
    let up_stats = stats.clone();
    let down_udp = udp.clone();
    let down_stats = stats.clone();
    let down_writer = writer.clone();
    let last_activity = Arc::new(AtomicU64::new(now_ms() as u64));
    let up_activity = last_activity.clone();
    let down_activity = last_activity.clone();

    let up = async move {
        while let Some(data) = reader.recv().await {
            up_activity.store(now_ms() as u64, Ordering::Relaxed);
            forward_udp_payload(&up_udp, &data, Some(up_stats.as_ref())).await;
        }
    };
    let down = async move {
        let mut buf = vec![0u8; UDP_MAX];
        let mut idle = tokio::time::interval(Duration::from_secs(10));
        idle.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = idle.tick() => {
                    let last = down_activity.load(Ordering::Relaxed) as i64;
                    if now_ms() - last >= UDP_IDLE_TIMEOUT.as_millis() as i64 {
                        break;
                    }
                }
                result = down_udp.recv(&mut buf[2..]) => {
                    match result {
                        Ok(0) => continue,
                        Ok(n) => {
                            down_activity.store(now_ms() as u64, Ordering::Relaxed);
                            down_stats.add_down(n as u64);
                            buf[..2].copy_from_slice(&(n as u16).to_be_bytes());
                            if down_writer.send(buf[..n + 2].to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }

    let _ = state;
    Outcome {
        status: "ok",
        error: String::new(),
        path: path.to_vec(),
        link_id: None,
    }
}

async fn pump_channels(
    mut up_reader: mpsc::Receiver<Vec<u8>>,
    up_writer: mpsc::Sender<Vec<u8>>,
    mut down_reader: mpsc::Receiver<Vec<u8>>,
    down_writer: mpsc::Sender<Vec<u8>>,
    stats: Arc<SessionStats>,
) {
    let up_stats = stats.clone();
    let up = async move {
        while let Some(data) = up_reader.recv().await {
            up_stats.add_up(data.len() as u64);
            if up_writer.send(data).await.is_err() {
                break;
            }
        }
    };
    let down = async move {
        while let Some(data) = down_reader.recv().await {
            stats.add_down(data.len() as u64);
            if down_writer.send(data).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
}

async fn forward_udp_payload(udp: &UdpSocket, data: &[u8], stats: Option<&SessionStats>) {
    let mut offset = 0usize;
    while offset + 2 <= data.len() {
        let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if len == 0 {
            continue;
        }
        if offset + len > data.len() {
            tracing::debug!(len, remaining = data.len() - offset, "UDP 帧被截断");
            break;
        }
        match udp.send(&data[offset..offset + len]).await {
            Ok(n) => {
                if let Some(stats) = stats {
                    stats.add_up(n as u64);
                }
            }
            Err(_) => break,
        }
        offset += len;
    }
}

async fn finish(
    state: &Arc<DataPlaneState>,
    request: &ParsedRequest,
    ctx: &SessionContext,
    stats: &Arc<SessionStats>,
    outcome: Outcome,
) {
    let (up, down) = stats.snapshot();

    if let Some(link_id) = &outcome.link_id {
        state.emit(DataEvent::Link(LinkTrafficRecord {
            link_id: link_id.clone(),
            up_bytes: up,
            down_bytes: down,
            sessions: 1,
        }));
    }

    if !ctx.report_traffic {
        return;
    }

    if let Some(user_stats) = state.users.stats_for(request.user_id) {
        add_user_stats(&user_stats, up, down);
    }

    state.emit(DataEvent::Access(AccessRecord {
        session_id: ctx.session_id.clone(),
        user_id: request.user_id.to_string(),
        entry_node_id: state.node_id.to_string(),
        target: request.target.clone(),
        command: match request.command {
            Command::Tcp => "tcp".into(),
            Command::Udp => "udp".into(),
        },
        started_at_ms: ctx.started_at_ms,
        ended_at_ms: rust_vless_common::util::now_ms(),
        up_bytes: up,
        down_bytes: down,
        status: outcome.status.into(),
        error: outcome.error,
        path: outcome.path,
        client_addr: ctx.client_addr.clone(),
    }));
}

fn add_user_stats(stats: &UserStats, up: u64, down: u64) {
    stats.add_up(up);
    stats.add_down(down);
}
