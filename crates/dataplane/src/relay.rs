use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use rust_vless_common::parse_request;
use rust_vless_common::util::now_ms;
use rust_vless_proto::relay_client::RelayClient;
use rust_vless_proto::relay_packet::Payload as RelayPayload;
use rust_vless_proto::relay_server::Relay as RelayServiceTrait;
use rust_vless_proto::{RelayHello, RelayPacket};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration as TokioDuration};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as ClientMessage;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::frame::{from_client_ws_parts, from_relay_client, from_relay_server};
use crate::session::{run_session, SessionContext, SessionMode};
use crate::state::{DataPlaneState, Outbound};

const DIAL_TIMEOUT: Duration = Duration::from_secs(5);
const READY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct OutboundConnection {
    pub reader: mpsc::Receiver<Vec<u8>>,
    pub writer: mpsc::Sender<Vec<u8>>,
    pub path: Vec<String>,
}

#[derive(Debug)]
pub struct RelayError {
    pub message: String,
    /// 下游可能已经收到 VLESS 头。一旦为 true，就不能再改用直连重放会话。
    pub header_sent: bool,
}

impl RelayError {
    fn not_sent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            header_sent: false,
        }
    }

    fn maybe_sent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            header_sent: true,
        }
    }
}

pub async fn connect(
    link: &Outbound,
    state: &Arc<DataPlaneState>,
    header: &[u8],
    visited: &[String],
    report_traffic: bool,
) -> Result<OutboundConnection, RelayError> {
    if link.transport == "http" {
        connect_http(link, state, header, visited, report_traffic).await
    } else {
        connect_rpc(link, state, header, visited, report_traffic).await
    }
}

async fn connect_rpc(
    link: &Outbound,
    state: &Arc<DataPlaneState>,
    header: &[u8],
    visited: &[String],
    report_traffic: bool,
) -> Result<OutboundConnection, RelayError> {
    let endpoint = normalize_rpc_endpoint(&link.endpoint);
    let mut client = timeout(DIAL_TIMEOUT, RelayClient::connect(endpoint.clone()))
        .await
        .map_err(|_| RelayError::not_sent(format!("connect {endpoint}: timeout")))?
        .map_err(|err| RelayError::not_sent(format!("connect {endpoint}: {err}")))?;

    let (out_tx, out_rx) = mpsc::channel::<RelayPacket>(64);
    let hello = RelayPacket {
        payload: Some(RelayPayload::Hello(RelayHello {
            link_token: link.token.clone(),
            from_node_id: state.node_id.to_string(),
            vless_header: header.to_vec(),
            visited: visited.to_vec(),
            report_traffic,
        })),
    };
    out_tx
        .send(hello)
        .await
        .map_err(|_| RelayError::not_sent("relay channel closed"))?;

    let response = timeout(
        TokioDuration::from_secs(5),
        client.forward(ReceiverStream::new(out_rx)),
    )
    .await
    .map_err(|_| RelayError::maybe_sent(format!("relay {endpoint}: timeout")))?
    .map_err(|err| RelayError::maybe_sent(format!("relay {endpoint}: {err}")))?;
    let mut inbound = response.into_inner();

    let path = loop {
        match timeout(READY_TIMEOUT, inbound.message())
            .await
            .map_err(|_| RelayError::maybe_sent("relay ready: timeout"))?
            .map_err(|err| RelayError::maybe_sent(format!("relay ready: {err}")))?
        {
            Some(RelayPacket {
                payload: Some(RelayPayload::Ready(ready)),
            }) => break ready.path,
            Some(RelayPacket {
                payload: Some(RelayPayload::Data(_)),
            }) => {
                // 中转节点不应在 Ready 之前发送数据；忽略异常数据帧。
            }
            Some(_) => {}
            None => return Err(RelayError::maybe_sent("relay closed before ready")),
        }
    };

    let frame = from_relay_client(inbound, out_tx);
    let (reader, writer) = frame.split();
    Ok(OutboundConnection {
        reader,
        writer,
        path,
    })
}

async fn connect_http(
    link: &Outbound,
    state: &Arc<DataPlaneState>,
    header: &[u8],
    visited: &[String],
    report_traffic: bool,
) -> Result<OutboundConnection, RelayError> {
    let url = http_to_ws(&link.endpoint);
    let mut request = url
        .clone()
        .into_client_request()
        .map_err(|err| RelayError::not_sent(format!("build relay request {url}: {err}")))?;
    let headers = request.headers_mut();
    headers.insert(
        "X-Rust-Vless-Link-Token",
        HeaderValue::from_str(&link.token).map_err(|err| RelayError::not_sent(err.to_string()))?,
    );
    headers.insert(
        "X-Rust-Vless-From",
        HeaderValue::from_str(&state.node_id.to_string())
            .map_err(|err| RelayError::not_sent(err.to_string()))?,
    );
    headers.insert(
        "X-Rust-Vless-Report-Traffic",
        HeaderValue::from_static(if report_traffic { "1" } else { "0" }),
    );
    if !visited.is_empty() {
        headers.insert(
            "X-Rust-Vless-Visited",
            HeaderValue::from_str(&visited.join(","))
                .map_err(|err| RelayError::not_sent(err.to_string()))?,
        );
    }

    let (ws, _) = timeout(DIAL_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| RelayError::not_sent(format!("connect {url}: timeout")))?
        .map_err(|err| RelayError::not_sent(format!("connect {url}: {err}")))?;
    let (mut sink, mut stream) = ws.split();
    timeout(
        READY_TIMEOUT,
        sink.send(ClientMessage::Binary(header.to_vec().into())),
    )
    .await
    .map_err(|_| RelayError::maybe_sent("send relay header: timeout"))?
    .map_err(|err| RelayError::maybe_sent(format!("send relay header: {err}")))?;

    let metadata = loop {
        match timeout(READY_TIMEOUT, stream.next())
            .await
            .map_err(|_| RelayError::maybe_sent("relay metadata: timeout"))?
        {
            Some(Ok(ClientMessage::Binary(data))) => break data.to_vec(),
            Some(Ok(ClientMessage::Close(_))) | None => {
                return Err(RelayError::maybe_sent("relay closed before metadata"))
            }
            Some(Err(err)) => return Err(RelayError::maybe_sent(format!("relay metadata: {err}"))),
            Some(Ok(_)) => continue,
        }
    };
    let path = decode_metadata(&metadata)
        .ok_or_else(|| RelayError::maybe_sent("invalid relay metadata"))?;
    let frame = from_client_ws_parts(sink, stream);
    let (reader, writer) = frame.split();
    Ok(OutboundConnection {
        reader,
        writer,
        path,
    })
}

fn normalize_rpc_endpoint(raw: &str) -> String {
    if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    }
}

fn http_to_ws(raw: &str) -> String {
    let mut base = raw.trim_end_matches('/').to_string();
    if let Some(rest) = base.strip_prefix("http://") {
        base = format!("ws://{rest}");
    } else if let Some(rest) = base.strip_prefix("https://") {
        base = format!("wss://{rest}");
    } else if !base.contains("://") {
        base = format!("ws://{base}");
    }
    if !base.ends_with("/ws") {
        base.push_str("/ws");
    }
    base
}

pub(crate) fn encode_metadata(path: &[String]) -> Vec<u8> {
    let mut out = b"RVL1".to_vec();
    out.extend_from_slice(&serde_json::to_vec(path).unwrap_or_default());
    out
}

pub(crate) fn decode_metadata(data: &[u8]) -> Option<Vec<String>> {
    if !data.starts_with(b"RVL1") {
        return None;
    }
    serde_json::from_slice(&data[4..]).ok()
}

pub struct RelayService {
    state: Arc<DataPlaneState>,
}

impl RelayService {
    pub fn new(state: Arc<DataPlaneState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl RelayServiceTrait for RelayService {
    type ForwardStream = Pin<Box<dyn Stream<Item = Result<RelayPacket, Status>> + Send + 'static>>;

    async fn forward(
        &self,
        request: Request<Streaming<RelayPacket>>,
    ) -> Result<Response<Self::ForwardStream>, Status> {
        let client_addr = request
            .remote_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default();
        let mut inbound = request.into_inner();

        let hello = match timeout(READY_TIMEOUT, inbound.message())
            .await
            .map_err(|_| Status::deadline_exceeded("relay hello timeout"))?
            .map_err(|err| Status::internal(err.to_string()))?
        {
            Some(RelayPacket {
                payload: Some(RelayPayload::Hello(hello)),
            }) => hello,
            _ => {
                return Err(Status::invalid_argument(
                    "first relay packet must be RelayHello",
                ))
            }
        };

        let from_id = Uuid::parse_str(&hello.from_node_id)
            .map_err(|_| Status::invalid_argument("invalid from_node_id"))?;
        if self
            .state
            .inbound_match(&hello.link_token, from_id, "rpc")
            .await
            .is_none()
        {
            return Err(Status::permission_denied("invalid relay link token"));
        }

        let parsed = parse_request(&hello.vless_header, &self.state.users)
            .map_err(|err| Status::permission_denied(format!("invalid VLESS header: {err}")))?;
        let self_id = self.state.node_id.to_string();
        if hello.visited.contains(&self_id) || hello.visited.len() >= self.state.config.max_hops {
            return Err(Status::failed_precondition(
                "relay loop or max hops exceeded",
            ));
        }

        let permit = self
            .state
            .sem
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("too many relay sessions"))?;
        let guard = self.state.active_guard();
        let (out_tx, out_rx) = mpsc::channel::<Result<RelayPacket, Status>>(64);
        let frame = from_relay_server(inbound, out_tx.clone());
        let state = self.state.clone();
        let ctx = SessionContext {
            report_traffic: false,
            visited: hello.visited,
            client_addr,
            session_id: Uuid::new_v4().to_string(),
            started_at_ms: now_ms(),
        };
        let header = hello.vless_header;
        tokio::spawn(async move {
            let _permit = permit;
            let _guard = guard;
            run_session(
                frame,
                parsed,
                header,
                ctx,
                state,
                SessionMode::RpcRelay(out_tx),
            )
            .await;
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(out_rx))))
    }
}
