use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message as AxumMessage, WebSocket};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rust_vless_proto::relay_packet::Payload as RelayPayload;
use rust_vless_proto::RelayPacket;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tonic::{Status, Streaming};

const CHANNEL_SIZE: usize = 64;
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// 与传输方式无关的字节流。WebSocket、HTTP 中转与 RPC 中转都被归一成两个
/// 通道，会话层因此不需要关心底层协议。
pub struct FrameIo {
    pub reader: mpsc::Receiver<Vec<u8>>,
    pub writer: mpsc::Sender<Vec<u8>>,
}

impl FrameIo {
    pub fn split(self) -> (mpsc::Receiver<Vec<u8>>, mpsc::Sender<Vec<u8>>) {
        (self.reader, self.writer)
    }
}

pub fn from_axum_ws(socket: WebSocket) -> FrameIo {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, in_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(message) = stream.next().await {
            match message {
                Ok(AxumMessage::Binary(data)) => {
                    if in_tx.send(data.to_vec()).await.is_err() {
                        break;
                    }
                }
                Ok(AxumMessage::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(CHANNEL_SIZE);
    tokio::spawn(async move {
        let mut ping = interval(PING_INTERVAL);
        ping.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ping.tick() => {
                    if sink.send(AxumMessage::Ping(Bytes::new())).await.is_err() {
                        break;
                    }
                }
                data = out_rx.recv() => {
                    match data {
                        Some(data) => {
                            if sink.send(AxumMessage::Binary(data.into())).await.is_err() {
                                break;
                            }
                        }
                        None => {
                            let _ = sink.send(AxumMessage::Close(None)).await;
                            break;
                        }
                    }
                }
            }
        }
    });

    FrameIo {
        reader: in_rx,
        writer: out_tx,
    }
}

pub fn from_client_ws(socket: WebSocketStream<MaybeTlsStream<TcpStream>>) -> FrameIo {
    let (sink, stream) = socket.split();
    from_client_ws_parts(sink, stream)
}

pub fn from_client_ws_parts(
    mut sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, ClientMessage>,
    mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
) -> FrameIo {
    let (in_tx, in_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(message) = stream.next().await {
            match message {
                Ok(ClientMessage::Binary(data)) => {
                    if in_tx.send(data.to_vec()).await.is_err() {
                        break;
                    }
                }
                Ok(ClientMessage::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(CHANNEL_SIZE);
    tokio::spawn(async move {
        let mut ping = interval(PING_INTERVAL);
        ping.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ping.tick() => {
                    if sink.send(ClientMessage::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
                data = out_rx.recv() => {
                    match data {
                        Some(data) => {
                            if sink.send(ClientMessage::Binary(data.into())).await.is_err() {
                                break;
                            }
                        }
                        None => {
                            let _ = sink.send(ClientMessage::Close(None)).await;
                            break;
                        }
                    }
                }
            }
        }
    });

    FrameIo {
        reader: in_rx,
        writer: out_tx,
    }
}

/// RPC 中转客户端方向：请求包为 `RelayPacket`，响应包拆成原始数据帧。
pub fn from_relay_client(
    stream: Streaming<RelayPacket>,
    out_tx: mpsc::Sender<RelayPacket>,
) -> FrameIo {
    let mut stream = stream;
    let (in_tx, in_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(packet) = stream.next().await {
            match packet {
                Ok(RelayPacket {
                    payload: Some(RelayPayload::Data(data)),
                }) => {
                    if in_tx.send(data).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let (data_tx, mut data_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(data) = data_rx.recv().await {
            if out_tx
                .send(RelayPacket {
                    payload: Some(RelayPayload::Data(data)),
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    FrameIo {
        reader: in_rx,
        writer: data_tx,
    }
}

/// RPC 中转服务端方向：响应包为 `Result<RelayPacket, Status>`。
pub fn from_relay_server(
    stream: Streaming<RelayPacket>,
    out_tx: mpsc::Sender<Result<RelayPacket, Status>>,
) -> FrameIo {
    let mut stream = stream;
    let (in_tx, in_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(packet) = stream.next().await {
            match packet {
                Ok(RelayPacket {
                    payload: Some(RelayPayload::Data(data)),
                }) => {
                    if in_tx.send(data).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let (data_tx, mut data_rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move {
        while let Some(data) = data_rx.recv().await {
            if out_tx
                .send(Ok(RelayPacket {
                    payload: Some(RelayPayload::Data(data)),
                }))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    FrameIo {
        reader: in_rx,
        writer: data_tx,
    }
}
